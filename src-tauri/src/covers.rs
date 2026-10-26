//! 封面：在线封面也走后端自定义协议，才能吃到应用内代理设置 + 图片线路自动 fallback
//!
//! - `/cover/{comicId}`：在线封面（3x4 缩略图），走 JmClient（多线路 fallback），带内存缓存
//! - `/local-cover/{comicId}?dir=<目录>`：本地库存优先直接读目录里的 cover.jpg；
//!   导出目录没有独立封面时，从 cbz/ 下第一本 cbz 里按需读第一张图；都没有再回落到在线封面
//!
//! 为什么要走后端：WebView 里的 `<img>` 走浏览器自己的网络栈，既不吃应用内代理设置，
//! 也没有多线路 fallback，封面域名 cdn-msp3.18comic.vip 连不上时整页封面就是空的。

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::LazyLock;
use std::time::{SystemTime, UNIX_EPOCH};

use tokio::sync::Semaphore;

use bytes::Bytes;
use eyre::WrapErr;
use indexmap::IndexMap;
use parking_lot::Mutex;
use tauri::{AppHandle, Manager};
use tracing::instrument;

use crate::extensions::AppHandleExt;
use crate::jm_client::IMAGE_DOMAIN;
use crate::reader::{image_response, list_cbz_files, natural_cmp, not_found};

/// 封面内存缓存上限：一张 3x4 缩略图大约 60~90KB，64MB 能放几百上千张
const COVER_CACHE_BUDGET: usize = 64 * 1024 * 1024;

/// 列表缩略图最大宽度（前端默认请求 320：卡片 3:4、3 倍屏也够）
const THUMB_MAX_WIDTH: u32 = 720;
/// 缩略图磁盘缓存：app_data/cover-cache/{id}-{w}.jpg，总容量上限与清理目标
const THUMB_CACHE_DIR: &str = "cover-cache";
const THUMB_DISK_BUDGET: u64 = 200 * 1024 * 1024;
/// 小图并发放宽：一屏二十多张，避免排队
const THUMB_FETCH_CONCURRENCY: usize = 16;
static THUMB_FETCH_SEM: LazyLock<Semaphore> =
    LazyLock::new(|| Semaphore::new(THUMB_FETCH_CONCURRENCY));
/// 每写这么多次缩略图，顺手清一次超额的旧文件
const THUMB_CLEANUP_EVERY: usize = 50;
static THUMB_WRITE_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// 从 cbz 里取封面时单张图片的上限；超过就放弃，保持原来的回落行为
const CBZ_COVER_MAX_BYTES: u64 = 8 * 1024 * 1024;

static COVER_CACHE: LazyLock<Mutex<CoverCache>> = LazyLock::new(|| Mutex::new(CoverCache::default()));

/// 同时只允许这么多张封面在下载：一页列表可能有几十上百张，
/// 不限并发容易被 CDN 限速（前端也做了 loading="lazy"，正常不会打到上限）
const COVER_FETCH_CONCURRENCY: usize = 8;
static COVER_FETCH_SEM: LazyLock<Semaphore> =
    LazyLock::new(|| Semaphore::new(COVER_FETCH_CONCURRENCY));

/// 临时测量：时间线写到 logcat（tag=JMCOVER），adb logcat 直接读；
/// 应用没有权限自己创建外部文件目录，所以不走文件
static INFLIGHT: AtomicUsize = AtomicUsize::new(0);

/// 1x1 透明 PNG：测量信标的响应体
const PIXEL_PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x0a, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x00, 0x01, 0x00, 0x00,
    0x05, 0x00, 0x01, 0x0d, 0x0a, 0x2d, 0xb4, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae,
    0x42, 0x60, 0x82,
];

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis())
        .unwrap_or_default()
}

fn measure(line: String) {
    crate::android_log::log_line("JMCOVER", &line);
}

/// 简单的LRU：用IndexMap的插入顺序当访问顺序，超预算就从最旧的开始淘汰
#[derive(Default)]
struct CoverCache {
    /// key = (comic_id, 宽度)：0 表示原图，>0 表示缩略图，两者分开缓存
    entries: IndexMap<(i64, u32), (Bytes, &'static str)>,
    bytes: usize,
}

impl CoverCache {
    fn get(&mut self, key: &(i64, u32)) -> Option<(Bytes, &'static str)> {
        let value = self.entries.shift_remove(key)?;
        // 重新插入 = 标记为最近使用
        self.entries.insert(*key, value.clone());
        Some(value)
    }

    fn insert(&mut self, key: (i64, u32), value: (Bytes, &'static str)) {
        if let Some(old) = self.entries.shift_remove(&key) {
            self.bytes = self.bytes.saturating_sub(old.0.len());
        }
        self.bytes += value.0.len();
        self.entries.insert(key, value);

        while self.bytes > COVER_CACHE_BUDGET {
            let Some((_, evicted)) = self.entries.shift_remove_index(0) else {
                break;
            };
            self.bytes = self.bytes.saturating_sub(evicted.0.len());
        }
    }
}

/// 处理 `comic` 协议里封面相关的路由；不是封面路由时返回 None，交给阅读器处理
pub fn handle_cover_request(
    app: &AppHandle,
    path: &str,
    query: Option<&str>,
) -> Option<tauri::http::Response<Vec<u8>>> {
    // 临时测量：前端把每张封面的 Resource Timing 回传到这里，落盘供 adb pull
    if path == "/cover-measure" {
        if let Some(data) = query.and_then(|query| query.strip_prefix("d=")) {
            measure(format!("js {}", percent_decode(data)));
        }
        return Some(image_response(PIXEL_PNG.to_vec(), "image/png"));
    }

    if let Some(rest) = path.strip_prefix("/cover/") {
        let comic_id = rest.parse::<i64>().ok()?;
        let width = thumb_width_of(query);
        return Some(cover_response(app, comic_id, None, width));
    }

    if let Some(rest) = path.strip_prefix("/local-cover/") {
        let comic_id = rest.parse::<i64>().ok()?;
        let dir = query.and_then(query_value_of_dir);
        let width = thumb_width_of(query);
        return Some(cover_response(app, comic_id, dir, width));
    }

    None
}

#[instrument(level = "error", skip_all, fields(comic_id = comic_id, local = local_dir.is_some()))]
fn cover_response(
    app: &AppHandle,
    comic_id: i64,
    local_dir: Option<String>,
    width: u32,
) -> tauri::http::Response<Vec<u8>> {
    // 临时测量：进入/返回各打一个时间点，并统计同时在处理的请求数
    let entered = now_ms();
    let inflight = INFLIGHT.fetch_add(1, Ordering::SeqCst) + 1;
    measure(format!(
        "cover-enter id={comic_id} w={width} t0={entered} inflight={inflight}"
    ));
    let outcome = cover_payload(app, comic_id, local_dir, width);
    INFLIGHT.fetch_sub(1, Ordering::SeqCst);

    match outcome {
        Ok(payload) => {
            measure(format!(
                "cover id={comic_id} w={width} t0={entered} t1={} src={} bytes={} wait_ms={} net_ms={} total_ms={} inflight={inflight}",
                now_ms(),
                payload.src,
                payload.bytes.len(),
                payload.wait_ms,
                payload.net_ms,
                now_ms().saturating_sub(entered),
            ));
            image_response(payload.bytes, payload.content_type)
        }
        Err(err) => {
            measure(format!(
                "cover id={comic_id} t0={entered} t1={} src=fail total_ms={} message={err}",
                now_ms(),
                now_ms().saturating_sub(entered),
            ));
            tracing::error!(message = %err, "获取封面失败");
            not_found("fetch cover failed")
        }
    }
}

/// 封面字节 + 来源标记；wait_ms/net_ms 只在走网络时有意义
struct CoverPayload {
    bytes: Vec<u8>,
    content_type: &'static str,
    src: &'static str,
    wait_ms: u128,
    net_ms: u128,
}

fn cover_payload(
    app: &AppHandle,
    comic_id: i64,
    local_dir: Option<String>,
    width: u32,
) -> eyre::Result<CoverPayload> {
    // 1. 本地库存：直接用目录里的 cover.jpg（零网络、离线也能显示）
    if let Some(dir) = local_dir {
        if let Some(path) = local_cover_path(app, &dir) {
            match std::fs::read(&path) {
                Ok(bytes) if !bytes.is_empty() => {
                    return processed_cover(
                        app,
                        comic_id,
                        width,
                        bytes,
                        content_type_of_cover(&path),
                        "local",
                        0,
                        0,
                    );
                }
                Ok(_) => tracing::warn!(path = %path.display(), "本地封面是空文件，改用在线封面"),
                Err(err) => tracing::warn!(path = %path.display(), message = %err, "读取本地封面失败，改用在线封面"),
            }
        }
        // 1.5 导出目录没有独立封面：从 cbz 里取第一张图，失效时安静地回落
        if let Some((bytes, content_type)) = local_cbz_cover(app, &dir) {
            return processed_cover(app, comic_id, width, bytes, content_type, "local-cbz", 0, 0);
        }
        // 1.6 下载目录是散图：取第一话目录里自然序第一张图（和导出目录同一套「第一页当封面」）
        if let Some((bytes, content_type)) = local_chapter_cover(app, &dir) {
            return processed_cover(
                app,
                comic_id,
                width,
                bytes,
                content_type,
                "local-chapter",
                0,
                0,
            );
        }
    }

    // 2. 内存缓存（原图 / 缩略图分开存）
    if let Some((bytes, content_type)) = COVER_CACHE.lock().get(&(comic_id, width)) {
        return Ok(CoverPayload {
            bytes: bytes.to_vec(),
            content_type,
            src: "ram",
            wait_ms: 0,
            net_ms: 0,
        });
    }

    // 3. 缩略图磁盘缓存：重启后仍然命中，第二次进列表几乎零等待
    if width > 0 {
        if let Some(bytes) = read_thumb_disk(app, comic_id, width) {
            let content_type = "image/jpeg";
            COVER_CACHE
                .lock()
                .insert((comic_id, width), (Bytes::from(bytes.clone()), content_type));
            return Ok(CoverPayload {
                bytes,
                content_type,
                src: "thumb-disk",
                wait_ms: 0,
                net_ms: 0,
            });
        }
    }

    // 4. 在线封面（JmClient 内部会按线路健康顺序自动 fallback）
    let url = format!("https://{IMAGE_DOMAIN}/media/albums/{comic_id}_3x4.jpg");
    let net_started = now_ms();
    let (bytes, content_type, wait_ms) = fetch_cover(app, &url, width)?;
    let net_ms = now_ms().saturating_sub(net_started);
    processed_cover(
        app,
        comic_id,
        width,
        bytes.to_vec(),
        content_type,
        if width > 0 { "thumb-net" } else { "net" },
        wait_ms,
        net_ms,
    )
}

/// 统一收尾：需要缩略图就缩放 + 落盘，然后进内存缓存
#[allow(clippy::too_many_arguments)]
fn processed_cover(
    app: &AppHandle,
    comic_id: i64,
    width: u32,
    bytes: Vec<u8>,
    content_type: &'static str,
    src: &'static str,
    wait_ms: u128,
    net_ms: u128,
) -> eyre::Result<CoverPayload> {
    if width == 0 {
        COVER_CACHE
            .lock()
            .insert((comic_id, width), (Bytes::from(bytes.clone()), content_type));
        return Ok(CoverPayload {
            bytes,
            content_type,
            src,
            wait_ms,
            net_ms,
        });
    }

    // 缩放失败（比如格式不认识）就退回原图，别让整张封面挂掉
    let thumb = match resize_cover(&bytes, width) {
        Ok(thumb) => thumb,
        Err(err) => {
            tracing::warn!(message = %err, "缩略图处理失败，直接返回原图");
            bytes
        }
    };
    write_thumb_disk(app, comic_id, width, &thumb);
    COVER_CACHE.lock().insert(
        (comic_id, width),
        (Bytes::from(thumb.clone()), "image/jpeg"),
    );
    Ok(CoverPayload {
        bytes: thumb,
        content_type: "image/jpeg",
        src,
        wait_ms,
        net_ms,
    })
}

/// 缩放到 3:4 的目标宽度，再按 JPEG 78 编码（原图 60~90KB → 约 15~30KB）
fn resize_cover(bytes: &[u8], width: u32) -> eyre::Result<Vec<u8>> {
    let image = image::load_from_memory(bytes).wrap_err("封面解码失败")?;
    let height = (width * 4).div_ceil(3).max(1);
    let resized = image.resize(width, height, image::imageops::FilterType::Triangle);
    let mut out = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 78)
        .encode_image(&resized.to_rgb8())
        .wrap_err("封面编码失败")?;
    Ok(out)
}

/// 缩略图缓存目录：app_data/cover-cache
fn thumb_cache_dir(app: &AppHandle) -> Option<PathBuf> {
    let dir = app.path().app_data_dir().ok()?.join(THUMB_CACHE_DIR);
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir)
}

/// 缩略图磁盘缓存路径：app_data/cover-cache/{id}-{w}.jpg
fn thumb_disk_path(app: &AppHandle, comic_id: i64, width: u32) -> Option<PathBuf> {
    Some(thumb_cache_dir(app)?.join(format!("{comic_id}-{width}.jpg")))
}

fn read_thumb_disk(app: &AppHandle, comic_id: i64, width: u32) -> Option<Vec<u8>> {
    let path = thumb_disk_path(app, comic_id, width)?;
    let bytes = std::fs::read(path).ok()?;
    (!bytes.is_empty()).then_some(bytes)
}

fn write_thumb_disk(app: &AppHandle, comic_id: i64, width: u32, bytes: &[u8]) {
    let Some(path) = thumb_disk_path(app, comic_id, width) else {
        return;
    };
    // 先写临时文件再改名：并发读时不会读到半个文件
    let tmp = path.with_extension("jpg.tmp");
    if std::fs::write(&tmp, bytes).is_ok() {
        let _ = std::fs::rename(&tmp, &path);
    }
    if THUMB_WRITE_COUNTER.fetch_add(1, Ordering::Relaxed) % THUMB_CLEANUP_EVERY == 0 {
        cleanup_thumb_disk(app);
    }
}

/// 超过 200MB 就把最旧的一半清掉（按修改时间），避免缓存无限长
fn cleanup_thumb_disk(app: &AppHandle) {
    let Some(dir) = thumb_cache_dir(app) else {
        return;
    };
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return;
    };
    let mut files: Vec<(PathBuf, u64, Option<SystemTime>)> = Vec::new();
    let mut total = 0_u64;
    for entry in entries.flatten() {
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        total += meta.len();
        files.push((entry.path(), meta.len(), meta.modified().ok()));
    }
    if total <= THUMB_DISK_BUDGET {
        return;
    }
    files.sort_by_key(|(_, _, modified)| *modified);
    let target = THUMB_DISK_BUDGET / 2;
    let mut remaining = total;
    for (path, len, _) in files {
        if remaining <= target {
            break;
        }
        if std::fs::remove_file(&path).is_ok() {
            remaining = remaining.saturating_sub(len);
        }
    }
}

#[instrument(level = "error", skip_all, fields(url = url))]
fn fetch_cover(
    app: &AppHandle,
    url: &str,
    width: u32,
) -> eyre::Result<(Bytes, &'static str, u128)> {
    let jm_client = app.get_jm_client();
    let semaphore = if width > 0 {
        &THUMB_FETCH_SEM
    } else {
        &COVER_FETCH_SEM
    };
    // 这里是自定义协议的独立线程（不在 tokio runtime 里），所以可以 block_on
    let (bytes, wait_ms) = tauri::async_runtime::block_on(async {
        let started = now_ms();
        let _permit = semaphore
            .acquire()
            .await
            .wrap_err("等待封面下载并发许可失败")?;
        let wait_ms = now_ms().saturating_sub(started);
        let (bytes, _format) = jm_client.get_img_data_and_format(url).await?;
        Ok::<_, eyre::Report>((bytes, wait_ms))
    })
    .wrap_err("下载封面失败")?;
    // 在线封面固定是 jpg
    Ok((bytes, "image/jpeg", wait_ms))
}

/// 本地封面路径：只允许下载目录/导出目录里的 cover.jpg，避免自定义协议变成任意文件读取
fn local_cover_path(app: &AppHandle, dir: &str) -> Option<PathBuf> {
    let candidate = Path::new(dir).join("cover.jpg");
    if !candidate.is_file() {
        return None;
    }
    let candidate = candidate.canonicalize().ok()?;
    is_allowed_local_path(app, &candidate).then_some(candidate)
}

/// 只允许下载目录/导出目录内的路径，避免自定义协议变成任意文件读取
fn is_allowed_local_path(app: &AppHandle, path: &Path) -> bool {
    let config = app.get_config();
    let config = config.read();
    let allowed = [config.download_dir.clone(), config.export_dir.clone()];
    allowed
        .iter()
        .filter_map(|root| root.canonicalize().ok())
        .any(|root| path.starts_with(root))
}

/// 下载目录的兜底封面：章节目录里自然序第一张图片
/// - 下载目录结构是「漫画/章节/图片」，只多扫一层；图片直接放在漫画目录里的也照顾到
/// - 只检查目录里自然序第一张，读不到（空文件 / 超过上限）就当没有，继续回落在线封面
fn local_chapter_cover(app: &AppHandle, dir: &str) -> Option<(Vec<u8>, &'static str)> {
    let root = Path::new(dir).canonicalize().ok()?;
    if !is_allowed_local_path(app, &root) {
        return None;
    }
    if let Some(cover) = first_image_in_dir(&root) {
        return Some(cover);
    }

    let mut chapter_dirs: Vec<PathBuf> = std::fs::read_dir(&root)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    chapter_dirs.sort_by(|a, b| natural_cmp(&file_name_of(a), &file_name_of(b)));
    chapter_dirs
        .iter()
        .find_map(|chapter_dir| first_image_in_dir(chapter_dir))
}

/// 目录里自然序第一张图片（png/jpg/jpeg/webp）；只读一张，超过上限就放弃
fn first_image_in_dir(dir: &Path) -> Option<(Vec<u8>, &'static str)> {
    let mut images: Vec<PathBuf> = std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file() && is_cover_image(&path.to_string_lossy()))
        .collect();
    images.sort_by(|a, b| natural_cmp(&file_name_of(a), &file_name_of(b)));

    let path = images.into_iter().next()?;
    let size = std::fs::metadata(&path).ok()?.len();
    if size == 0 || size > CBZ_COVER_MAX_BYTES {
        return None;
    }
    let bytes = std::fs::read(&path).ok()?;
    (!bytes.is_empty()).then(|| (bytes, content_type_of_cover(&path)))
}

/// 路径最后一段：自然序比较只认文件名
fn file_name_of(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().to_string())
        .unwrap_or_default()
}

/// 导出目录的兜底封面：cbz/ 下自然序第一本 cbz 里自然序第一张图片
/// - 只按需读一个 entry，不解压整本、不落临时文件；单张超过 CBZ_COVER_MAX_BYTES 就放弃
fn local_cbz_cover(app: &AppHandle, dir: &str) -> Option<(Vec<u8>, &'static str)> {
    let cbz_dir = Path::new(dir).join("cbz");
    if !cbz_dir.is_dir() {
        return None;
    }
    let cbz_dir = cbz_dir.canonicalize().ok()?;
    if !is_allowed_local_path(app, &cbz_dir) {
        return None;
    }

    let cbz_path = list_cbz_files(&cbz_dir).ok()?.into_iter().next()?;
    let cbz_path = cbz_path.canonicalize().ok()?;
    if !is_allowed_local_path(app, &cbz_path) {
        return None;
    }

    let file = std::fs::File::open(&cbz_path).ok()?;
    let mut archive = zip::ZipArchive::new(file).ok()?;
    // 只收集 entry 名（不读数据）再挑图，避免依赖 zip 内部顺序
    let names: Vec<String> = archive.file_names().map(str::to_string).collect();
    let name = first_image_entry(names.iter().map(String::as_str))?;

    let entry = archive.by_name(&name).ok()?;
    if entry.size() > CBZ_COVER_MAX_BYTES {
        return None;
    }
    let mut bytes = Vec::with_capacity(usize::try_from(entry.size()).unwrap_or(0));
    // 多读 1 字节：entry 头里的 size 不可信时也不会无限读
    entry
        .take(CBZ_COVER_MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .ok()?;
    if bytes.is_empty() || bytes.len() as u64 > CBZ_COVER_MAX_BYTES {
        return None;
    }

    Some((bytes, content_type_of_cover(Path::new(&name))))
}

/// 从 zip 的 entry 名里挑自然序第一张图片（png/jpg/jpeg/webp）
fn first_image_entry<'a>(names: impl Iterator<Item = &'a str>) -> Option<String> {
    names
        .filter(|name| is_cover_image(name))
        .min_by(|a, b| natural_cmp(a, b))
        .map(str::to_string)
}

fn is_cover_image(name: &str) -> bool {
    Path::new(name)
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            matches!(
                ext.to_ascii_lowercase().as_str(),
                "png" | "jpg" | "jpeg" | "webp"
            )
        })
}

fn content_type_of_cover(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_lowercase)
        .as_deref()
    {
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        _ => "image/jpeg",
    }
}

/// 从 `dir=xxx` 里取出下载目录（前端用 encodeURIComponent 编码过）
fn query_value_of_dir(query: &str) -> Option<String> {
    query_value_of(query, "dir")
}

/// 通用查询参数取值（不处理 '+'：
/// 前端用的是 encodeURIComponent，不会产出它）
fn query_value_of(query: &str, key: &str) -> Option<String> {
    let prefix = format!("{key}=");
    let raw = query.split('&').find_map(|pair| pair.strip_prefix(&prefix))?;
    Some(percent_decode(raw))
}

/// 缩略图宽度：带 w 参数才会缩放，0 / 缺省 = 原图（桌面行为不变）
fn thumb_width_of(query: Option<&str>) -> u32 {
    let raw = query.and_then(|query| query_value_of(query, "w"));
    match raw.and_then(|raw| raw.parse::<u32>().ok()) {
        Some(width) if width > 0 => width.clamp(64, THUMB_MAX_WIDTH),
        _ => 0,
    }
}

/// 只做 percent 解码：前端用的是 encodeURIComponent，不会产出 '+'
fn percent_decode(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[index + 1..index + 3])
                .ok()
                .and_then(|hex| u8::from_str_radix(hex, 16).ok());
            if let Some(byte) = hex {
                decoded.push(byte);
                index += 3;
                continue;
            }
        }
        decoded.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percent_decode_should_handle_chinese_and_backslash() {
        assert_eq!(
            percent_decode("C%3A%5CUsers%5C%E6%BC%AB%E7%94%BB"),
            "C:\\Users\\漫画"
        );
        assert_eq!(percent_decode("plain"), "plain");
        // 不合法的 % 序列原样保留，不能 panic
        assert_eq!(percent_decode("100%"), "100%");
    }

    #[test]
    fn query_value_of_dir_should_pick_dir_param() {
        assert_eq!(
            query_value_of_dir("dir=C%3A%5C%E6%BC%AB%E7%94%BB"),
            Some("C:\\漫画".to_string())
        );
        assert_eq!(query_value_of_dir("x=1&dir=abc"), Some("abc".to_string()));
        assert_eq!(query_value_of_dir("x=1"), None);
    }

    /// cbz 里混着 ComicInfo.xml、非图片和大写扩展名时，取自然序第一张图片
    #[test]
    fn first_image_entry_should_pick_natural_order_image() {
        let names = [
            "ComicInfo.xml",
            "10.jpg",
            "2.png",
            "1.WEBP",
            "notes.txt",
            "folder/3.jpeg",
        ];
        assert_eq!(
            first_image_entry(names.into_iter()),
            Some("1.WEBP".to_string())
        );
        assert_eq!(first_image_entry(["ComicInfo.xml"].into_iter()), None);
    }

    /// 散图目录：取自然序第一张图片，非图片和空目录不参与
    #[test]
    fn first_image_in_dir_should_pick_natural_first_image() {
        let root = std::env::temp_dir().join(format!(
            "jmcomic-shelf-cover-chapter-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).unwrap();
        for name in ["10.jpg", "2.jpg", "1.jpg", "notes.txt"] {
            std::fs::write(root.join(name), b"image").unwrap();
        }
        let (bytes, content_type) = first_image_in_dir(&root).unwrap();
        assert_eq!(bytes, b"image");
        assert_eq!(content_type, "image/jpeg");

        // 空目录 / 只有非图片时返回 None
        let empty = root.join("empty");
        std::fs::create_dir_all(&empty).unwrap();
        assert!(first_image_in_dir(&empty).is_none());
        let _ = std::fs::remove_dir_all(&root);
    }

    /// 导出目录挑 cbz：只看 .cbz，按文件名自然序取第一本
    #[test]
    fn list_cbz_files_should_pick_natural_first() {
        let root =
            std::env::temp_dir().join(format!("jmcomic-shelf-cover-cbz-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&root).unwrap();
        for name in ["第10话.cbz", "第2话.cbz", "cover.jpg", "notes.txt"] {
            std::fs::write(root.join(name), b"x").unwrap();
        }

        let files = list_cbz_files(&root).unwrap();
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].file_name().unwrap(), "第2话.cbz");

        let _ = std::fs::remove_dir_all(&root);
    }
}
