mod cbz;
mod cbz_direct;
mod pdf;

use std::{
    collections::HashSet,
    path::{Path, PathBuf},
    sync::Arc,
};

pub use cbz::{cbz, cbz_chapters};
pub use cbz_direct::export_cbz_without_download;
use eyre::WrapErr;
use parking_lot::Mutex;
pub use pdf::{pdf, pdf_chapters};
use tracing::instrument;

use crate::{extensions::PathIsImg, types::ChapterInfo};

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
