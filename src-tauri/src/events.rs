use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri_specta::Event;

use crate::{
    downloader::download_task_state::DownloadTaskState,
    types::{ChapterInfo, Comic},
};

#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(tag = "event", content = "data")]
pub enum DownloadEvent {
    #[serde(rename_all = "camelCase")]
    Speed { speed: String },

    #[serde(rename_all = "camelCase")]
    Sleeping { chapter_id: i64, remaining_sec: u64 },

    #[serde(rename_all = "camelCase")]
    TaskCreate {
        state: DownloadTaskState,
        comic: Box<Comic>,
        chapter_info: Box<ChapterInfo>,
        downloaded_img_count: u32,
        total_img_count: u32,
    },

    #[serde(rename_all = "camelCase")]
    TaskDelete { chapter_id: i64 },

    #[serde(rename_all = "camelCase")]
    TaskUpdate {
        chapter_id: i64,
        state: DownloadTaskState,
        downloaded_img_count: u32,
        total_img_count: u32,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(tag = "event", content = "data")]
pub enum DownloadAllFavoritesEvent {
    #[serde(rename_all = "camelCase")]
    GetFavoritesStart,

    #[serde(rename_all = "camelCase")]
    GetComicsProgress { current: i64, total: i64 },

    #[serde(rename_all = "camelCase")]
    StartCreateDownloadTasks {
        comic_id: i64,
        comic_title: String,
        current: i64,
        total: i64,
    },

    #[serde(rename_all = "camelCase")]
    CreatingDownloadTask { comic_id: i64, current: i64 },

    #[serde(rename_all = "camelCase")]
    EndCreateDownloadTasks { comic_id: i64 },

    #[serde(rename_all = "camelCase")]
    GetComicsEnd,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(tag = "event", content = "data")]
pub enum UpdateDownloadedComicsEvent {
    #[serde(rename_all = "camelCase")]
    GetComicStart { total: i64 },

    #[serde(rename_all = "camelCase")]
    GetComicProgress { current: i64, total: i64 },

    #[serde(rename_all = "camelCase")]
    CreateDownloadTasksStart {
        comic_id: i64,
        comic_title: String,
        current: i64,
        total: i64,
    },

    #[serde(rename_all = "camelCase")]
    CreateDownloadTaskProgress { comic_id: i64, current: i64 },

    #[serde(rename_all = "camelCase")]
    CreateDownloadTasksEnd { comic_id: i64 },

    #[serde(rename_all = "camelCase")]
    GetComicEnd,
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(tag = "event", content = "data")]
pub enum ExportCbzEvent {
    #[serde(rename_all = "camelCase")]
    Start {
        uuid: String,
        comic_title: String,
        total: u32,
    },
    #[serde(rename_all = "camelCase")]
    Progress {
        uuid: String,
        /// 已完成的章节数
        current: u32,
        /// 当前章节已下载的图片数（仅直接导出时会上报）
        img_current: Option<u32>,
        /// 当前章节的图片总数（仅直接导出时会上报）
        img_total: Option<u32>,
        /// 当前正在处理的章节标题（仅直接导出时会上报）
        chapter_title: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    Error { uuid: String },
    #[serde(rename_all = "camelCase")]
    End {
        uuid: String,
        comic_id: i64,
        chapter_export_dir: PathBuf,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(tag = "event", content = "data")]
pub enum ExportPdfEvent {
    #[serde(rename_all = "camelCase")]
    CreateStart {
        uuid: String,
        comic_title: String,
        total: u32,
    },
    #[serde(rename_all = "camelCase")]
    CreateProgress { uuid: String, current: u32 },
    #[serde(rename_all = "camelCase")]
    CreateError { uuid: String },
    #[serde(rename_all = "camelCase")]
    CreateEnd {
        uuid: String,
        comic_id: i64,
        chapter_export_dir: PathBuf,
    },

    #[serde(rename_all = "camelCase")]
    MergeStart {
        uuid: String,
        comic_title: String,
        total: u32,
    },
    #[serde(rename_all = "camelCase")]
    MergeProgress { uuid: String, current: u32 },
    #[serde(rename_all = "camelCase")]
    MergeError { uuid: String },
    #[serde(rename_all = "camelCase")]
    MergeEnd {
        uuid: String,
        comic_id: i64,
        chapter_export_dir: PathBuf,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(tag = "event", content = "data")]
pub enum ExportQuickReaderEvent {
    #[serde(rename_all = "camelCase")]
    Start { total: usize },
    #[serde(rename_all = "camelCase")]
    Progress {
        current: usize,
        total: usize,
        indicator: String,
    },
    #[serde(rename_all = "camelCase")]
    End { dir: PathBuf },
    #[serde(rename_all = "camelCase")]
    Error { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize, Type, Event)]
#[serde(rename_all = "camelCase")]
pub struct LogEvent {
    pub json_raw: String,
}
