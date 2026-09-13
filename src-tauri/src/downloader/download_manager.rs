use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use eyre::{eyre, WrapErr};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};
use tauri_specta::Event;
use tokio::sync::Semaphore;
use tracing::instrument;

use crate::{
    config::Config,
    downloader::{download_task::DownloadTask, download_task_state::DownloadTaskState},
    events::DownloadEvent,
    extensions::EyreReportToMessage,
    types::Comic,
};

/// 任务记录文件名（放在应用数据目录下）
const TASKS_FILE_NAME: &str = "下载任务.json";

/// 持久化的下载任务记录
/// - 只记录没下完的任务，重启后恢复成暂停状态，由用户手动开始
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadTaskRecord {
    pub chapter_id: i64,
    pub comic_id: i64,
    pub comic_title: String,
    pub chapter_title: String,
    pub state: DownloadTaskState,
    pub downloaded_img_count: u32,
    pub total_img_count: u32,
    /// 漫画下载目录：恢复时从这里读 元数据.json 重建 Comic
    pub comic_download_dir: PathBuf,
    pub updated_at: u64,
}

/// 没下完的任务才需要恢复
fn is_unfinished(state: DownloadTaskState) -> bool {
    !matches!(state, DownloadTaskState::Completed)
}

pub struct DownloadManager {
    pub app: AppHandle,
    pub chapter_sem: Arc<Semaphore>,
    pub img_sem: Arc<Semaphore>,
    pub byte_per_sec: Arc<AtomicU64>,
    pub download_tasks: RwLock<HashMap<i64, Arc<DownloadTask>>>,
}

impl DownloadManager {
    pub fn new(app: &AppHandle) -> Self {
        let (chapter_concurrency, img_concurrency) = {
            let config = app.state::<RwLock<Config>>();
            let config = config.read();
            (config.chapter_concurrency, config.img_concurrency)
        };

        let manager = DownloadManager {
            app: app.clone(),
            chapter_sem: Arc::new(Semaphore::new(chapter_concurrency)),
            img_sem: Arc::new(Semaphore::new(img_concurrency)),
            byte_per_sec: Arc::new(AtomicU64::new(0)),
            download_tasks: RwLock::new(HashMap::new()),
        };

        tauri::async_runtime::spawn(Self::emit_download_speed_loop(
            manager.app.clone(),
            manager.byte_per_sec.clone(),
        ));

        manager
    }

    #[instrument(
        level = "error",
        skip_all,
        fields(comic_id = comic.id, comic_title = comic.name)
    )]
    pub fn create_download_tasks(&self, mut comic: Comic, chapter_ids: &[i64]) {
        use DownloadTaskState::{Downloading, Paused, Pending};

        if let Err(err) = comic.ensure_download_dir_fields(&self.app) {
            let err_title = "批量创建下载任务失败";
            let message = err.to_message();
            tracing::error!(err_title, message);
            return;
        }

        let mut tasks = self.download_tasks.write();
        for chapter_id in chapter_ids {
            let span = tracing::error_span!("create_download_task", chapter_id = chapter_id);
            let _enter = span.enter();

            if let Some(task) = tasks.get(chapter_id) {
                let state = *task.state_sender.borrow();
                if matches!(state, Pending | Downloading | Paused) {
                    let err_title = "章节ID对应的下载任务创建失败";
                    let message = eyre!("章节ID对应的下载任务已存在").to_message();
                    tracing::error!(err_title, message);
                    continue;
                }
            }

            if let Some(task) = tasks.remove(chapter_id) {
                if let Err(err) = task
                    .delete_sender
                    .send(false)
                    .wrap_err("章节ID对应的旧下载任务删除失败")
                {
                    let err_title = "章节ID对应的下载任务创建失败";
                    let message = err.to_message();
                    tracing::error!(err_title, message);
                    continue;
                }
            }

            let task = match DownloadTask::new(self.app.clone(), comic.clone(), *chapter_id) {
                Ok(task) => task,
                Err(err) => {
                    let err_title = "章节ID对应的下载任务创建失败";
                    let message = err.to_message();
                    tracing::error!(err_title, message);
                    continue;
                }
            };

            tasks.insert(*chapter_id, task);
        }

        drop(tasks);
        self.save_tasks();
    }

    async fn emit_download_speed_loop(app: AppHandle, byte_per_sec: Arc<AtomicU64>) {
        let mut interval = tokio::time::interval(Duration::from_secs(1));

        loop {
            interval.tick().await;
            let byte_per_sec = byte_per_sec.swap(0, Ordering::Relaxed);
            #[allow(clippy::cast_precision_loss)]
            let mega_byte_per_sec = byte_per_sec as f64 / 1024.0 / 1024.0;
            let speed = format!("{mega_byte_per_sec:.2}MB/s");
            let _ = DownloadEvent::Speed { speed }.emit(&app);
        }
    }

    #[instrument(
        level = "error",
        skip_all,
        fields(
            comic_id = comic.id,
            comic_title = comic.name,
            chapter_id = chapter_id
        )
    )]
    pub fn create_download_task(&self, comic: Comic, chapter_id: i64) -> eyre::Result<()> {
        use DownloadTaskState::{Downloading, Paused, Pending};

        let comic_title = comic.name.clone();
        let mut tasks = self.download_tasks.write();

        if let Some(task) = tasks.get(&chapter_id) {
            let state = *task.state_sender.borrow();
            if matches!(state, Pending | Downloading | Paused) {
                return Err(eyre!("章节ID为`{chapter_id}`的下载任务已存在"));
            }
        }

        if let Some(task) = tasks.remove(&chapter_id) {
            if let Err(err) = task
                .delete_sender
                .send(false)
                .wrap_err(format!("章节ID为`{chapter_id}`的旧下载任务删除失败"))
            {
                let err_title =
                    format!("`{comic_title}`的章节ID为`{chapter_id}`的下载任务替换失败");
                let message = err.to_message();
                tracing::error!(err_title, message);
            }
        }

        let task = DownloadTask::new(self.app.clone(), comic, chapter_id)
            .wrap_err("DownloadTask创建失败")?;

        tasks.insert(chapter_id, task);

        drop(tasks);
        self.save_tasks();

        Ok(())
    }

    #[instrument(level = "error", skip_all, fields(chapter_id = chapter_id))]
    pub fn pause_download_task(&self, chapter_id: i64) -> eyre::Result<()> {
        let tasks = self.download_tasks.read();
        let Some(task) = tasks.get(&chapter_id) else {
            return Err(eyre!("未找到章节ID为`{chapter_id}`的下载任务"));
        };
        task.set_state(DownloadTaskState::Paused);
        Ok(())
    }

    #[instrument(level = "error", skip_all, fields(chapter_id = chapter_id))]
    pub fn resume_download_task(&self, chapter_id: i64) -> eyre::Result<()> {
        let tasks = self.download_tasks.read();
        let Some(task) = tasks.get(&chapter_id) else {
            return Err(eyre!("未找到章节ID为`{chapter_id}`的下载任务"));
        };
        task.set_state(DownloadTaskState::Pending);
        Ok(())
    }

    /// 删除下载任务
    /// - delete_files 为 true 时，连这一话已下载的文件夹（临时目录 + 正式目录）一起删掉
    #[instrument(level = "error", skip_all, fields(chapter_id = chapter_id))]
    pub fn delete_download_task(&self, chapter_id: i64, delete_files: bool) -> eyre::Result<()> {
        let task = {
            let mut tasks = self.download_tasks.write();
            let Some(task) = tasks.remove(&chapter_id) else {
                return Err(eyre!("未找到章节ID为 `{chapter_id}` 的下载任务"));
            };
            task
        };

        task.delete_sender
            .send(delete_files)
            .wrap_err(format!("通知章节ID为 `{chapter_id}` 的下载任务删除失败"))?;

        self.save_tasks();

        Ok(())
    }

    /// 退出软件前调用：把所有没结束的任务标记为暂停，并写进任务记录
    pub fn pause_all_tasks(&self) {
        let tasks: Vec<Arc<DownloadTask>> = self.download_tasks.read().values().cloned().collect();

        for task in tasks {
            let state = *task.state_sender.borrow();
            if matches!(state, DownloadTaskState::Pending | DownloadTaskState::Downloading) {
                task.set_state(DownloadTaskState::Paused);
            }
        }

        self.save_tasks();
    }

    fn tasks_file_path(&self) -> eyre::Result<PathBuf> {
        Ok(self.app.path().app_data_dir()?.join(TASKS_FILE_NAME))
    }

    /// 把没下完的任务写到磁盘
    pub fn save_tasks(&self) {
        if let Err(err) = self.save_tasks_inner() {
            tracing::error!(message = %err, "保存下载任务记录失败");
        }
    }

    fn save_tasks_inner(&self) -> eyre::Result<()> {
        let records: Vec<DownloadTaskRecord> = {
            let tasks = self.download_tasks.read();
            tasks
                .values()
                .filter(|task| is_unfinished(*task.state_sender.borrow()))
                .map(|task| DownloadTaskRecord {
                    chapter_id: task.chapter_info.chapter_id,
                    comic_id: task.comic.id,
                    comic_title: task.comic.name.clone(),
                    chapter_title: task.chapter_info.chapter_title.clone(),
                    state: *task.state_sender.borrow(),
                    downloaded_img_count: task.downloaded_img_count.load(Ordering::Relaxed),
                    total_img_count: task.total_img_count.load(Ordering::Relaxed),
                    comic_download_dir: task.comic.comic_download_dir.clone().unwrap_or_default(),
                    updated_at: SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map(|duration| duration.as_secs())
                        .unwrap_or_default(),
                })
                .collect()
        };

        let tasks_file = self.tasks_file_path()?;
        let json = serde_json::to_string_pretty(&records)?;
        std::fs::write(&tasks_file, json)
            .wrap_err(format!("写入 {} 失败", tasks_file.display()))?;

        Ok(())
    }

    /// 启动时恢复上次没下完的任务：一律恢复成暂停状态，由用户手动继续
    pub fn restore_tasks(&self) {
        if let Err(err) = self.restore_tasks_inner() {
            tracing::error!(message = %err, "恢复下载任务失败");
        }
    }

    fn restore_tasks_inner(&self) -> eyre::Result<()> {
        let tasks_file = self.tasks_file_path()?;
        if !tasks_file.exists() {
            return Ok(());
        }

        let json = std::fs::read_to_string(&tasks_file)
            .wrap_err(format!("读取 {} 失败", tasks_file.display()))?;
        let records: Vec<DownloadTaskRecord> = match serde_json::from_str(&json) {
            Ok(records) => records,
            Err(err) => {
                tracing::error!(message = %err, "解析下载任务记录失败，已忽略");
                return Ok(());
            }
        };

        let mut restored: Vec<(i64, Arc<DownloadTask>)> = Vec::new();
        for record in records {
            let metadata_path = record.comic_download_dir.join("元数据.json");
            if !metadata_path.exists() {
                // 漫画目录已经被删掉了，这条记录没用了
                continue;
            }

            let comic = match Comic::from_metadata(&metadata_path) {
                Ok(comic) => comic,
                Err(err) => {
                    tracing::error!(
                        message = %err,
                        path = %metadata_path.display(),
                        "恢复下载任务时读取元数据失败，已跳过"
                    );
                    continue;
                }
            };

            let chapter_id = record.chapter_id;
            match DownloadTask::new_with_state(
                self.app.clone(),
                comic,
                chapter_id,
                DownloadTaskState::Paused,
                record.downloaded_img_count,
                record.total_img_count,
            ) {
                Ok(task) => restored.push((chapter_id, task)),
                Err(err) => {
                    tracing::error!(message = %err, chapter_id, "恢复下载任务失败，已跳过");
                }
            }
        }

        let count = restored.len();
        {
            let mut tasks = self.download_tasks.write();
            for (chapter_id, task) in restored {
                tasks.insert(chapter_id, task);
            }
        }

        if count > 0 {
            tracing::info!(count, "已恢复未完成的下载任务（全部为暂停状态）");
            self.save_tasks();
        }

        Ok(())
    }

    /// 前端挂载后主动同步一次，把恢复出来的任务补进进度列表
    pub fn sync_tasks(&self) {
        let tasks: Vec<Arc<DownloadTask>> = {
            let tasks = self.download_tasks.read();
            tasks.values().cloned().collect()
        };

        tracing::debug!(count = tasks.len(), "同步下载任务到进度列表");

        for task in tasks {
            task.emit_download_task_create_event();
            task.emit_download_task_update_event();
        }
    }
}
