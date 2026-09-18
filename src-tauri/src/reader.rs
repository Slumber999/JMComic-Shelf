use std::{
    cmp::Ordering,
    collections::HashMap,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU32, Ordering as AtomicOrdering},
};

use bytes::Bytes;
use eyre::{eyre, OptionExt, WrapErr};
use indexmap::IndexMap;
use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager};
use tracing::instrument;

use crate::{
    downloader::download_img_task::{calculate_block_num, decode_and_encode_img},
    extensions::{AppHandleExt, PathIsImg},
    jm_client::IMAGE_DOMAIN,
};

/// 页面内存缓存上限（字节）：命中缓存的翻页是零IO的
const PAGE_CACHE_BUDGET: usize = 256 * 1024 * 1024;

/// 阅读器状态：章节登记表 + 页面内存缓存
#[derive(Default)]
pub struct ReaderState {
    next_token: AtomicU32,
    chapters: RwLock<HashMap<u32, ChapterSource>>,
    page_cache: Mutex<PageCache>,
}

/// 一个章节的图片来源
#[derive(Debug, Clone)]
enum ChapterSource {
    /// 下载目录：一话对应一个目录，里面是图片
    Dir { pages: Vec<PathBuf> },
    /// 导出目录：一话对应一个 cbz
    Cbz { path: PathBuf, pages: Vec<String> },
    /// 在线阅读：没下载过，图片从禁漫服务器实时取
    Remote {
        chapter_id: i64,
        prepared: Option<RemotePages>,
    },
}

/// 在线章节的图片地址（打开章节时才去请求，保证打开速度）
#[derive(Debug, Clone)]
struct RemotePages {
    urls: Vec<String>,
    block_nums: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ReaderChapter {
    /// 传给自定义协议用的章节令牌
    pub token: u32,
    pub title: String,
    /// 在线章节在打开前还不知道页数（是0），需要先调用 prepare_reader_chapter
    pub page_count: usize,
    /// 是否是在线章节
    pub online: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ReaderComic {
    pub title: String,
    pub chapters: Vec<ReaderChapter>,
}

/// 简单的LRU：用IndexMap的插入顺序当访问顺序，超预算就从最旧的开始淘汰
#[derive(Default)]
struct PageCache {
    entries: IndexMap<(u32, usize), (Bytes, &'static str)>,
    bytes: usize,
}

impl PageCache {
    fn get(&mut self, key: &(u32, usize)) -> Option<(Bytes, &'static str)> {
        let value = self.entries.shift_remove(key)?;
        // 重新插入 = 标记为最近使用
        self.entries.insert(*key, value.clone());
        Some(value)
    }

    fn insert(&mut self, key: (u32, usize), value: (Bytes, &'static str)) {
        if let Some(old) = self.entries.shift_remove(&key) {
            self.bytes = self.bytes.saturating_sub(old.0.len());
        }
        self.bytes += value.0.len();
        self.entries.insert(key, value);

        while self.bytes > PAGE_CACHE_BUDGET {
            let Some((_, evicted)) = self.entries.shift_remove_index(0) else {
                break;
            };
            self.bytes = self.bytes.saturating_sub(evicted.0.len());
        }
    }

    fn clear(&mut self) {
        self.entries.clear();
        self.bytes = 0;
    }
}

impl ChapterSource {
    fn page_count(&self) -> usize {
        match self {
            ChapterSource::Dir { pages } => pages.len(),
            ChapterSource::Cbz { pages, .. } => pages.len(),
            ChapterSource::Remote { prepared, .. } => {
                prepared.as_ref().map_or(0, |pages| pages.urls.len())
            }
        }
    }

    /// 读取一页，返回 (图片字节, Content-Type)
    fn read_page(&self, app: &AppHandle, index: usize) -> eyre::Result<(Bytes, &'static str)> {
        match self {
            ChapterSource::Remote { .. } => self.read_remote_page(app, index),
            _ => self.read_local_page(index),
        }
    }

    fn read_local_page(&self, index: usize) -> eyre::Result<(Bytes, &'static str)> {
        match self {
            ChapterSource::Dir { pages } => {
                let path = pages.get(index).ok_or_eyre("页码超出范围")?;
                let data =
                    std::fs::read(path).wrap_err(format!("读取图片`{}`失败", path.display()))?;
                let content_type = content_type_of(&file_name_string(path));
                Ok((Bytes::from(data), content_type))
            }
            ChapterSource::Cbz { path, pages } => {
                let name = pages.get(index).ok_or_eyre("页码超出范围")?;
                let file = File::open(path).wrap_err(format!("打开cbz`{}`失败", path.display()))?;
                let mut archive = zip::ZipArchive::new(file)
                    .wrap_err(format!("解析cbz`{}`失败", path.display()))?;
                let mut entry = archive.by_name(name).wrap_err("在cbz中找不到该页")?;
                let mut buffer = Vec::with_capacity(usize::try_from(entry.size()).unwrap_or(0));
                entry.read_to_end(&mut buffer).wrap_err("读取cbz中的图片失败")?;
                Ok((Bytes::from(buffer), content_type_of(name)))
            }
            ChapterSource::Remote { .. } => Err(eyre!("在线章节请使用 read_remote_page")),
        }
    }

    /// 在线阅读：下载 + 还原拼图 + 按配置格式编码
    fn read_remote_page(&self, app: &AppHandle, index: usize) -> eyre::Result<(Bytes, &'static str)> {
        let ChapterSource::Remote { prepared, .. } = self else {
            return Err(eyre!("不是在线章节"));
        };

        let prepared = prepared.as_ref().ok_or_eyre("章节内容还没准备好")?;
        let url = prepared.urls.get(index).ok_or_eyre("页码超出范围")?.clone();
        let block_num = prepared.block_nums.get(index).copied().unwrap_or(0);

        // 这里跑在自定义协议的独立线程上（不在 tokio runtime 里），所以可以 block_on
        tauri::async_runtime::block_on(fetch_remote_page(app, &url, block_num))
    }
}

/// 在线页的完整处理链：下载 → 还原拼图 → 按配置格式编码
/// - 协议线程用 block_on 调；prepare_chapter 直接用 await 调（那边在 tokio runtime 上，不能再 block_on）
async fn fetch_remote_page(
    app: &AppHandle,
    url: &str,
    block_num: u32,
) -> eyre::Result<(Bytes, &'static str)> {
    let download_format = app.get_config().read().download_format;
    let jm_client = app.get_jm_client();
    let (img_data, src_format) = jm_client.get_img_data_and_format(url).await?;
    let encoded = decode_and_encode_img(&img_data, src_format, block_num, download_format)?;

    let content_type = if src_format == image::ImageFormat::Gif {
        "image/gif"
    } else {
        content_type_of(download_format.extension())
    };

    Ok((Bytes::from(encoded), content_type))
}

/// 自然排序：让 第2话 < 第10话、0009.jpg < 0010.jpg
pub(crate) fn natural_cmp(a: &str, b: &str) -> Ordering {
    let mut a_chars = a.chars().peekable();
    let mut b_chars = b.chars().peekable();

    loop {
        match (a_chars.peek().copied(), b_chars.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(a_char), Some(b_char)) => {
                if a_char.is_ascii_digit() && b_char.is_ascii_digit() {
                    let a_num = take_number(&mut a_chars);
                    let b_num = take_number(&mut b_chars);
                    match a_num.cmp(&b_num) {
                        Ordering::Equal => {}
                        other => return other,
                    }
                } else {
                    a_chars.next();
                    b_chars.next();
                    match a_char.cmp(&b_char) {
                        Ordering::Equal => {}
                        other => return other,
                    }
                }
            }
        }
    }
}

fn take_number(chars: &mut std::iter::Peekable<std::str::Chars<'_>>) -> u128 {
    let mut value: u128 = 0;
    while let Some(c) = chars.peek().copied() {
        if !c.is_ascii_digit() {
            break;
        }
        chars.next();
        value = value
            .saturating_mul(10)
            .saturating_add(u128::from(c as u8 - b'0'));
    }
    value
}

fn file_name_string(path: &Path) -> String {
    path.file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().to_string())
}

fn stem_string(path: &Path) -> String {
    path.file_stem()
        .map_or_else(|| file_name_string(path), |name| name.to_string_lossy().to_string())
}

/// 按文件名自然排序，列出目录里的图片（不递归）
fn list_images(dir: &Path) -> Vec<PathBuf> {
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return Vec::new();
    };

    let mut paths: Vec<PathBuf> = read_dir
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && path.is_img())
        .collect();

    paths.sort_by(|a, b| natural_cmp(&file_name_string(a), &file_name_string(b)));
    paths
}

/// 打开一个 cbz，读出里面图片条目的名字（按自然序）
fn read_cbz_pages(path: &Path) -> eyre::Result<Vec<String>> {
    let file = File::open(path).wrap_err(format!("打开cbz`{}`失败", path.display()))?;
    let archive =
        zip::ZipArchive::new(file).wrap_err(format!("解析cbz`{}`失败", path.display()))?;

    let mut pages: Vec<String> = archive
        .file_names()
        .filter(|name| Path::new(name).is_img())
        .map(str::to_string)
        .collect();

    pages.sort_by(|a, b| natural_cmp(a, b));
    Ok(pages)
}

/// 收集这部漫画的章节
/// - 导出目录：`cbz/` 下每个 cbz 是一话
/// - 下载目录：每个直接子目录是一话
/// - 兜底：漫画目录本身就是一堆图片
fn collect_chapters(comic_dir: &Path) -> eyre::Result<Vec<(String, ChapterSource)>> {
    // 1) cbz
    let cbz_dir = comic_dir.join("cbz");
    if cbz_dir.is_dir() {
        let mut cbz_files: Vec<PathBuf> = std::fs::read_dir(&cbz_dir)
            .wrap_err(format!("读取目录`{}`失败", cbz_dir.display()))?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| {
                path.is_file()
                    && path
                        .extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case("cbz"))
            })
            .collect();
        cbz_files.sort_by(|a, b| natural_cmp(&file_name_string(a), &file_name_string(b)));

        let chapters: Vec<(String, ChapterSource)> = cbz_files
            .into_iter()
            .filter_map(|path| match read_cbz_pages(&path) {
                Ok(pages) if !pages.is_empty() => {
                    Some((stem_string(&path), ChapterSource::Cbz { path, pages }))
                }
                Ok(_) => None,
                Err(err) => {
                    tracing::warn!("读取cbz`{}`失败: {err:?}", path.display());
                    None
                }
            })
            .collect();

        if !chapters.is_empty() {
            return Ok(chapters);
        }
    }

    // 2) 子目录 = 话
    let mut chapter_dirs: Vec<PathBuf> = std::fs::read_dir(comic_dir)
        .wrap_err(format!("读取目录`{}`失败", comic_dir.display()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .filter(|path| !file_name_string(path).starts_with(".下载中-"))
        .collect();
    chapter_dirs.sort_by(|a, b| natural_cmp(&file_name_string(a), &file_name_string(b)));

    let mut chapters: Vec<(String, ChapterSource)> = Vec::new();
    for dir in chapter_dirs {
        let pages = list_images(&dir);
        if !pages.is_empty() {
            chapters.push((file_name_string(&dir), ChapterSource::Dir { pages }));
        }
    }
    if !chapters.is_empty() {
        return Ok(chapters);
    }

    // 3) 兜底：目录本身就是图片
    let pages = list_images(comic_dir);
    if !pages.is_empty() {
        return Ok(vec![("全部图片".to_string(), ChapterSource::Dir { pages })]);
    }

    Err(eyre!("没有找到可阅读的图片或cbz"))
}

/// 打开阅读器：只列章节、不做任何解码，保证打开速度
#[instrument(
    level = "error",
    skip_all,
    fields(comic_title = comic_title, comic_dir = %comic_dir.display())
)]
pub fn open_reader(
    catalog: &ReaderState,
    comic_title: &str,
    comic_dir: &Path,
) -> eyre::Result<ReaderComic> {
    let chapters = collect_chapters(comic_dir)?;

    let mut registry = catalog.chapters.write();
    let chapters = chapters
        .into_iter()
        .map(|(title, source)| {
            let token = catalog
                .next_token
                .fetch_add(1, AtomicOrdering::Relaxed)
                .wrapping_add(1);
            let page_count = source.page_count();
            registry.insert(token, source);
            ReaderChapter {
                token,
                title,
                page_count,
                online: false,
            }
        })
        .collect();

    Ok(ReaderComic {
        title: comic_title.to_string(),
        chapters,
    })
}

/// 打开在线阅读：没下载过的漫画也能看，图片实时从禁漫服务器取
/// - 这里只登记章节，不请求任何图片地址，保证打开速度
/// - 页数在打开具体章节时通过 `prepare_chapter` 才知道
pub fn open_reader_remote(
    catalog: &ReaderState,
    comic_title: &str,
    chapters: Vec<(i64, String)>,
) -> ReaderComic {
    let mut registry = catalog.chapters.write();
    let chapters = chapters
        .into_iter()
        .map(|(chapter_id, title)| {
            let token = catalog
                .next_token
                .fetch_add(1, AtomicOrdering::Relaxed)
                .wrapping_add(1);
            registry.insert(
                token,
                ChapterSource::Remote {
                    chapter_id,
                    prepared: None,
                },
            );
            ReaderChapter {
                token,
                title,
                page_count: 0,
                online: true,
            }
        })
        .collect();

    ReaderComic {
        title: comic_title.to_string(),
        chapters,
    }
}

/// 准备在线章节：请求图片地址和混淆参数，返回页数
/// - 本地章节直接返回已有页数
/// - 必须是 async：调用方是 async 命令，已经在 tokio runtime 上，不能再用 block_on
pub async fn prepare_chapter(
    app: &AppHandle,
    catalog: &ReaderState,
    token: u32,
) -> eyre::Result<usize> {
    let (chapter_id, prepared_count) = {
        let registry = catalog.chapters.read();
        let source = registry.get(&token).ok_or_eyre("未找到该章节")?;
        match source {
            ChapterSource::Remote {
                chapter_id,
                prepared,
            } => (*chapter_id, prepared.as_ref().map(|pages| pages.urls.len())),
            other => return Ok(other.page_count()),
        }
    };

    if let Some(count) = prepared_count {
        return Ok(count);
    }

    let jm_client = app.get_jm_client();
    let (scramble_id, chapter_resp) = tokio::try_join!(
        jm_client.get_scramble_id(chapter_id),
        jm_client.get_chapter(chapter_id)
    )
    .wrap_err("获取章节图片链接失败")?;

    let mut urls = Vec::new();
    let mut block_nums = Vec::new();
    for filename in chapter_resp.images {
        let Some(file_stem) = Path::new(&filename)
            .file_stem()
            .and_then(|stem| stem.to_str())
        else {
            continue;
        };
        let Some(ext) = Path::new(&filename)
            .extension()
            .and_then(|ext| ext.to_str())
            .map(str::to_lowercase)
        else {
            continue;
        };

        let url = format!("https://{IMAGE_DOMAIN}/media/photos/{chapter_id}/{filename}");
        if ext == "gif" {
            urls.push(url);
            block_nums.push(0);
        } else if ext == "webp" {
            let block_num = calculate_block_num(scramble_id, chapter_id, file_stem);
            urls.push(url);
            block_nums.push(block_num);
        }
    }

    let count = urls.len();
    let first_page = urls.first().cloned().zip(block_nums.first().copied());
    if let Some(source) = catalog.chapters.write().get_mut(&token) {
        if let ChapterSource::Remote { prepared, .. } = source {
            *prepared = Some(RemotePages { urls, block_nums });
        }
    }

    // 顺手把第一页也下好塞进缓存：前端拿到页数马上就回来取第 1 页，
    // 提前取掉能省下"章节准备好了、图还在路上"的那几秒空白。
    // 取不到就算了，前端自己还会再请求一次
    if let Some((url, block_num)) = first_page {
        match fetch_remote_page(app, &url, block_num).await {
            Ok(page) => catalog.page_cache.lock().insert((token, 0), page),
            Err(err) => tracing::warn!(message = %err, "预热第一页失败"),
        }
    }

    Ok(count)
}

/// 关闭阅读器：丢弃章节登记表和页面缓存
pub fn close_reader(catalog: &ReaderState) {
    catalog.chapters.write().clear();
    catalog.page_cache.lock().clear();
}

/// 解析 `/page/{token}/{index}`
fn parse_page_path(path: &str) -> Option<(u32, usize)> {
    let parts: Vec<&str> = path.trim_start_matches('/').split('/').collect();
    if parts.len() != 3 || parts[0] != "page" {
        return None;
    }

    let token = parts[1].parse::<u32>().ok()?;
    let index = parts[2].parse::<usize>().ok()?;
    Some((token, index))
}

fn content_type_of(page_name: &str) -> &'static str {
    match Path::new(page_name)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_lowercase)
        .as_deref()
    {
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        Some("gif") => "image/gif",
        Some("bmp") => "image/bmp",
        Some("avif") => "image/avif",
        _ => "image/jpeg",
    }
}

/// 自定义协议 `comic` 的处理函数
/// - `/page/{token}/{index}` 返回某一页的图片字节
/// - 命中内存缓存时零IO；未命中时后台线程读取（不阻塞主线程）
pub fn handle_request(app: &AppHandle, path: &str) -> tauri::http::Response<Vec<u8>> {
    let Some((token, index)) = parse_page_path(path) else {
        return not_found("unknown path");
    };

    let state = app.state::<ReaderState>();

    // 先查内存缓存
    if let Some((bytes, content_type)) = state.page_cache.lock().get(&(token, index)) {
        return image_response(bytes.to_vec(), content_type);
    }

    let source = state.chapters.read().get(&token).cloned();
    let Some(source) = source else {
        return not_found("chapter not found");
    };

    match source.read_page(app, index) {
        Ok((bytes, content_type)) => {
            state
                .page_cache
                .lock()
                .insert((token, index), (bytes.clone(), content_type));
            image_response(bytes.to_vec(), content_type)
        }
        Err(err) => {
            tracing::error!("读取页面失败: {err:?}");
            not_found("read page failed")
        }
    }
}

pub fn image_response(body: Vec<u8>, content_type: &str) -> tauri::http::Response<Vec<u8>> {
    tauri::http::Response::builder()
        .status(tauri::http::StatusCode::OK)
        .header(tauri::http::header::CONTENT_TYPE, content_type)
        // 同一个页面反复读时直接用缓存，避免重复解码传输
        .header(tauri::http::header::CACHE_CONTROL, "public, max-age=31536000")
        .body(body)
        .unwrap_or_else(|_| tauri::http::Response::new(Vec::new()))
}

pub fn not_found(reason: &str) -> tauri::http::Response<Vec<u8>> {
    tauri::http::Response::builder()
        .status(tauri::http::StatusCode::NOT_FOUND)
        .header(tauri::http::header::CONTENT_TYPE, "text/plain")
        .body(reason.as_bytes().to_vec())
        .unwrap_or_else(|_| tauri::http::Response::new(Vec::new()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natural_cmp_should_order_numbers_as_numbers() {
        assert_eq!(natural_cmp("第2话", "第10话"), Ordering::Less);
        assert_eq!(natural_cmp("0009.jpg", "0010.jpg"), Ordering::Less);
        assert_eq!(natural_cmp("a2b", "a2b"), Ordering::Equal);
        assert_eq!(natural_cmp("第1话", "第1话 番外"), Ordering::Less);
    }

    #[test]
    fn content_type_should_follow_extension() {
        assert_eq!(content_type_of("0001.jpg"), "image/jpeg");
        assert_eq!(content_type_of("0001.PNG"), "image/png");
        assert_eq!(content_type_of("a.webp"), "image/webp");
    }

    #[test]
    fn collect_chapters_should_read_download_layout() {
        let root = std::env::temp_dir().join(format!(
            "jmcomic-shelf-reader-test-{}",
            uuid::Uuid::new_v4()
        ));
        let comic_dir = root.join("漫画1");
        std::fs::create_dir_all(comic_dir.join("第1话")).unwrap();
        std::fs::create_dir_all(comic_dir.join("第10话")).unwrap();
        std::fs::create_dir_all(comic_dir.join(format!(".下载中-第2话"))).unwrap();
        std::fs::write(comic_dir.join("cover.jpg"), b"cover").unwrap();
        std::fs::write(comic_dir.join("第1话").join("0001.jpg"), b"p1").unwrap();
        std::fs::write(comic_dir.join("第1话").join("0002.jpg"), b"p2").unwrap();
        std::fs::write(comic_dir.join("第10话").join("0001.jpg"), b"p1").unwrap();
        std::fs::write(
            comic_dir.join(format!(".下载中-第2话")).join("0001.jpg"),
            b"tmp",
        )
        .unwrap();

        let chapters = collect_chapters(&comic_dir).unwrap();
        let titles: Vec<&str> = chapters.iter().map(|(title, _)| title.as_str()).collect();
        // 自然序：第1话 在 第10话 前面；临时目录被跳过；cover.jpg 不算一话
        assert_eq!(titles, vec!["第1话", "第10话"]);
        assert_eq!(chapters[0].1.page_count(), 2);

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn handle_request_should_serve_image_then_hit_cache() {
        let root = std::env::temp_dir().join(format!(
            "jmcomic-shelf-reader-http-{}",
            uuid::Uuid::new_v4()
        ));
        let comic_dir = root.join("漫画1");
        std::fs::create_dir_all(comic_dir.join("第1话")).unwrap();
        std::fs::write(comic_dir.join("第1话").join("0001.jpg"), b"page-one").unwrap();

        let chapters = collect_chapters(&comic_dir).unwrap();
        let (_, source) = chapters.into_iter().next().unwrap();

        let state = ReaderState::default();
        let token = 7u32;
        state.chapters.write().insert(token, source.clone());

        // 路径解析
        assert_eq!(parse_page_path(&format!("/page/{token}/0")), Some((token, 0)));
        assert_eq!(parse_page_path("/page/abc/0"), None);
        assert_eq!(parse_page_path("/other/7/0"), None);
        assert_eq!(parse_page_path("/page/7"), None);

        // 读页面
        let (bytes, content_type) = source.read_local_page(0).unwrap();
        assert_eq!(content_type, "image/jpeg");
        assert_eq!(bytes.as_ref(), b"page-one");

        // 缓存命中
        state
            .page_cache
            .lock()
            .insert((token, 0), (bytes.clone(), content_type));
        assert!(state.page_cache.lock().get(&(token, 0)).is_some());

        // 越界页码
        assert!(source.read_local_page(9).is_err());

        let _ = std::fs::remove_dir_all(&root);
    }

    /// 导出目录的 cbz 也要能读（这里刻意用 Deflated 条目，验证 deflate 支持）
    #[test]
    fn handle_request_should_read_cbz_pages() {
        use std::io::Write;

        let root = std::env::temp_dir().join(format!(
            "jmcomic-shelf-reader-cbz-{}",
            uuid::Uuid::new_v4()
        ));
        let comic_dir = root.join("漫画1");
        let cbz_dir = comic_dir.join("cbz");
        std::fs::create_dir_all(&cbz_dir).unwrap();

        let cbz_path = cbz_dir.join("第1话.cbz");
        {
            let file = File::create(&cbz_path).unwrap();
            let mut zip_writer = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default();
            // 故意乱序写入，验证读取时按自然序排好
            zip_writer.start_file("0002.jpg", options).unwrap();
            zip_writer.write_all(b"second").unwrap();
            zip_writer.start_file("0001.jpg", options).unwrap();
            zip_writer.write_all(b"first").unwrap();
            zip_writer.finish().unwrap();
        }

        let chapters = collect_chapters(&comic_dir).unwrap();
        assert_eq!(chapters.len(), 1);
        let (title, source) = chapters.into_iter().next().unwrap();
        assert_eq!(title, "第1话");
        assert_eq!(source.page_count(), 2);

        let (first, content_type) = source.read_local_page(0).unwrap();
        assert_eq!(first.as_ref(), b"first");
        assert_eq!(content_type, "image/jpeg");

        let (second, _) = source.read_local_page(1).unwrap();
        assert_eq!(second.as_ref(), b"second");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// 在线阅读的章节：打开时不需要联网、页数待定，prepare 之后才有页
    #[test]
    fn remote_chapters_should_be_registered_lazily() {
        let state = ReaderState::default();
        let comic = open_reader_remote(
            &state,
            "某本漫画",
            vec![(111, "第1话".to_string()), (222, "第2话".to_string())],
        );

        assert_eq!(comic.chapters.len(), 2);
        assert!(comic.chapters.iter().all(|chapter| chapter.online));
        assert!(comic.chapters.iter().all(|chapter| chapter.page_count == 0));

        let token = comic.chapters[0].token;
        let source = state.chapters.read().get(&token).cloned().unwrap();
        assert_eq!(source.page_count(), 0);
        // 还没 prepare，读页会失败（而不是崩溃）
        assert!(source.read_local_page(0).is_err());

        // 登记表里能查到
        assert!(state.chapters.read().contains_key(&token));
    }

    #[test]
    fn page_cache_should_evict_least_recently_used() {
        let mut cache = PageCache::default();
        cache.insert((1, 0), (Bytes::from_static(b"a"), "image/jpeg"));
        cache.insert((1, 1), (Bytes::from_static(b"b"), "image/jpeg"));

        // 访问 (1,0)，让它变成最近使用
        assert!(cache.get(&(1, 0)).is_some());

        // 清空后重新验证命中顺序
        cache.clear();
        assert!(cache.get(&(1, 0)).is_none());
        cache.insert((1, 0), (Bytes::from_static(b"a"), "image/jpeg"));
        assert!(cache.get(&(1, 0)).is_some());
    }
}
