use std::{
    fs,
    path::{Path, PathBuf},
};

use eyre::{eyre, WrapErr};
use serde::{Deserialize, Serialize};
use tauri::AppHandle;
use tauri_specta::Event;
use tracing::instrument;

use crate::{
    config::LocalLibrarySource,
    events::ExportQuickReaderEvent,
    extensions::{AppHandleExt, PathIsImg},
    reader::natural_cmp,
    utils,
};

/// 默认的阅读器文件夹名
pub const DEFAULT_READER_DIR_NAME: &str = "快速阅读器";
/// 判断某个目录是不是之前导出的阅读器（用于扫描时跳过）
const READER_HTML_FILE: &str = "index.html";
const READER_GUIDE_FILE: &str = "使用说明.txt";

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Manifest {
    title: String,
    generated_at: String,
    comics: Vec<ManifestComic>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ManifestComic {
    name: String,
    cover: Option<String>,
    chapters: Vec<ManifestChapter>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ManifestChapter {
    title: String,
    pages: Vec<String>,
}

/// 可以导出进阅读器的漫画（给前端勾选用）
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct QuickReaderCandidate {
    /// 相对来源目录的路径，导出时回传
    pub key: String,
    /// 展示用的漫画名（目录名）
    pub name: String,
    pub chapter_count: usize,
    pub page_count: usize,
    /// 估算体积（字节），用于提示单文件会不会太大
    pub estimated_bytes: u64,
    /// 图片来源：图片目录 / cbz
    pub kind: String,
}

/// 一个章节的图片来源
enum ChapterSource {
    /// 一堆图片文件
    Images(Vec<PathBuf>),
    /// 一个 cbz（导出时解压成图片目录，浏览器才能直接读）
    Cbz(PathBuf),
}

struct ChapterJob {
    title: String,
    source: ChapterSource,
}

struct ComicJob {
    /// 相对来源目录的路径
    key: String,
    name: String,
    cover: Option<PathBuf>,
    chapters: Vec<ChapterJob>,
}

impl ChapterSource {
    /// 顺序取出这一章的所有图片：(文件名, 字节)
    fn read_all_images(&self) -> eyre::Result<Vec<(String, Vec<u8>)>> {
        match self {
            ChapterSource::Images(paths) => {
                let mut images = Vec::with_capacity(paths.len());
                for path in paths {
                    let data = fs::read(path)
                        .wrap_err(format!("读取图片`{}`失败", path.display()))?;
                    images.push((file_name_string(path), data));
                }
                Ok(images)
            }
            ChapterSource::Cbz(path) => {
                // 同样按自然序取页
                let names = cbz_image_names(path)?;
                let file = fs::File::open(path)
                    .wrap_err(format!("打开cbz`{}`失败", path.display()))?;
                let mut archive = zip::ZipArchive::new(file)
                    .wrap_err(format!("解析cbz`{}`失败", path.display()))?;

                let mut images = Vec::with_capacity(names.len());
                for name in names {
                    let Ok(mut entry) = archive.by_name(&name) else {
                        continue;
                    };
                    let mut buffer = Vec::with_capacity(usize::try_from(entry.size()).unwrap_or(0));
                    if std::io::Read::read_to_end(&mut entry, &mut buffer).is_err() {
                        continue;
                    }
                    images.push((safe_name(&file_name_string(Path::new(&name))), buffer));
                }
                Ok(images)
            }
        }
    }

    /// 估算解压/复制后的体积（用于前端提示"单文件会不会太大"）
    fn estimated_bytes(&self) -> u64 {
        match self {
            ChapterSource::Images(paths) => paths
                .iter()
                .filter_map(|path| fs::metadata(path).ok())
                .map(|metadata| metadata.len())
                .sum(),
            ChapterSource::Cbz(path) => {
                let Ok(file) = fs::File::open(path) else {
                    return 0;
                };
                let Ok(mut archive) = zip::ZipArchive::new(file) else {
                    return 0;
                };

                let mut total = 0u64;
                for index in 0..archive.len() {
                    let Ok(entry) = archive.by_index(index) else {
                        continue;
                    };
                    if entry.is_file() && Path::new(entry.name()).is_img() {
                        total += entry.size();
                    }
                }
                total
            }
        }
    }
}

impl ComicJob {
    fn estimated_bytes(&self) -> u64 {
        self.chapters
            .iter()
            .map(|chapter| chapter.source.estimated_bytes())
            .sum()
    }

    fn file_count(&self) -> usize {
        self.chapters
            .iter()
            .map(|chapter| match &chapter.source {
                ChapterSource::Images(paths) => paths.len(),
                ChapterSource::Cbz(path) => cbz_image_count(path).unwrap_or(0),
            })
            .sum()
    }

    fn kind(&self) -> &'static str {
        let has_cbz = self
            .chapters
            .iter()
            .any(|chapter| matches!(chapter.source, ChapterSource::Cbz(_)));
        if has_cbz {
            "cbz"
        } else {
            "图片目录"
        }
    }
}

/// 复制/解压到输出目录时用到的安全名字（去掉会破坏相对URL的字符）
fn safe_name(name: &str) -> String {
    let filtered = utils::filename_filter(name);
    let sanitized: String = filtered
        .chars()
        .map(|c| match c {
            '#' | '?' | '%' | '&' | '+' | '\\' => '_',
            _ => c,
        })
        .collect();

    let trimmed = sanitized.trim().trim_matches('.').trim().to_string();
    if trimmed.is_empty() {
        "未命名".to_string()
    } else {
        trimmed
    }
}

/// 阅读器文件夹名：只把系统不允许的字符换成下划线，尽量保持用户输入的原样
/// （不能复用下载目录那套命名规则，否则 / 会变空格、* 会变 ⭐，用户会一脸问号）
fn safe_dir_name(name: &str) -> String {
    let sanitized: String = name
        .chars()
        .map(|c| match c {
            '\\' | '/' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
            c if c.is_control() => '_',
            _ => c,
        })
        .collect();

    let trimmed = sanitized.trim().trim_end_matches('.').trim().to_string();
    if trimmed.is_empty() {
        "未命名".to_string()
    } else {
        trimmed
    }
}

fn cbz_image_names(path: &Path) -> eyre::Result<Vec<String>> {
    let file = fs::File::open(path).wrap_err(format!("打开cbz`{}`失败", path.display()))?;
    let archive =
        zip::ZipArchive::new(file).wrap_err(format!("解析cbz`{}`失败", path.display()))?;

    let mut names: Vec<String> = archive
        .file_names()
        .filter(|name| Path::new(name).is_img())
        .map(str::to_string)
        .collect();
    names.sort_by(|a, b| natural_cmp(a, b));
    Ok(names)
}

fn cbz_image_count(path: &Path) -> Option<usize> {
    cbz_image_names(path).ok().map(|names| names.len())
}

fn list_images(dir: &Path) -> Vec<PathBuf> {
    let Ok(read_dir) = fs::read_dir(dir) else {
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

fn file_name_string(path: &Path) -> String {
    path.file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().to_string())
}

fn stem_string(path: &Path) -> String {
    path.file_stem()
        .map_or_else(|| file_name_string(path), |name| name.to_string_lossy().to_string())
}

fn subdirs(dir: &Path) -> Vec<PathBuf> {
    let Ok(read_dir) = fs::read_dir(dir) else {
        return Vec::new();
    };

    let mut dirs: Vec<PathBuf> = read_dir
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .filter(|path| !file_name_string(path).starts_with(".下载中-"))
        .collect();
    dirs.sort_by(|a, b| natural_cmp(&file_name_string(a), &file_name_string(b)));
    dirs
}

fn cbz_files(cbz_dir: &Path) -> Vec<PathBuf> {
    let Ok(read_dir) = fs::read_dir(cbz_dir) else {
        return Vec::new();
    };

    let mut files: Vec<PathBuf> = read_dir
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_file() && path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("cbz"))
        })
        .collect();
    files.sort_by(|a, b| natural_cmp(&file_name_string(a), &file_name_string(b)));
    files
}

/// 扫描一部漫画目录，得到它的章节
fn scan_comic(dir: &Path, key: String) -> Option<ComicJob> {
    let name = safe_name(&file_name_string(dir));
    let mut chapters: Vec<ChapterJob> = Vec::new();
    let mut cover: Option<PathBuf> = None;

    // 1) cbz 目录
    let cbz_dir = dir.join("cbz");
    if cbz_dir.is_dir() {
        for path in cbz_files(&cbz_dir) {
            if matches!(cbz_image_count(&path), Some(count) if count > 0) {
                chapters.push(ChapterJob {
                    title: safe_name(&stem_string(&path)),
                    source: ChapterSource::Cbz(path),
                });
            }
        }

        let cbz_cover = cbz_dir.join("cover.jpg");
        if cbz_cover.is_file() {
            cover = Some(cbz_cover);
        }
    }

    // 2) 子目录 = 每一话
    if chapters.is_empty() {
        for chapter_dir in subdirs(dir) {
            let images = list_images(&chapter_dir);
            if !images.is_empty() {
                chapters.push(ChapterJob {
                    title: safe_name(&file_name_string(&chapter_dir)),
                    source: ChapterSource::Images(images),
                });
            }
        }
    }

    // 3) 兜底：目录本身就是一堆图片（没有子目录的情况）
    if chapters.is_empty() && subdirs(dir).is_empty() {
        let images = list_images(dir);
        if !images.is_empty() {
            chapters.push(ChapterJob {
                title: "全部图片".to_string(),
                source: ChapterSource::Images(images),
            });
        }
    }

    if chapters.is_empty() {
        return None;
    }

    if cover.is_none() {
        let dir_cover = dir.join("cover.jpg");
        if dir_cover.is_file() {
            cover = Some(dir_cover);
        }
    }

    Some(ComicJob {
        key,
        name,
        cover,
        chapters,
    })
}

/// 是不是上一次导出的阅读器目录（扫描时跳过）
fn is_quick_reader_dir(dir: &Path) -> bool {
    dir.join(READER_HTML_FILE).is_file() && dir.join(READER_GUIDE_FILE).is_file()
}

/// 扫描来源目录里所有可导出的漫画
/// - 兼容两种布局：漫画直接在根目录下（默认目录格式）、或多一层作者目录
fn scan_all_comics(source_dir: &Path) -> eyre::Result<Vec<ComicJob>> {
    let mut comics: Vec<ComicJob> = Vec::new();

    for entry in subdirs(source_dir) {
        if is_quick_reader_dir(&entry) {
            continue;
        }

        let key = file_name_string(&entry);
        if let Some(comic) = scan_comic(&entry, key.clone()) {
            comics.push(comic);
            continue;
        }

        // 这一层不是漫画，看看下一层（例如 {author}/{comic_title} 的布局）
        for sub in subdirs(&entry) {
            let sub_key = format!("{key}/{}", file_name_string(&sub));
            if let Some(comic) = scan_comic(&sub, sub_key) {
                comics.push(comic);
            }
        }
    }

    Ok(comics)
}

/// 列出可以导出的漫画（给前端勾选）
pub fn list_candidates(
    app: &AppHandle,
    source: LocalLibrarySource,
) -> eyre::Result<Vec<QuickReaderCandidate>> {
    let source_dir = source_dir_of(app, source)?;
    let comics = scan_all_comics(&source_dir)?;

    Ok(comics
        .into_iter()
        .map(|comic| {
            let page_count = comic.file_count();
            let chapter_count = comic.chapters.len();
            let estimated_bytes = comic.estimated_bytes();
            let kind = comic.kind().to_string();

            QuickReaderCandidate {
                key: comic.key,
                name: comic.name,
                chapter_count,
                page_count,
                estimated_bytes,
                kind,
            }
        })
        .collect())
}

fn source_dir_of(app: &AppHandle, source: LocalLibrarySource) -> eyre::Result<PathBuf> {
    let (export_dir, download_dir) = {
        let config = app.get_config();
        let config = config.read();
        (config.export_dir.clone(), config.download_dir.clone())
    };

    let source_dir = match source {
        LocalLibrarySource::DownloadDir => download_dir,
        LocalLibrarySource::ExportDir => export_dir,
    };

    if !source_dir.is_dir() {
        return Err(eyre!("目录`{}`不存在", source_dir.display()));
    }

    Ok(source_dir)
}

/// 决定分享包写在哪个目录
/// - 传了自定义目录就用它（不存在会自动创建，必须是绝对路径）
/// - 没传就用来源目录
fn resolve_base_dir(source_dir: &Path, custom_dir: Option<&str>) -> eyre::Result<PathBuf> {
    let Some(custom) = custom_dir.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(source_dir.to_path_buf());
    };

    let path = PathBuf::from(custom);
    if !path.is_absolute() {
        return Err(eyre!("自定义导出目录必须是绝对路径：`{custom}`"));
    }

    fs::create_dir_all(&path).wrap_err(format!("创建目录`{}`失败", path.display()))?;
    Ok(path)
}

/// 目录重名时自动加后缀，保证多个阅读器可以同时存在
fn unique_out_dir(source_dir: &Path, dir_name: &str) -> PathBuf {
    let base = source_dir.join(dir_name);
    if !base.exists() {
        return base;
    }

    for index in 2..1000 {
        let candidate = source_dir.join(format!("{dir_name}-{index}"));
        if !candidate.exists() {
            return candidate;
        }
    }

    base
}

#[instrument(
    level = "error",
    skip_all,
    fields(source = ?source, dir_name = ?dir_name, target_dir = ?target_dir, comics = keys.len())
)]
pub fn export_quick_reader(
    app: &AppHandle,
    source: LocalLibrarySource,
    keys: Vec<String>,
    dir_name: Option<String>,
    target_dir: Option<String>,
) -> eyre::Result<PathBuf> {
    // 从来源目录扫描漫画
    let source_dir = source_dir_of(app, source)?;
    // 写到目标目录（默认就是来源目录）
    let base_dir = resolve_base_dir(&source_dir, target_dir.as_deref())?;

    let dir_name = dir_name
        .map(|name| safe_dir_name(&name))
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| DEFAULT_READER_DIR_NAME.to_string());

    let out_dir = unique_out_dir(&base_dir, &dir_name);

    // 选中了哪些漫画；没选就是全部
    let selected_keys: Vec<String> = keys
        .into_iter()
        .map(|key| key.replace('\\', "/"))
        .collect();

    let comics: Vec<ComicJob> = scan_all_comics(&source_dir)?
        .into_iter()
        .filter(|comic| selected_keys.is_empty() || selected_keys.contains(&comic.key))
        .collect();

    if comics.is_empty() {
        return Err(eyre!("没有选中任何可导出的漫画"));
    }

    let manifest = build_quick_reader(
        &out_dir,
        &comics,
        |total| {
            let _ = ExportQuickReaderEvent::Start { total }.emit(app);
        },
        |current, total, comic, chapter| {
            emit_progress(app, current, total, comic, chapter);
        },
    )?;

    let html = render_html(&manifest)?;
    fs::write(out_dir.join(READER_HTML_FILE), html).wrap_err("写入 index.html 失败")?;
    fs::write(out_dir.join(READER_GUIDE_FILE), READER_GUIDE).wrap_err("写入使用说明失败")?;

    let _ = ExportQuickReaderEvent::End { dir: out_dir.clone() }.emit(app);
    tracing::info!("快速阅读器导出成功: {}", out_dir.display());

    Ok(out_dir)
}

/// 把选中的漫画写进分享包，返回清单
/// - 图片目录直接复制；cbz 解压成图片目录（浏览器从 file:// 打不开 zip）
/// - 事件/进度通过回调上报，方便单测
fn build_quick_reader(
    out_dir: &Path,
    comics: &[ComicJob],
    on_start: impl FnOnce(usize),
    mut on_file: impl FnMut(usize, usize, &str, &str),
) -> eyre::Result<Manifest> {
    // 目标目录应当是唯一的（unique_out_dir 保证），这里再兜一层
    if out_dir.exists() {
        fs::remove_dir_all(out_dir).wrap_err(format!("清理旧的`{}`失败", out_dir.display()))?;
    }
    fs::create_dir_all(out_dir).wrap_err(format!("创建目录`{}`失败", out_dir.display()))?;

    let total: usize = comics.iter().map(ComicJob::file_count).sum();
    on_start(total);

    let mut manifest_comics: Vec<ManifestComic> = Vec::new();
    let mut done = 0usize;
    let mut used_names: Vec<String> = Vec::new();

    for comic in comics {
        // 同名漫画加后缀，避免互相覆盖
        let mut comic_dir_name = safe_name(&comic.name);
        if used_names.contains(&comic_dir_name) {
            let mut index = 2;
            loop {
                let candidate = format!("{}-{index}", safe_name(&comic.name));
                if !used_names.contains(&candidate) {
                    comic_dir_name = candidate;
                    break;
                }
                index += 1;
            }
        }
        used_names.push(comic_dir_name.clone());

        let comic_out_dir = out_dir.join(&comic_dir_name);
        fs::create_dir_all(&comic_out_dir)
            .wrap_err(format!("创建目录`{}`失败", comic_out_dir.display()))?;

        let mut cover_relative: Option<String> = None;
        if let Some(cover_src) = &comic.cover {
            let cover_out = comic_out_dir.join("cover.jpg");
            if fs::copy(cover_src, &cover_out).is_ok() {
                cover_relative = Some(format!("{comic_dir_name}/cover.jpg"));
            }
        }

        let mut manifest_chapters: Vec<ManifestChapter> = Vec::new();
        for chapter in &comic.chapters {
            let chapter_dir_name = safe_name(&chapter.title);
            let chapter_out_dir = comic_out_dir.join(&chapter_dir_name);
            fs::create_dir_all(&chapter_out_dir)
                .wrap_err(format!("创建目录`{}`失败", chapter_out_dir.display()))?;

            let mut pages: Vec<String> = Vec::new();

            match &chapter.source {
                ChapterSource::Images(paths) => {
                    for path in paths {
                        let file_name = file_name_string(path);
                        let dst = chapter_out_dir.join(&file_name);
                        match fs::copy(path, &dst) {
                            Ok(_) => pages.push(format!(
                                "{comic_dir_name}/{chapter_dir_name}/{file_name}"
                            )),
                            Err(err) => {
                                tracing::error!("复制`{}`失败: {err:?}", path.display());
                            }
                        }

                        done += 1;
                        on_file(done, total, &comic_dir_name, &chapter_dir_name);
                    }
                }
                ChapterSource::Cbz(cbz_path) => {
                    // 解压成图片目录，浏览器从 file:// 打开时才能读到
                    // 注意：必须按自然序取页（zip 内部顺序不保证是 0001、0002…）
                    let names = cbz_image_names(cbz_path)?;

                    let file = fs::File::open(cbz_path)
                        .wrap_err(format!("打开cbz`{}`失败", cbz_path.display()))?;
                    let mut archive = zip::ZipArchive::new(file)
                        .wrap_err(format!("解析cbz`{}`失败", cbz_path.display()))?;

                    for name in names {
                        let Ok(mut entry) = archive.by_name(&name) else {
                            continue;
                        };

                        let file_name = safe_name(&file_name_string(Path::new(&name)));
                        let dst = chapter_out_dir.join(&file_name);

                        let mut buffer =
                            Vec::with_capacity(usize::try_from(entry.size()).unwrap_or(0));
                        if std::io::Read::read_to_end(&mut entry, &mut buffer).is_err() {
                            continue;
                        }
                        if fs::write(&dst, &buffer).is_err() {
                            continue;
                        }

                        pages.push(format!("{comic_dir_name}/{chapter_dir_name}/{file_name}"));

                        done += 1;
                        on_file(done, total, &comic_dir_name, &chapter_dir_name);
                    }
                }
            }

            if pages.is_empty() {
                let _ = fs::remove_dir_all(&chapter_out_dir);
                continue;
            }

            manifest_chapters.push(ManifestChapter {
                title: chapter.title.clone(),
                pages,
            });
        }

        if manifest_chapters.is_empty() {
            let _ = fs::remove_dir_all(&comic_out_dir);
            continue;
        }

        manifest_comics.push(ManifestComic {
            name: comic.name.clone(),
            cover: cover_relative,
            chapters: manifest_chapters,
        });
    }

    if manifest_comics.is_empty() {
        let _ = fs::remove_dir_all(out_dir);
        return Err(eyre!("没有可导出的内容"));
    }

    Ok(Manifest {
        title: DEFAULT_READER_DIR_NAME.to_string(),
        generated_at: time_string(),
        comics: manifest_comics,
    })
}

fn mime_of(file_name: &str) -> &'static str {
    match Path::new(file_name)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_lowercase)
        .as_deref()
    {
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        Some("gif") => "image/gif",
        Some("bmp") => "image/bmp",
        _ => "image/jpeg",
    }
}

fn to_data_uri(file_name: &str, bytes: &[u8]) -> String {
    use base64::Engine;
    format!(
        "data:{};base64,{}",
        mime_of(file_name),
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )
}

/// 文件重名时自动加后缀
fn unique_file_path(dir: &Path, base: &str, extension: &str) -> PathBuf {
    let base = safe_dir_name(base);
    let first = dir.join(format!("{base}.{extension}"));
    if !first.exists() {
        return first;
    }

    for index in 2..1000 {
        let candidate = dir.join(format!("{base}-{index}.{extension}"));
        if !candidate.exists() {
            return candidate;
        }
    }

    first
}

/// 导出「单文件 HTML」：图片以 base64 内嵌，一个文件就能读
/// - 手机浏览器禁止 file:// 页面读取子目录图片，只有把图片内嵌进 HTML 才能在手机上直接打开
/// - split_by_chapter = true 时每章一个文件（漫画很大时单个文件浏览器吃不消）
#[instrument(
    level = "error",
    skip_all,
    fields(source = ?source, target_dir = ?target_dir, split_by_chapter = split_by_chapter, comics = keys.len())
)]
pub fn export_quick_reader_single_file(
    app: &AppHandle,
    source: LocalLibrarySource,
    keys: Vec<String>,
    target_dir: Option<String>,
    split_by_chapter: bool,
) -> eyre::Result<Vec<PathBuf>> {
    let source_dir = source_dir_of(app, source)?;
    let base_dir = resolve_base_dir(&source_dir, target_dir.as_deref())?;

    let selected_keys: Vec<String> = keys.into_iter().map(|key| key.replace('\\', "/")).collect();
    let comics: Vec<ComicJob> = scan_all_comics(&source_dir)?
        .into_iter()
        .filter(|comic| selected_keys.is_empty() || selected_keys.contains(&comic.key))
        .collect();

    if comics.is_empty() {
        return Err(eyre!("没有选中任何可导出的漫画"));
    }

    let total: usize = comics.iter().map(ComicJob::file_count).sum();
    let _ = ExportQuickReaderEvent::Start { total }.emit(app);

    let mut done = 0usize;
    let mut written: Vec<PathBuf> = Vec::new();

    for comic in &comics {
        let mut chapters: Vec<(String, Vec<(String, Vec<u8>)>)> = Vec::new();
        for chapter in &comic.chapters {
            let images = chapter.source.read_all_images()?;
            done += images.len();
            emit_progress(app, done, total, &comic.name, &chapter.title);
            chapters.push((chapter.title.clone(), images));
        }

        if split_by_chapter {
            for (chapter_title, images) in chapters {
                let manifest = build_data_uri_manifest(
                    &comic.name,
                    std::slice::from_ref(&(chapter_title.clone(), images)),
                )?;
                let path = unique_file_path(
                    &base_dir,
                    &format!("{}-{chapter_title}", comic.name),
                    "html",
                );
                fs::write(&path, render_html(&manifest)?).wrap_err("写入单文件 HTML 失败")?;
                written.push(path);
            }
        } else {
            let manifest = build_data_uri_manifest(&comic.name, &chapters)?;
            let path = unique_file_path(&base_dir, &comic.name, "html");
            fs::write(&path, render_html(&manifest)?).wrap_err("写入单文件 HTML 失败")?;
            written.push(path);
        }
    }

    let _ = ExportQuickReaderEvent::End { dir: base_dir.clone() }.emit(app);
    tracing::info!("单文件快速阅读器导出成功，共 {} 个文件", written.len());

    Ok(written)
}

/// 组装「图片内嵌」的清单
fn build_data_uri_manifest(
    comic_name: &str,
    chapters: &[(String, Vec<(String, Vec<u8>)>)],
) -> eyre::Result<Manifest> {
    let mut manifest_chapters: Vec<ManifestChapter> = Vec::new();
    let mut cover: Option<String> = None;

    for (chapter_title, images) in chapters {
        let mut pages: Vec<String> = Vec::with_capacity(images.len());
        for (file_name, bytes) in images {
            pages.push(to_data_uri(file_name, bytes));
            if cover.is_none() && images.len() > 1 {
                // 用第一张图当封面（只在真正的页面里挑，避免用占位图）
                cover = Some(pages[0].clone());
            }
        }

        if pages.is_empty() {
            continue;
        }

        manifest_chapters.push(ManifestChapter {
            title: chapter_title.clone(),
            pages,
        });
    }

    if manifest_chapters.is_empty() {
        return Err(eyre!("没有可导出的内容"));
    }

    Ok(Manifest {
        title: comic_name.to_string(),
        generated_at: time_string(),
        comics: vec![ManifestComic {
            name: comic_name.to_string(),
            cover,
            chapters: manifest_chapters,
        }],
    })
}

/// 把清单嵌进阅读器 HTML
fn render_html(manifest: &Manifest) -> eyre::Result<String> {
    let manifest_json = serde_json::to_string(manifest)
        .wrap_err("序列化清单失败")?
        // 防止漫画名里出现 </script> 之类把页面搞坏
        .replace('<', "\\u003c");

    Ok(READER_HTML.replace("/*__MANIFEST__*/", &manifest_json))
}

fn emit_progress(app: &AppHandle, current: usize, total: usize, comic: &str, chapter: &str) {
    // 文件很多时按批上报，避免事件太密集
    if total > 200 && current % 20 != 0 && current != total {
        return;
    }

    let _ = ExportQuickReaderEvent::Progress {
        current,
        total,
        indicator: format!("{comic} / {chapter}"),
    }
    .emit(app);
}

fn time_string() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    format!("{now}")
}

const READER_GUIDE: &str = "快速阅读器 使用说明\n\n1. 双击本目录下的 index.html，用 Chrome / Edge / Safari 等浏览器打开即可阅读\n2. 手机上：把整个「快速阅读器」文件夹拷到手机，用浏览器打开其中的 index.html\n3. 分享：把整个文件夹压缩后发给别人，对方解压后打开 index.html 就能看\n\n目录结构：\n  快速阅读器/index.html          阅读器本体（已内置漫画清单，无需联网）\n  快速阅读器/漫画名/章节/0001.jpg  解压好的漫画图片\n\n注意：不要只单独发送 index.html，必须把漫画文件夹一起发送。\n";

/// 分享包里 index.html 的内容
/// - 清单内嵌在页面里（file:// 下不能 fetch，所以不能外链 JSON）
/// - 图片用相对路径，双击打开即可阅读
/// - 电脑端固定 600px 宽（和软件内阅读器一致），手机端铺满屏幕宽度
const READER_HTML: &str = r##"<!doctype html>
<html lang="zh-CN">
<head>
<meta charset="utf-8" />
<meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
<title>漫画快速阅读器</title>
<style>
  :root { color-scheme: dark; }
  * { box-sizing: border-box; -webkit-tap-highlight-color: transparent; }
  html, body { margin: 0; padding: 0; height: 100%; }
  body { background: #111; color: #e8e8e8; font: 14px/1.5 system-ui, -apple-system, "Segoe UI", "Microsoft YaHei", sans-serif; }
  img { -webkit-user-drag: none; user-select: none; }
  .app { max-width: 600px; margin: 0 auto; min-height: 100%; background: #111; }
  body.mobile .app { max-width: 100%; }

  .page { padding: 12px; }
  h1 { font-size: 16px; margin: 4px 0 12px; font-weight: 600; }
  .sub { color: #8a8a8a; font-size: 12px; }
  .btn { background: #262626; color: #e8e8e8; border: 1px solid #333; border-radius: 6px; padding: 5px 10px; font-size: 13px; cursor: pointer; }
  .btn:hover { background: #333; }
  .btn:disabled { opacity: .4; cursor: default; }
  .btn.on { background: #ff7a00; border-color: #ff7a00; color: #fff; }

  .library { display: flex; flex-direction: column; gap: 8px; }
  .item { display: flex; gap: 10px; padding: 8px; background: #1b1b1b; border: 1px solid #2a2a2a; border-radius: 8px; cursor: pointer; }
  .item:hover { background: #242424; }
  .thumb { width: 64px; height: 88px; flex: 0 0 64px; background: #222; border-radius: 4px; overflow: hidden; display: flex; align-items: center; justify-content: center; color: #666; font-size: 11px; }
  .thumb img { width: 100%; height: 100%; object-fit: cover; }
  .info { display: flex; flex-direction: column; justify-content: center; overflow: hidden; }
  .title { font-size: 15px; font-weight: 600; overflow: hidden; text-overflow: ellipsis; display: -webkit-box; -webkit-line-clamp: 2; -webkit-box-orient: vertical; }
  .chapters { display: flex; flex-direction: column; gap: 6px; }
  .chapter { display: flex; justify-content: space-between; gap: 10px; padding: 10px; background: #1b1b1b; border: 1px solid #2a2a2a; border-radius: 6px; cursor: pointer; }
  .chapter:hover { background: #242424; }
  .head { display: flex; align-items: center; gap: 8px; margin-bottom: 10px; }
  .htitle { font-weight: 600; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }

  .reader { display: flex; flex-direction: column; height: 100vh; }
  .bar { display: flex; align-items: center; gap: 6px; padding: 6px 8px; background: #000; flex-wrap: wrap; }
  .bar .sp { flex: 1; }
  .nowrap { white-space: nowrap; }
  select { background: #262626; color: #e8e8e8; border: 1px solid #333; border-radius: 6px; padding: 5px 6px; font-size: 13px; max-width: 100%; }
  .stage { flex: 1; overflow: auto; position: relative; background: #111; }
  .stage.paged { display: flex; align-items: center; justify-content: center; }
  .stage.scroll { display: flex; flex-direction: column; }
  .sline { width: 100%; }
  .sline img { display: block; width: 100%; height: auto; }
  .stage.paged img { display: block; max-width: 100%; max-height: 100%; object-fit: contain; }
  .zone { position: absolute; top: 0; bottom: 0; width: 28%; }
  .zone.left { left: 0; cursor: w-resize; }
  .zone.right { right: 0; cursor: e-resize; }
  .pager { color: #9a9a9a; }
  input[type=range] { flex: 1; }
</style>
</head>
<body>
<div id="root" class="app"></div>
<script id="manifest" type="application/json">/*__MANIFEST__*/</script>
<script>
(function () {
  var MANIFEST = JSON.parse(document.getElementById('manifest').textContent);
  var root = document.getElementById('root');
  var isMobile = /Android|iPhone|iPad|iPod|Mobile/i.test(navigator.userAgent) || window.innerWidth <= 820;
  if (isMobile) document.body.classList.add('mobile');

  var state = { view: 'library', comic: 0, chapter: 0, page: 0, mode: 'scroll' };
  var preloaded = [];
  var lastFlip = 0;

  function esc(s) { return String(s).replace(/[&<>"']/g, function (c) { return { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]; }); }
  function url(p) { return p.indexOf('data:') === 0 ? p : encodeURI(p); }
  var imgErrorShown = false;
  function onImgError() {
    if (imgErrorShown) return;
    imgErrorShown = true;
    var tip = document.createElement('div');
    tip.style.cssText = 'position:fixed;left:0;right:0;bottom:0;z-index:99;background:#7a1f1f;color:#fff;padding:10px 14px;font-size:13px;line-height:1.6';
    tip.innerHTML = '图片加载失败：手机浏览器禁止本地网页读取同目录的图片文件。<br>请改用「单文件 HTML」版本（图片已内嵌），或用电脑浏览器打开。';
    document.body.appendChild(tip);
  }
  function comic() { return MANIFEST.comics[state.comic]; }
  function chapter() { return comic().chapters[state.chapter]; }
  function pages() { return chapter().pages; }

  // ---------- 漫画列表 ----------
  function renderLibrary() {
    state.view = 'library';
    var html = '<div class="page"><h1>共 ' + MANIFEST.comics.length + ' 部漫画</h1><div class="library">';
    MANIFEST.comics.forEach(function (c, i) {
      html += '<div class="item" data-comic="' + i + '">' +
        '<div class="thumb">' + (c.cover ? '<img src="' + url(c.cover) + '" loading="lazy" alt="">' : '无封面') + '</div>' +
        '<div class="info"><div class="title">' + esc(c.name) + '</div><div class="sub">' + c.chapters.length + ' 章</div></div>' +
        '</div>';
    });
    root.innerHTML = html + '</div></div>';
    root.querySelectorAll('[data-comic]').forEach(function (el) {
      el.onclick = function () { state.comic = +el.dataset.comic; renderChapters(); };
    });
  }

  // ---------- 章节列表 ----------
  function renderChapters() {
    state.view = 'chapters';
    var c = comic();
    var html = '<div class="page"><div class="head"><button class="btn" id="back">返回</button><span class="htitle">' + esc(c.name) + '</span></div><div class="chapters">';
    c.chapters.forEach(function (ch, i) {
      html += '<div class="chapter" data-chapter="' + i + '"><span>' + esc(ch.title) + '</span><span class="sub">' + ch.pages.length + ' 页</span></div>';
    });
    root.innerHTML = html + '</div></div>';
    var back = root.querySelector('#back');
    if (MANIFEST.comics.length > 1) { back.onclick = renderLibrary; } else { back.style.display = 'none'; }
    root.querySelectorAll('[data-chapter]').forEach(function (el) {
      el.onclick = function () { state.chapter = +el.dataset.chapter; state.page = 0; renderReader(); };
    });
  }

  // ---------- 阅读器 ----------
  function renderReader() {
    state.view = 'reader';
    var c = comic();
    var ch = chapter();
    var options = c.chapters.map(function (x, i) {
      return '<option value="' + i + '"' + (i === state.chapter ? ' selected' : '') + '>' + esc(x.title) + '</option>';
    }).join('');

    root.innerHTML =
      '<div class="reader">' +
        '<div class="bar">' +
          '<button class="btn" id="back">返回</button>' +
          '<span class="htitle">' + esc(c.name) + '</span>' +
          '<span class="sp"></span>' +
          '<span class="nowrap pager"><span id="cur">' + (state.page + 1) + '</span> / ' + ch.pages.length + '</span>' +
        '</div>' +
        '<div class="bar">' +
          '<button class="btn" id="prevch"' + (state.chapter === 0 ? ' disabled' : '') + '>上一章</button>' +
          '<select id="chselect">' + options + '</select>' +
          '<button class="btn" id="nextch"' + (state.chapter === c.chapters.length - 1 ? ' disabled' : '') + '>下一章</button>' +
          '<span class="sp"></span>' +
          '<button class="btn' + (state.mode === 'scroll' ? ' on' : '') + '" id="mscroll">上下滑动</button>' +
          '<button class="btn' + (state.mode === 'paged' ? ' on' : '') + '" id="mpaged">左右翻页</button>' +
        '</div>' +
        '<div class="stage" id="stage"></div>' +
        '<div class="bar" id="bottom" style="display:none"></div>' +
      '</div>';

    var back = root.querySelector('#back');
    back.onclick = function () { if (MANIFEST.comics.length > 1) { renderChapters(); } else { renderChapters(); } };
    root.querySelector('#chselect').onchange = function (e) {
      state.chapter = +e.target.value; state.page = 0; renderReader();
    };
    root.querySelector('#prevch').onclick = function () { goChapter(state.chapter - 1); };
    root.querySelector('#nextch').onclick = function () { goChapter(state.chapter + 1); };
    root.querySelector('#mscroll').onclick = function () { setMode('scroll'); };
    root.querySelector('#mpaged').onclick = function () { setMode('paged'); };
    renderStage();
  }

  function setMode(mode) {
    if (state.mode === mode) return;
    state.mode = mode; state.page = 0;
    renderReader();
  }

  function goChapter(index) {
    if (index < 0 || index >= comic().chapters.length) return;
    state.chapter = index; state.page = 0;
    renderReader();
  }

  function renderStage() {
    var stage = root.querySelector('#stage');
    var list = pages();
    stage.className = 'stage ' + state.mode;
    stage.scrollTop = 0;
    preloaded = [];

    if (state.mode === 'scroll') {
      var html = '';
      list.forEach(function (p, i) {
        html += '<div class="sline" data-page="' + i + '"><img src="' + url(p) + '" loading="lazy" decoding="async" alt="" onerror="window.__qrImgError && window.__qrImgError()"></div>';
      });
      stage.innerHTML = html;
      stage.onscroll = onScroll;
      stage.onwheel = null;
    } else {
      stage.innerHTML = '<img id="pimg" src="' + url(list[state.page]) + '" decoding="async" alt="" onerror="window.__qrImgError && window.__qrImgError()">' +
        '<div class="zone left" id="zl"></div><div class="zone right" id="zr"></div>';
      root.querySelector('#zl').onclick = prevPage;
      root.querySelector('#zr').onclick = nextPage;
      stage.onscroll = null;
      stage.onwheel = onWheel;
      prefetch();
    }

    renderBottom();
    updateIndicator();
  }

  function renderBottom() {
    var bottom = root.querySelector('#bottom');
    if (state.mode !== 'paged' || pages().length <= 1) { bottom.style.display = 'none'; return; }
    bottom.style.display = 'flex';
    bottom.innerHTML = '<button class="btn" id="pprev">‹</button>' +
      '<input type="range" id="pslider" min="0" max="' + (pages().length - 1) + '" value="' + state.page + '">' +
      '<button class="btn" id="pnext">›</button>';
    root.querySelector('#pprev').onclick = prevPage;
    root.querySelector('#pnext').onclick = nextPage;
    root.querySelector('#pslider').oninput = function (e) { goPage(+e.target.value); };
  }

  function goPage(index) {
    var max = pages().length - 1;
    if (index < 0) index = 0;
    if (index > max) index = max;
    state.page = index;
    var image = root.querySelector('#pimg');
    if (image) { image.src = url(pages()[index]); }
    var slider = root.querySelector('#pslider');
    if (slider) { slider.value = String(index); }
    var stage = root.querySelector('#stage');
    if (stage && state.mode === 'paged') { stage.scrollTop = 0; }
    updateIndicator();
    prefetch();
    save();
  }

  function nextPage() { if (state.page < pages().length - 1) goPage(state.page + 1); else goChapter(state.chapter + 1); }
  function prevPage() { if (state.page > 0) goPage(state.page - 1); else goChapter(state.chapter - 1); }

  function updateIndicator() {
    var cur = root.querySelector('#cur');
    if (cur) cur.textContent = String(state.page + 1);
  }

  function prefetch() {
    if (state.mode !== 'paged') return;
    var list = pages();
    preloaded = [];
    [1, 2, -1].forEach(function (offset) {
      var index = state.page + offset;
      if (index < 0 || index >= list.length) return;
      var image = new Image();
      image.src = url(list[index]);
      preloaded.push(image);
    });
    save();
  }

  function onScroll() {
    var stage = root.querySelector('#stage');
    if (!stage) return;
    var marker = stage.scrollTop + stage.clientHeight * 0.4;
    var lines = stage.querySelectorAll('[data-page]');
    var current = 0;
    for (var i = 0; i < lines.length; i++) {
      if (lines[i].offsetTop <= marker) { current = +lines[i].dataset.page; } else { break; }
    }
    if (current !== state.page) { state.page = current; updateIndicator(); save(); }
  }

  function onWheel(event) {
    var stage = root.querySelector('#stage');
    if (!stage) return;
    var now = Date.now();
    if (now - lastFlip < 160) { return; }
    var atBottom = stage.scrollTop + stage.clientHeight >= stage.scrollHeight - 2;
    var atTop = stage.scrollTop <= 2;
    if (event.deltaY > 0 && atBottom) { lastFlip = now; nextPage(); }
    else if (event.deltaY < 0 && atTop) { lastFlip = now; prevPage(); }
  }

  function save() {
    try { localStorage.setItem('qr:last', JSON.stringify({ comic: state.comic, chapter: state.chapter, page: state.page, mode: state.mode })); } catch (e) {}
  }

  function restore() {
    try {
      var raw = localStorage.getItem('qr:last');
      if (!raw) return;
      var saved = JSON.parse(raw);
      if (typeof saved.comic === 'number' && saved.comic < MANIFEST.comics.length) state.comic = saved.comic;
      if (saved.mode === 'paged' || saved.mode === 'scroll') state.mode = saved.mode;
      var chapterCount = MANIFEST.comics[state.comic].chapters.length;
      if (typeof saved.chapter === 'number' && saved.chapter < chapterCount) state.chapter = saved.chapter;
    } catch (e) {}
  }

  window.__qrImgError = onImgError;

  document.addEventListener('keydown', function (event) {
    if (state.view !== 'reader') { if (event.key === 'Escape') renderChapters(); return; }
    var stage = root.querySelector('#stage');
    var step = (stage ? stage.clientHeight : window.innerHeight) * 0.9;
    if (event.key === 'ArrowRight') { nextPage(); }
    else if (event.key === 'ArrowLeft') { prevPage(); }
    else if (event.key === 'ArrowDown' || event.key === 'PageDown' || event.key === ' ') { if (stage) stage.scrollBy({ top: step }); }
    else if (event.key === 'ArrowUp' || event.key === 'PageUp') { if (stage) stage.scrollBy({ top: -step }); }
    else if (event.key === 'm' || event.key === 'M') { setMode(state.mode === 'paged' ? 'scroll' : 'paged'); }
    else if (event.key === 'Escape') { renderChapters(); }
    else { return; }
    event.preventDefault();
  });

  restore();
  if (MANIFEST.comics.length === 1) { renderChapters(); } else { renderLibrary(); }
})();
</script>
</body>
</html>
"##;
#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "jmcomic-shelf-quickreader-{name}-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// 图片目录 + cbz 目录都能被扫到，cbz 会被解压，清单里的相对路径要能对上真实文件
    #[test]
    fn scan_and_build_should_pack_images_and_extract_cbz() {
        let root = temp_dir("src");
        let sources = root.join("源目录");
        std::fs::create_dir_all(&sources).unwrap();

        let comic_a = sources.join("漫画A");
        std::fs::create_dir_all(comic_a.join("第1话")).unwrap();
        std::fs::create_dir_all(comic_a.join("第2话")).unwrap();
        std::fs::write(comic_a.join("cover.jpg"), b"cover-a").unwrap();
        std::fs::write(comic_a.join("第1话").join("0001.jpg"), b"a1").unwrap();
        std::fs::write(comic_a.join("第1话").join("0002.jpg"), b"a2").unwrap();
        std::fs::write(comic_a.join("第2话").join("0001.jpg"), b"a3").unwrap();

        let comic_b = sources.join("漫画B");
        let cbz_dir = comic_b.join("cbz");
        std::fs::create_dir_all(&cbz_dir).unwrap();
        {
            let file = std::fs::File::create(cbz_dir.join("第1话.cbz")).unwrap();
            let mut zip_writer = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default();
            zip_writer.start_file("0002.jpg", options).unwrap();
            zip_writer.write_all(b"b2").unwrap();
            zip_writer.start_file("0001.jpg", options).unwrap();
            zip_writer.write_all(b"b1").unwrap();
            zip_writer.start_file("ComicInfo.xml", options).unwrap();
            zip_writer.write_all(b"<ComicInfo/>").unwrap();
            zip_writer.finish().unwrap();
        }

        // 扫描：两部漫画，key 是目录名
        let comics = scan_all_comics(&sources).unwrap();
        assert_eq!(comics.len(), 2);
        assert_eq!(comics[0].key, "漫画A");
        assert_eq!(comics[1].key, "漫画B");
        assert_eq!(comics[0].chapters.len(), 2);
        assert_eq!(comics[1].chapters.len(), 1);
        assert_eq!(comics[1].kind(), "cbz");
        assert_eq!(comics[0].kind(), "图片目录");

        let out_dir = root.join("快速阅读器");
        let mut starts = Vec::new();
        let manifest = build_quick_reader(
            &out_dir,
            &comics,
            |total| starts.push(total),
            |_current, _total, _comic, _chapter| {},
        )
        .unwrap();

        assert_eq!(manifest.comics.len(), 2);
        assert_eq!(starts, vec![5]); // 3 张图片 + 2 张 cbz 里的图片
        let a = &manifest.comics[0];
        assert_eq!(a.name, "漫画A");
        assert_eq!(a.chapters.len(), 2);
        assert_eq!(a.chapters[0].pages.len(), 2);
        assert_eq!(a.cover.as_deref(), Some("漫画A/cover.jpg"));
        let b = &manifest.comics[1];
        assert_eq!(b.name, "漫画B");
        assert_eq!(b.chapters.len(), 1);
        assert_eq!(b.chapters[0].title, "第1话");
        // cbz 解压后必须按自然序排页
        assert_eq!(
            b.chapters[0].pages,
            vec!["漫画B/第1话/0001.jpg", "漫画B/第1话/0002.jpg"]
        );

        for comic in &manifest.comics {
            for chapter in &comic.chapters {
                for page in &chapter.pages {
                    let path = out_dir.join(page);
                    assert!(path.is_file(), "缺少文件: {}", path.display());
                }
            }
        }
        assert_eq!(
            std::fs::read(out_dir.join("漫画B/第1话/0002.jpg")).unwrap(),
            b"b2"
        );
        assert!(!out_dir.join("漫画B/第1话/ComicInfo.xml").exists());

        let html = render_html(&manifest).unwrap();
        assert!(html.contains("漫画A/第1话/0001.jpg"));
        assert!(!html.contains("/*__MANIFEST__*/"));

        let _ = std::fs::remove_dir_all(&root);
    }

    /// 只导出选中的漫画
    #[test]
    fn scan_should_allow_exporting_only_selected_comics() {
        let root = temp_dir("select");
        let sources = root.join("源目录");
        for name in ["漫画A", "漫画B", "漫画C"] {
            let chapter = sources.join(name).join("第1话");
            std::fs::create_dir_all(&chapter).unwrap();
            std::fs::write(chapter.join("0001.jpg"), b"x").unwrap();
        }

        let comics = scan_all_comics(&sources).unwrap();
        assert_eq!(comics.len(), 3);

        let selected: Vec<ComicJob> = comics
            .into_iter()
            .filter(|comic| comic.key == "漫画B" || comic.key == "漫画C")
            .collect();
        assert_eq!(selected.len(), 2);

        let out_dir = root.join("快速阅读器");
        let manifest = build_quick_reader(&out_dir, &selected, |_total| {}, |_, _, _, _| {})
            .unwrap();
        let names: Vec<&str> = manifest
            .comics
            .iter()
            .map(|comic| comic.name.as_str())
            .collect();
        assert_eq!(names, vec!["漫画B", "漫画C"]);
        assert!(!out_dir.join("漫画A").exists());

        let _ = std::fs::remove_dir_all(&root);
    }

    /// 上一次导出的阅读器目录不能被当成一部漫画再打包一次
    #[test]
    fn scan_should_skip_previous_reader_dirs() {
        let root = temp_dir("skip");
        let sources = root.join("源目录");
        let chapter = sources.join("漫画A").join("第1话");
        std::fs::create_dir_all(&chapter).unwrap();
        std::fs::write(chapter.join("0001.jpg"), b"x").unwrap();

        // 旧输出目录：带 index.html + 使用说明.txt 才会被识别成阅读器
        let old_reader = sources.join("快速阅读器").join("旧漫画").join("第1话");
        std::fs::create_dir_all(&old_reader).unwrap();
        std::fs::write(old_reader.join("0001.jpg"), b"old").unwrap();
        std::fs::write(sources.join("快速阅读器").join("index.html"), b"<html></html>").unwrap();
        std::fs::write(sources.join("快速阅读器").join("使用说明.txt"), b"guide").unwrap();

        let comics = scan_all_comics(&sources).unwrap();
        assert_eq!(comics.len(), 1);
        assert_eq!(comics[0].name, "漫画A");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// 目录重名时自动加后缀：多个阅读器可以同时存在
    #[test]
    fn unique_out_dir_should_add_suffix() {
        let root = temp_dir("unique");
        let source = root.join("源目录");
        std::fs::create_dir_all(&source).unwrap();

        assert_eq!(
            unique_out_dir(&source, "快速阅读器")
                .file_name()
                .unwrap()
                .to_string_lossy(),
            "快速阅读器"
        );

        std::fs::create_dir_all(source.join("快速阅读器")).unwrap();
        assert_eq!(
            unique_out_dir(&source, "快速阅读器")
                .file_name()
                .unwrap()
                .to_string_lossy(),
            "快速阅读器-2"
        );

        std::fs::create_dir_all(source.join("快速阅读器-2")).unwrap();
        assert_eq!(
            unique_out_dir(&source, "快速阅读器")
                .file_name()
                .unwrap()
                .to_string_lossy(),
            "快速阅读器-3"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    /// 单文件模式：图片必须内嵌成 data URI（手机端不允许读同目录图片）
    #[test]
    fn data_uri_manifest_should_inline_every_image() {
        let chapters = vec![(
            "第1话".to_string(),
            vec![
                ("0001.jpg".to_string(), vec![1u8, 2, 3]),
                ("0002.png".to_string(), vec![4u8, 5]),
            ],
        )];

        let manifest = build_data_uri_manifest("漫画A", &chapters).unwrap();
        assert_eq!(manifest.comics.len(), 1);
        assert_eq!(manifest.comics[0].name, "漫画A");

        let pages = &manifest.comics[0].chapters[0].pages;
        assert_eq!(pages.len(), 2);
        assert!(pages[0].starts_with("data:image/jpeg;base64,"));
        assert!(pages[1].starts_with("data:image/png;base64,"));
        assert!(
            manifest.comics[0]
                .cover
                .as_deref()
                .is_some_and(|cover| cover.starts_with("data:image/")),
            "单文件模式的封面也要内嵌"
        );

        // 渲染出来的 HTML 里不应再有相对路径的图片引用
        let html = render_html(&manifest).unwrap();
        assert!(html.contains("data:image/jpeg;base64,"));
        assert!(!html.contains("0001.jpg\""), "不应残留相对路径");
    }

    /// 同名文件要加后缀，避免互相覆盖
    #[test]
    fn unique_file_path_should_add_suffix() {
        let root = temp_dir("file");
        std::fs::create_dir_all(&root).unwrap();

        assert_eq!(
            unique_file_path(&root, "漫画A", "html")
                .file_name()
                .unwrap()
                .to_string_lossy(),
            "漫画A.html"
        );

        std::fs::write(root.join("漫画A.html"), b"x").unwrap();
        assert_eq!(
            unique_file_path(&root, "漫画A", "html")
                .file_name()
                .unwrap()
                .to_string_lossy(),
            "漫画A-2.html"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    /// 导出位置：没传就用来源目录；传了就写到自定义目录（不存在会自动创建）
    #[test]
    fn resolve_base_dir_should_prefer_custom_dir() {
        let root = temp_dir("basedir");
        let source = root.join("源目录");
        std::fs::create_dir_all(&source).unwrap();

        assert_eq!(resolve_base_dir(&source, None).unwrap(), source);
        assert_eq!(resolve_base_dir(&source, Some("   ")).unwrap(), source);

        let custom = root.join("D盘分享").join("漫画");
        let resolved = resolve_base_dir(
            &source,
            Some(custom.to_string_lossy().to_string().as_str()),
        )
        .unwrap();
        assert_eq!(resolved, custom);
        assert!(custom.is_dir(), "自定义目录应该被自动创建");

        // 相对路径要拒绝
        assert!(resolve_base_dir(&source, Some("relative/path")).is_err());

        let _ = std::fs::remove_dir_all(&root);
    }

    /// 目录名要做安全化处理
    #[test]
    fn safe_dir_name_should_strip_path_separators() {
        assert_eq!(safe_dir_name("我的/阅读器"), "我的_阅读器");
        assert_eq!(safe_dir_name("a:b*c"), "a_b_c");
        assert_eq!(safe_dir_name(""), "未命名");
    }

    /// 空目录：扫不到漫画，也不该生成空包
    #[test]
    fn empty_dir_should_not_produce_package() {
        let root = temp_dir("empty");
        let sources = root.join("源目录");
        std::fs::create_dir_all(&sources).unwrap();

        assert!(scan_all_comics(&sources).unwrap().is_empty());

        let result = build_quick_reader(&root.join("out"), &[], |_t| {}, |_, _, _, _| {});
        assert!(result.is_err());

        let _ = std::fs::remove_dir_all(&root);
    }
}

