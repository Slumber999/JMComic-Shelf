//! 封面：在线封面也走后端自定义协议，才能吃到应用内代理设置 + 图片线路自动 fallback
//!
//! - `/cover/{comicId}`：在线封面（3x4 缩略图），走 JmClient（多线路 fallback），带内存缓存
//! - `/local-cover/{comicId}?dir=<下载目录>`：本地库存优先直接读下载目录里的 cover.jpg，
//!   读不到（没勾选"下载封面"、或导出目录没有封面）再回落到在线封面
//!
//! 为什么要走后端：WebView 里的 `<img>` 走浏览器自己的网络栈，既不吃应用内代理设置，
//! 也没有多线路 fallback，封面域名 cdn-msp3.18comic.vip 连不上时整页封面就是空的。

use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use tokio::sync::Semaphore;

use bytes::Bytes;
use eyre::WrapErr;
use indexmap::IndexMap;
use parking_lot::Mutex;
use tauri::AppHandle;
use tracing::instrument;

use crate::extensions::AppHandleExt;
use crate::jm_client::IMAGE_DOMAIN;
use crate::reader::{image_response, not_found};

/// 封面内存缓存上限：一张 3x4 缩略图大约 60~90KB，64MB 能放几百上千张
const COVER_CACHE_BUDGET: usize = 64 * 1024 * 1024;

static COVER_CACHE: LazyLock<Mutex<CoverCache>> = LazyLock::new(|| Mutex::new(CoverCache::default()));

/// 同时只允许这么多张封面在下载：一页列表可能有几十上百张，
/// 不限并发容易被 CDN 限速（前端也做了 loading="lazy"，正常不会打到上限）
const COVER_FETCH_CONCURRENCY: usize = 8;
static COVER_FETCH_SEM: LazyLock<Semaphore> =
    LazyLock::new(|| Semaphore::new(COVER_FETCH_CONCURRENCY));

/// 简单的LRU：用IndexMap的插入顺序当访问顺序，超预算就从最旧的开始淘汰
#[derive(Default)]
struct CoverCache {
    entries: IndexMap<i64, (Bytes, &'static str)>,
    bytes: usize,
}

impl CoverCache {
    fn get(&mut self, comic_id: &i64) -> Option<(Bytes, &'static str)> {
        let value = self.entries.shift_remove(comic_id)?;
        // 重新插入 = 标记为最近使用
        self.entries.insert(*comic_id, value.clone());
        Some(value)
    }

    fn insert(&mut self, comic_id: i64, value: (Bytes, &'static str)) {
        if let Some(old) = self.entries.shift_remove(&comic_id) {
            self.bytes = self.bytes.saturating_sub(old.0.len());
        }
        self.bytes += value.0.len();
        self.entries.insert(comic_id, value);

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
    if let Some(rest) = path.strip_prefix("/cover/") {
        let comic_id = rest.parse::<i64>().ok()?;
        return Some(cover_response(app, comic_id, None));
    }

    if let Some(rest) = path.strip_prefix("/local-cover/") {
        let comic_id = rest.parse::<i64>().ok()?;
        let dir = query.and_then(query_value_of_dir);
        return Some(cover_response(app, comic_id, dir));
    }

    None
}

#[instrument(level = "error", skip_all, fields(comic_id = comic_id, local = local_dir.is_some()))]
fn cover_response(
    app: &AppHandle,
    comic_id: i64,
    local_dir: Option<String>,
) -> tauri::http::Response<Vec<u8>> {
    // 1. 本地库存：直接用下载目录里的 cover.jpg（零网络、离线也能显示）
    if let Some(dir) = local_dir {
        if let Some(path) = local_cover_path(app, &dir) {
            match std::fs::read(&path) {
                Ok(bytes) if !bytes.is_empty() => {
                    return image_response(bytes, content_type_of_cover(&path));
                }
                Ok(_) => tracing::warn!(path = %path.display(), "本地封面是空文件，改用在线封面"),
                Err(err) => tracing::warn!(path = %path.display(), message = %err, "读取本地封面失败，改用在线封面"),
            }
        }
    }

    // 2. 内存缓存
    if let Some((bytes, content_type)) = COVER_CACHE.lock().get(&comic_id) {
        return image_response(bytes.to_vec(), content_type);
    }

    // 3. 在线封面（JmClient 内部会按线路健康顺序自动 fallback）
    let url = format!("https://{IMAGE_DOMAIN}/media/albums/{comic_id}_3x4.jpg");
    match fetch_cover(app, &url) {
        Ok((bytes, content_type)) => {
            COVER_CACHE
                .lock()
                .insert(comic_id, (bytes.clone(), content_type));
            image_response(bytes.to_vec(), content_type)
        }
        Err(err) => {
            tracing::error!(url = %url, message = %err, "获取封面失败");
            not_found("fetch cover failed")
        }
    }
}

#[instrument(level = "error", skip_all, fields(url = url))]
fn fetch_cover(app: &AppHandle, url: &str) -> eyre::Result<(Bytes, &'static str)> {
    let jm_client = app.get_jm_client();
    // 这里是自定义协议的独立线程（不在 tokio runtime 里），所以可以 block_on
    let (bytes, _format) = tauri::async_runtime::block_on(async {
        let _permit = COVER_FETCH_SEM
            .acquire()
            .await
            .wrap_err("等待封面下载并发许可失败")?;
        jm_client.get_img_data_and_format(url).await
    })
    .wrap_err("下载封面失败")?;
    // 在线封面固定是 jpg
    Ok((bytes, "image/jpeg"))
}

/// 本地封面路径：只允许下载目录/导出目录里的 cover.jpg，避免自定义协议变成任意文件读取
fn local_cover_path(app: &AppHandle, dir: &str) -> Option<PathBuf> {
    let candidate = Path::new(dir).join("cover.jpg");
    if !candidate.is_file() {
        return None;
    }
    let candidate = candidate.canonicalize().ok()?;

    let config = app.get_config();
    let config = config.read();
    let allowed = [config.download_dir.clone(), config.export_dir.clone()];
    let allowed = allowed
        .iter()
        .filter_map(|root| root.canonicalize().ok())
        .any(|root| candidate.starts_with(root));

    allowed.then_some(candidate)
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
    let raw = query
        .split('&')
        .find_map(|pair| pair.strip_prefix("dir="))?;
    Some(percent_decode(raw))
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
}
