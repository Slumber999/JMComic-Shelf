mod cbz;
mod cbz_direct;
pub mod manager;
mod pdf;

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::Arc,
};

pub use cbz::{cbz, cbz_chapters};
pub use cbz_direct::{
    export_cbz_chapters_without_download, export_cbz_without_download, export_missing_chapters,
    resume_comic_cbz,
};
use eyre::{OptionExt, WrapErr};
use parking_lot::Mutex;
pub use pdf::{pdf, pdf_chapters};
use tauri::AppHandle;
use tauri_specta::Event;
use tracing::instrument;

use crate::{
    events::ExportCbzEvent,
    extensions::PathIsImg,
    types::{ChapterInfo, Comic},
};

/// 导出互斥锁管理器，确保同一漫画的导出操作串行执行
#[derive(Debug, Clone, Default)]
pub struct ComicExportLock {
    /// 正在导出的漫画 ID 集合
    locked_comic_ids: Arc<Mutex<HashSet<i64>>>,
}

impl ComicExportLock {
    pub fn new() -> Self {
        Self {
            locked_comic_ids: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// 尝试获取漫画导出锁，返回是否成功
    pub fn try_acquire(&self, comic_id: i64) -> bool {
        let mut locked = self.locked_comic_ids.lock();
        if locked.contains(&comic_id) {
            return false;
        }
        locked.insert(comic_id);
        true
    }

    /// 释放漫画导出锁
    pub fn release(&self, comic_id: i64) {
        self.locked_comic_ids.lock().remove(&comic_id);
    }
}

pub struct ComicExportLockGuard {
    lock: ComicExportLock,
    comic_id: i64,
}

impl Drop for ComicExportLockGuard {
    fn drop(&mut self) {
        self.lock.release(self.comic_id);
    }
}

/// 导出失败时补发一个 cbz Error 事件：Drop 时 success 仍为 false 就触发
/// - 打包下载目录的 cbz 和免下载直出两条路径共用，不再各写一份 guard
pub(crate) struct CbzErrorEventGuard {
    pub(crate) uuid: String,
    pub(crate) app: AppHandle,
    pub(crate) success: bool,
}

impl Drop for CbzErrorEventGuard {
    fn drop(&mut self) {
        if self.success {
            return;
        }
        let _ = ExportCbzEvent::Error {
            uuid: self.uuid.clone(),
        }
        .emit(&self.app);
    }
}

/// 导出格式
pub enum ExportFormat {
    Pdf,
    Cbz,
}

impl ExportFormat {
    pub fn extension(&self) -> &str {
        match self {
            ExportFormat::Pdf => "pdf",
            ExportFormat::Cbz => "cbz",
        }
    }
}

/// 获取已下载的章节
fn get_downloaded_chapters(chapter_infos: &[ChapterInfo]) -> Vec<ChapterInfo> {
    chapter_infos
        .iter()
        .filter(|chapter_info| chapter_info.is_downloaded.unwrap_or(false))
        .cloned()
        .collect()
}

/// 根据章节 ID 列表获取已下载的章节
fn get_downloaded_chapters_by_ids(
    chapter_infos: &[ChapterInfo],
    chapter_ids: &[i64],
) -> Vec<ChapterInfo> {
    let chapter_id_set: HashSet<_> = chapter_ids.iter().copied().collect();
    chapter_infos
        .iter()
        .filter(|chapter_info| {
            chapter_info.is_downloaded.unwrap_or(false)
                && chapter_id_set.contains(&chapter_info.chapter_id)
        })
        .cloned()
        .collect()
}

/// 章节的导出位置：返回 (章节下载目录, 导出文件路径)
/// - 导出目录保持下载目录的相对层级（`{author}/{comic_title}` 这种多级格式也成立）
/// - 文件名用章节下载目录名，和「导出目录里已有哪些文件」的判定保持同一套规则
/// - cbz / pdf 两条导出路径原来各写了一遍这段目录推导
fn chapter_export_path(
    comic: &Comic,
    chapter_info: &ChapterInfo,
    format_dir: &Path,
    extension: &str,
) -> eyre::Result<(PathBuf, PathBuf)> {
    let chapter_download_dir = chapter_info
        .chapter_download_dir
        .as_ref()
        .ok_or_eyre("`chapter_download_dir`字段为`None`")?;
    let chapter_download_dir_name = chapter_info
        .get_chapter_download_dir_name()
        .wrap_err("获取章节下载目录名失败")?;
    let chapter_relative_dir = chapter_info
        .get_chapter_relative_dir(comic)
        .wrap_err("获取章节相对目录失败")?;
    let chapter_export_dir = match chapter_relative_dir.parent() {
        Some(parent) => format_dir.join(parent),
        None => format_dir.to_path_buf(),
    };
    std::fs::create_dir_all(&chapter_export_dir)
        .wrap_err(format!("创建目录`{}`失败", chapter_export_dir.display()))?;

    Ok((
        chapter_download_dir.clone(),
        chapter_export_dir.join(format!("{chapter_download_dir_name}.{extension}")),
    ))
}

#[instrument(
    level = "error",
    skip_all,
    fields(images_dir = %images_dir.display(), must_be_common_img = must_be_common_img)
)]
fn get_image_paths(images_dir: &Path, must_be_common_img: bool) -> eyre::Result<Vec<PathBuf>> {
    let mut image_paths: Vec<PathBuf> = std::fs::read_dir(images_dir)
        .wrap_err(format!("读取目录`{}`失败", images_dir.display()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            if must_be_common_img {
                path.is_common_img()
            } else {
                path.is_img()
            }
        })
        .collect();
    image_paths.sort_by(|a, b| a.file_name().cmp(&b.file_name()));
    Ok(image_paths)
}
