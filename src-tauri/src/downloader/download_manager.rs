use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::Duration,
};

use eyre::{eyre, WrapErr};
use parking_lot::RwLock;
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
                    .send(())
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
                .send(())
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

    #[instrument(level = "error", skip_all, fields(chapter_id = chapter_id))]
    pub fn delete_download_task(&self, chapter_id: i64) -> eyre::Result<()> {
        let mut tasks = self.download_tasks.write();
        let Some(task) = tasks.remove(&chapter_id) else {
            return Err(eyre!("未找到章节ID为`{chapter_id}`的下载任务"));
        };
        task.delete_sender
            .send(())
            .wrap_err(format!("通知章节ID为`{chapter_id}`的下载任务删除失败"))?;
        Ok(())
    }
}
