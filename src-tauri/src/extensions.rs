use parking_lot::RwLock;
use tauri::{Manager, State};

use crate::{
    config::Config, downloader::download_manager::DownloadManager, export::ComicExportLock,
    jm_client::JmClient,
};

pub trait EyreReportToMessage {
    fn to_message(&self) -> String;
}

impl EyreReportToMessage for eyre::Report {
    fn to_message(&self) -> String {
        format!("{self:?}")
    }
}

pub trait PathIsImg {
    /// 判断路径是否为图片(jpg/png/webp/gif)
    fn is_img(&self) -> bool;

    /// 判断路径是否为普通图片(jpg/png/webp)
    fn is_common_img(&self) -> bool;
}

impl PathIsImg for std::path::Path {
    fn is_img(&self) -> bool {
        self.extension()
            .and_then(|ext| ext.to_str())
            .map(str::to_lowercase)
            .is_some_and(|ext| matches!(ext.as_str(), "jpg" | "png" | "webp" | "gif"))
    }

    fn is_common_img(&self) -> bool {
        self.extension()
            .and_then(|ext| ext.to_str())
            .map(str::to_lowercase)
            .is_some_and(|ext| matches!(ext.as_str(), "jpg" | "png" | "webp"))
    }
}

pub trait WalkDirEntryExt {
    fn is_comic_metadata(&self) -> bool;
    fn is_chapter_metadata(&self) -> bool;
}
impl WalkDirEntryExt for walkdir::DirEntry {
    fn is_comic_metadata(&self) -> bool {
        if !self.file_type().is_file() {
            return false;
        }
        if self.file_name() != "元数据.json" {
            return false;
        }

        true
    }

    fn is_chapter_metadata(&self) -> bool {
        if !self.file_type().is_file() {
            return false;
        }
        if self.file_name() != "章节元数据.json" {
            return false;
        }

        true
    }
}

pub trait AppHandleExt {
    fn get_config(&self) -> State<'_, RwLock<Config>>;
    fn get_jm_client(&self) -> State<'_, JmClient>;
    fn get_download_manager(&self) -> State<'_, DownloadManager>;
    fn get_export_lock(&self) -> State<'_, ComicExportLock>;
}

impl AppHandleExt for tauri::AppHandle {
    fn get_config(&self) -> State<'_, RwLock<Config>> {
        self.state::<RwLock<Config>>()
    }
    fn get_jm_client(&self) -> State<'_, JmClient> {
        self.state::<JmClient>()
    }
    fn get_download_manager(&self) -> State<'_, DownloadManager> {
        self.state::<DownloadManager>()
    }
    fn get_export_lock(&self) -> State<'_, ComicExportLock> {
        self.state::<ComicExportLock>()
    }
}
