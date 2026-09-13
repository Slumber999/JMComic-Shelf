//! 导出任务管理：记录导出任务、暂停/继续、删除、持久化
//!
//! - 一个任务 = 一次导出（一本漫画的 cbz 或免下载直出）
//! - 暂停：把状态改成 Paused，导出循环在「章节之间」检查到就停下（当前章节会做完）
//! - 继续：用「跳过已存在的文件」重新跑一遍，已导完的章节自然跳过，
//!   被暂停打断的那一章从头再来（这一章的文件还没写完）
//! - 退出软件时全部暂停并存盘，下次启动恢复成暂停状态，由用户手动继续

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
    time::{SystemTime, UNIX_EPOCH},
};

use eyre::eyre;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager};
use tauri_specta::Event;
use tokio::sync::watch;

use crate::{events::ExportTaskEvent, extensions::AppHandleExt};

const TASKS_FILE_NAME: &str = "导出任务.json";

/// 导出类型（pdf 暂时没有任务，保持原样）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
pub enum ExportTaskKind {
    Cbz,
    CbzDirect,
}

/// 导出任务状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
pub enum ExportTaskState {
    Exporting,
    Paused,
    Completed,
    Failed,
}

/// 持久化的导出任务记录
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ExportTaskRecord {
    pub uuid: String,
    pub comic_id: i64,
    pub comic_title: String,
    pub kind: ExportTaskKind,
    pub state: ExportTaskState,
    pub done: u32,
    pub total: u32,
    /// 这本漫画的导出目录（删除任务和文件时用）
    pub comic_export_dir: PathBuf,
    pub updated_at: u64,
}

/// 一个导出任务
pub struct ExportTask {
    pub uuid: String,
    pub app: AppHandle,
    pub comic_id: i64,
    pub comic_title: String,
    pub kind: ExportTaskKind,
    pub comic_export_dir: PathBuf,
    state_sender: watch::Sender<ExportTaskState>,
    delete_sender: watch::Sender<bool>,
    done: AtomicU32,
    total: AtomicU32,
}

impl ExportTask {
    pub fn state(&self) -> ExportTaskState {
        *self.state_sender.borrow()
    }

    /// 导出循环用它判断要不要停下（在章节之间检查）
    pub fn is_paused(&self) -> bool {
        self.state() == ExportTaskState::Paused
    }

    /// 任务是否已经被删除（删除后正在跑的导出要尽快停下）
    pub fn is_deleted(&self) -> bool {
        *self.delete_sender.borrow()
    }

    pub fn set_state(&self, state: ExportTaskState) {
        // 注意：必须用 send_replace，watch::Sender::send 在没有接收者时会失败且不更新值
        self.state_sender.send_replace(state);
        self.emit_state_event();
        self.app.get_export_manager().save_tasks();
    }

    pub fn set_progress(&self, done: u32, total: u32) {
        self.done.store(done, Ordering::Relaxed);
        self.total.store(total, Ordering::Relaxed);
        self.emit_state_event();
    }

    pub fn done(&self) -> u32 {
        self.done.load(Ordering::Relaxed)
    }

    pub fn total(&self) -> u32 {
        self.total.load(Ordering::Relaxed)
    }

    pub fn record(&self) -> ExportTaskRecord {
        ExportTaskRecord {
            uuid: self.uuid.clone(),
            comic_id: self.comic_id,
            comic_title: self.comic_title.clone(),
            kind: self.kind,
            state: self.state(),
            done: self.done(),
            total: self.total(),
            comic_export_dir: self.comic_export_dir.clone(),
            updated_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_secs())
                .unwrap_or_default(),
        }
    }

    pub fn emit_state_event(&self) {
        let _ = ExportTaskEvent::StateChanged {
            uuid: self.uuid.clone(),
            kind: self.kind,
            state: self.state(),
            comic_id: self.comic_id,
            comic_title: self.comic_title.clone(),
            done: self.done(),
            total: self.total(),
            comic_export_dir: self.comic_export_dir.clone(),
        }
        .emit(&self.app);
    }
}

/// 导出任务管理器
pub struct ExportManager {
    pub app: AppHandle,
    pub tasks: RwLock<HashMap<String, Arc<ExportTask>>>,
}

impl ExportManager {
    pub fn new(app: &AppHandle) -> Self {
        Self {
            app: app.clone(),
            tasks: RwLock::new(HashMap::new()),
        }
    }

    pub fn get(&self, uuid: &str) -> Option<Arc<ExportTask>> {
        self.tasks.read().get(uuid).cloned()
    }

    /// 创建一个导出任务（状态为 Exporting）
    pub fn create_task(
        &self,
        uuid: String,
        comic_id: i64,
        comic_title: String,
        kind: ExportTaskKind,
        total: u32,
        comic_export_dir: PathBuf,
    ) -> Arc<ExportTask> {
        let (state_sender, _) = watch::channel(ExportTaskState::Exporting);
        let (delete_sender, _) = watch::channel(false);

        let task = Arc::new(ExportTask {
            uuid: uuid.clone(),
            app: self.app.clone(),
            comic_id,
            comic_title,
            kind,
            comic_export_dir,
            state_sender,
            delete_sender,
            done: AtomicU32::new(0),
            total: AtomicU32::new(total),
        });

        self.tasks.write().insert(uuid, task.clone());
        self.save_tasks();

        task
    }

    /// 删除任务；delete_files 为 true 时连这本漫画的导出目录一起删掉
    pub fn delete_task(&self, uuid: &str, delete_files: bool) -> eyre::Result<()> {
        let task = {
            let mut tasks = self.tasks.write();
            let Some(task) = tasks.remove(uuid) else {
                return Err(eyre!("未找到导出任务 {}", uuid));
            };
            task
        };

        task.delete_sender.send_replace(delete_files);

        if delete_files {
            delete_dir(&task.comic_export_dir);
        }

        let _ = ExportTaskEvent::Deleted {
            uuid: uuid.to_string(),
        }
        .emit(&self.app);

        self.save_tasks();
        crate::local_index::invalidate();

        Ok(())
    }

    /// 退出软件前调用：把所有没结束的任务标记为暂停并落盘
    pub fn pause_all_tasks(&self) {
        let tasks: Vec<Arc<ExportTask>> = self.tasks.read().values().cloned().collect();

        for task in tasks {
            if task.state() == ExportTaskState::Exporting {
                task.set_state(ExportTaskState::Paused);
            }
        }

        self.save_tasks();
    }

    fn tasks_file_path(&self) -> eyre::Result<PathBuf> {
        Ok(self.app.path().app_data_dir()?.join(TASKS_FILE_NAME))
    }

    pub fn save_tasks(&self) {
        if let Err(err) = self.save_tasks_inner() {
            tracing::error!(message = %err, "保存导出任务记录失败");
        }
    }

    fn save_tasks_inner(&self) -> eyre::Result<()> {
        let records: Vec<ExportTaskRecord> = {
            let tasks = self.tasks.read();
            tasks
                .values()
                .filter(|task| task.state() != ExportTaskState::Completed)
                .map(|task| task.record())
                .collect()
        };

        let tasks_file = self.tasks_file_path()?;
        let json = serde_json::to_string_pretty(&records)?;
        std::fs::write(&tasks_file, json).map_err(eyre::Report::from)?;

        Ok(())
    }

    /// 启动时恢复上次没导完的任务：一律恢复成暂停状态
    pub fn restore_tasks(&self) {
        if let Err(err) = self.restore_tasks_inner() {
            tracing::error!(message = %err, "恢复导出任务失败");
        }
    }

    fn restore_tasks_inner(&self) -> eyre::Result<()> {
        let tasks_file = self.tasks_file_path()?;
        if !tasks_file.exists() {
            return Ok(());
        }

        let json = std::fs::read_to_string(&tasks_file).map_err(eyre::Report::from)?;
        let records: Vec<ExportTaskRecord> = match serde_json::from_str(&json) {
            Ok(records) => records,
            Err(err) => {
                tracing::error!(message = %err, "解析导出任务记录失败，已忽略");
                return Ok(());
            }
        };

        if records.is_empty() {
            return Ok(());
        }

        let mut restored: Vec<Arc<ExportTask>> = Vec::new();
        for record in records {
            if !record.comic_export_dir.exists() {
                continue;
            }

            let (state_sender, _) = watch::channel(ExportTaskState::Paused);
            let (delete_sender, _) = watch::channel(false);
            restored.push(Arc::new(ExportTask {
                uuid: record.uuid.clone(),
                app: self.app.clone(),
                comic_id: record.comic_id,
                comic_title: record.comic_title,
                kind: record.kind,
                comic_export_dir: record.comic_export_dir,
                state_sender,
                delete_sender,
                done: AtomicU32::new(record.done),
                total: AtomicU32::new(record.total),
            }));
        }

        let count = restored.len();
        {
            let mut tasks = self.tasks.write();
            for task in restored {
                tasks.insert(task.uuid.clone(), task);
            }
        }

        if count > 0 {
            tracing::info!(count, "已恢复未完成的导出任务（全部为暂停状态）");
        }

        Ok(())
    }

    /// 前端挂载后主动同步一次，把恢复出来的任务补进列表
    pub fn sync_tasks(&self) {
        let tasks: Vec<Arc<ExportTask>> = {
            let tasks = self.tasks.read();
            tasks.values().cloned().collect()
        };

        tracing::debug!(count = tasks.len(), "同步导出任务到进度列表");

        for task in tasks {
            task.emit_state_event();
        }
    }
}

/// 删掉一个目录（不存在或删失败都只记日志）
fn delete_dir(dir: &std::path::Path) {
    if !dir.exists() {
        return;
    }

    if let Err(err) = std::fs::remove_dir_all(dir) {
        tracing::error!(path = %dir.display(), message = %err, "删除导出文件失败");
    } else {
        tracing::info!(path = %dir.display(), "已删除导出文件");
    }
}
