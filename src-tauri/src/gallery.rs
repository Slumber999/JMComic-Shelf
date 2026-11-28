//! 本地画廊：页级收藏，图片存应用数据目录，索引一份 JSON。
//! 预留 gallery_id：当前只有 default，后续加多画廊时不需要改文件结构。

use std::{
    fs,
    io::Cursor,
    path::PathBuf,
    sync::LazyLock,
    time::{SystemTime, UNIX_EPOCH},
};

use bytes::Bytes;
use eyre::{eyre, WrapErr};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager};
use tracing::instrument;
use uuid::Uuid;

use crate::{
    errors::{CommandError, CommandResult},
    reader::read_page_bytes,
};

const DEFAULT_GALLERY: &str = "default";
const INDEX_VERSION: u32 = 1;
const THUMB_WIDTH: u32 = 480;

#[derive(Default, Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GalleryItem {
    pub id: String,
    pub gallery_id: String,
    pub comic_id: i64,
    pub comic_title: String,
    pub chapter_id: i64,
    pub chapter_title: String,
    pub page_index: i64,
    pub width: u32,
    pub height: u32,
    pub created_at: i64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GalleryIndex {
    version: u32,
    items: Vec<GalleryItem>,
}

impl Default for GalleryIndex {
    fn default() -> Self {
        Self {
            version: INDEX_VERSION,
            items: Vec::new(),
        }
    }
}

static GALLERY_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| i64::try_from(duration.as_millis()).unwrap_or(i64::MAX))
}

fn gallery_dir(app: &AppHandle) -> eyre::Result<PathBuf> {
    let dir = app
        .path()
        .app_data_dir()
        .wrap_err("获取应用数据目录失败")?
        .join("gallery");
    fs::create_dir_all(dir.join("images")).wrap_err("创建画廊图片目录失败")?;
    fs::create_dir_all(dir.join("thumbs")).wrap_err("创建画廊缩略图目录失败")?;
    Ok(dir)
}

fn index_path(app: &AppHandle) -> eyre::Result<PathBuf> {
    Ok(gallery_dir(app)?.join("index.json"))
}

fn load_index(app: &AppHandle) -> eyre::Result<GalleryIndex> {
    let path = index_path(app)?;
    if !path.exists() {
        return Ok(GalleryIndex::default());
    }
    let text = fs::read_to_string(&path).wrap_err("读取画廊索引失败")?;
    let mut index: GalleryIndex =
        serde_json::from_str(&text).wrap_err("解析画廊索引失败")?;
    index.version = INDEX_VERSION;
    Ok(index)
}

fn save_index(app: &AppHandle, index: &GalleryIndex) -> eyre::Result<()> {
    let path = index_path(app)?;
    let text = serde_json::to_string_pretty(index).wrap_err("序列化画廊索引失败")?;
    fs::write(path, text).wrap_err("写入画廊索引失败")
}

fn extension_of(content_type: &str) -> &'static str {
    match content_type {
        "image/png" => "png",
        "image/webp" => "webp",
        "image/gif" => "gif",
        _ => "jpg",
    }
}

fn content_type_of(name: &str) -> &'static str {
    match name.rsplit('.').next() {
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        Some("gif") => "image/gif",
        _ => "image/jpeg",
    }
}

/// 生成缩略图：只缩宽，保持比例；解码失败就跳过，卡片退化为原图
fn write_thumb(dir: &PathBuf, id: &str, bytes: &[u8]) -> Option<()> {
    let image = image::load_from_memory(bytes).ok()?;
    let width = image.width();
    let height = image.height();
    if width == 0 || height == 0 {
        return None;
    }
    let target_width = width.min(THUMB_WIDTH);
    let thumb = if target_width < width {
        image.resize(target_width, u32::MAX, image::imageops::FilterType::Triangle)
    } else {
        image
    };
    let mut buffer = Vec::new();
    thumb
        .to_rgb8()
        .write_to(&mut Cursor::new(&mut buffer), image::ImageFormat::Jpeg)
        .ok()?;
    fs::write(dir.join("thumbs").join(format!("{id}.jpg")), buffer).ok()?;
    Some(())
}

#[instrument(level = "error", skip_all, fields(comic_id = comic_id, page = page_index))]
fn add_image(
    app: &AppHandle,
    bytes: Bytes,
    content_type: &str,
    comic_id: i64,
    comic_title: String,
    chapter_id: i64,
    chapter_title: String,
    page_index: i64,
) -> eyre::Result<GalleryItem> {
    let _guard = GALLERY_LOCK.lock();
    let mut index = load_index(app)?;
    if let Some(existing) = index.items.iter().find(|item| {
        item.gallery_id == DEFAULT_GALLERY
            && item.comic_id == comic_id
            && item.chapter_id == chapter_id
            && item.page_index == page_index
    }) {
        return Ok(existing.clone());
    }

    let dir = gallery_dir(app)?;
    let id = Uuid::new_v4().to_string();
    let ext = extension_of(content_type);
    let name = format!("{id}.{ext}");
    fs::write(dir.join("images").join(&name), &bytes).wrap_err("写入画廊图片失败")?;
    let (width, height) = image::image_dimensions(dir.join("images").join(&name))
        .wrap_err("读取画廊图片尺寸失败")?;
    write_thumb(&dir, &id, &bytes);

    let item = GalleryItem {
        id,
        gallery_id: DEFAULT_GALLERY.to_string(),
        comic_id,
        comic_title,
        chapter_id,
        chapter_title,
        page_index,
        width,
        height,
        created_at: now_ms(),
    };
    index.items.push(item.clone());
    index.items.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    save_index(app, &index)?;
    Ok(item)
}

#[tauri::command]
#[specta::specta]
#[instrument(level = "error", skip_all)]
pub fn gallery_list(app: AppHandle) -> CommandResult<Vec<GalleryItem>> {
    let _guard = GALLERY_LOCK.lock();
    load_index(&app)
        .map(|index| index.items)
        .map_err(|err| CommandError::from("读取画廊失败", err))
}

#[tauri::command]
#[specta::specta]
#[instrument(level = "error", skip_all, fields(token = token, index = index))]
pub async fn gallery_add_page(
    app: AppHandle,
    token: u32,
    index: usize,
    comic_id: i64,
    comic_title: String,
    chapter_id: i64,
    chapter_title: String,
    page_index: i64,
) -> CommandResult<GalleryItem> {
    let (bytes, content_type) = read_page_bytes(&app, token, index)
        .await
        .map_err(|err| CommandError::from("读取当前页失败", err))?;
    let app_for_task = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        add_image(
            &app_for_task,
            bytes,
            content_type,
            comic_id,
            comic_title,
            chapter_id,
            chapter_title,
            page_index,
        )
    })
    .await
    .map_err(|err| CommandError::from("收藏到画廊失败", eyre!("转码任务异常: {err}")))?
    .map_err(|err| CommandError::from("收藏到画廊失败", err))
}

#[tauri::command]
#[specta::specta]
#[instrument(level = "error", skip_all, fields(id = id))]
pub fn gallery_remove(app: AppHandle, id: String) -> CommandResult<()> {
    let _guard = GALLERY_LOCK.lock();
    let mut index = load_index(&app).map_err(|err| CommandError::from("读取画廊失败", err))?;
    let Some(item) = index.items.iter().find(|item| item.id == id).cloned() else {
        return Ok(());
    };
    let dir = gallery_dir(&app).map_err(|err| CommandError::from("删除画廊记录失败", err))?;
    // 图片可能已不存在，删除失败不阻塞索引清理
    let images_dir = dir.join("images");
    if let Ok(entries) = fs::read_dir(&images_dir) {
        for entry in entries.flatten() {
            if entry.file_name().to_string_lossy().starts_with(&item.id) {
                let _ = fs::remove_file(entry.path());
            }
        }
    }
    let thumb = dir.join("thumbs").join(format!("{}.jpg", item.id));
    let _ = fs::remove_file(thumb);
    index.items.retain(|item| item.id != id);
    save_index(&app, &index).map_err(|err| CommandError::from("删除画廊记录失败", err))?;
    Ok(())
}

/// 自定义协议：/gallery/{id} 原图，/gallery-thumb/{id} 缩略图
pub fn handle_request(app: &AppHandle, path: &str) -> Option<tauri::http::Response<Vec<u8>>> {
    let (id, thumb) = if let Some(id) = path.strip_prefix("/gallery-thumb/") {
        (id, true)
    } else if let Some(id) = path.strip_prefix("/gallery/") {
        (id, false)
    } else {
        return None;
    };
    if id.is_empty() || !id.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '-') {
        return Some(crate::reader::not_found("bad gallery id"));
    }

    let _guard = GALLERY_LOCK.lock();
    let index = load_index(app).ok()?;
    let item = index.items.iter().find(|item| item.id == id)?.clone();
    let dir = gallery_dir(app).ok()?;
    let original = || -> Option<(PathBuf, &'static str)> {
        let images_dir = dir.join("images");
        let entry = fs::read_dir(&images_dir).ok()?.flatten().find(|entry| {
            entry.file_name().to_string_lossy().starts_with(&item.id)
        })?;
        let content_type = content_type_of(&entry.file_name().to_string_lossy());
        Some((entry.path(), content_type))
    };
    let (file, content_type) = if thumb {
        let thumb_path = dir.join("thumbs").join(format!("{}.jpg", item.id));
        if thumb_path.exists() {
            (thumb_path, "image/jpeg")
        } else {
            original()?
        }
    } else {
        original()?
    };
    let bytes = fs::read(file).ok()?;
    Some(crate::reader::image_response(bytes, content_type))
}
