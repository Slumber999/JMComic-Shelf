use std::{
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
    time::Duration,
};

use eyre::{eyre, OptionExt, WrapErr};
use tauri::AppHandle;
use tauri_specta::Event;
use tokio::{
    sync::{watch, SemaphorePermit},
    task::JoinSet,
    time::sleep,
};
use tracing::{instrument, Instrument};

use crate::{
    downloader::{
        download_img_task::{calculate_block_num, DownloadImgTask},
        download_task_state::DownloadTaskState,
    },
    events::DownloadEvent,
    extensions::{AppHandleExt, EyreReportToMessage},
    jm_client::IMAGE_DOMAIN,
    types::{ChapterInfo, Comic},
};

pub struct DownloadTask {
    pub app: AppHandle,
    pub comic: Arc<Comic>,
    pub chapter_info: Arc<ChapterInfo>,
    pub state_sender: watch::Sender<DownloadTaskState>,
    pub delete_sender: watch::Sender<()>,
    pub downloaded_img_count: Arc<AtomicU32>,
    pub total_img_count: Arc<AtomicU32>,
}

impl DownloadTask {
    #[instrument(
        level = "error",
        skip_all,
        fields(
            comic_id = comic.id,
            comic_title = comic.name,
            chapter_id = chapter_id
        )
    )]
    pub fn new(app: AppHandle, mut comic: Comic, chapter_id: i64) -> eyre::Result<Arc<Self>> {
        comic
            .ensure_download_dir_fields(&app)
            .wrap_err("更新下载目录字段失败")?;

        let chapter_info = comic
            .chapter_infos
            .iter()
            .find(|chapter| chapter.chapter_id == chapter_id)
            .cloned()
            .ok_or_eyre(format!("未找到章节ID为`{chapter_id}`的章节信息"))?;

        let (state_sender, _) = watch::channel(DownloadTaskState::Pending);
        let (delete_sender, _) = watch::channel(());

        let task = Arc::new(Self {
            app,
            comic: Arc::new(comic),
            chapter_info: Arc::new(chapter_info),
            state_sender,
            delete_sender,
            downloaded_img_count: Arc::new(AtomicU32::new(0)),
            total_img_count: Arc::new(AtomicU32::new(0)),
        });

        tauri::async_runtime::spawn(task.clone().process());

        Ok(task)
    }

    #[instrument(
        level = "error",
        skip_all,
        fields(
            comic_id = self.comic.id,
            comic_title = self.comic.name,
            chapter_id = self.chapter_info.chapter_id,
            chapter_title = self.chapter_info.chapter_title,
            order = self.chapter_info.order
        )
    )]
    async fn process(self: Arc<Self>) {
        self.emit_download_task_create_event();

        let mut state_receiver = self.state_sender.subscribe();
        state_receiver.mark_changed();

        let mut delete_receiver = self.delete_sender.subscribe();

        let mut permit = None;
        let mut download_task_option = None;

        loop {
            let state = *state_receiver.borrow();
            let state_is_downloading = state == DownloadTaskState::Downloading;
            let state_is_pending = state == DownloadTaskState::Pending;

            let download_task = async {
                download_task_option
                    .get_or_insert_with(|| Box::pin(self.download_chapter()))
                    .await;
            };

            tokio::select! {
                () = download_task, if state_is_downloading && permit.is_some() => {
                    download_task_option = None;
                    if let Some(permit) = permit.take() {
                        drop(permit);
                    }
                }

                () = self.acquire_chapter_permit(&mut permit), if state_is_pending => {}

                _ = state_receiver.changed() => {
                    self.handle_state_change(&mut permit, &mut state_receiver).await;
                }

                _ = delete_receiver.changed() => {
                    self.handle_delete_receiver_change(&mut permit).await;
                    return;
                }
            }
        }
    }

    #[instrument(level = "error", skip_all)]
    async fn download_chapter(self: &Arc<Self>) {
        let chapter_id = self.chapter_info.chapter_id;

        if let Err(err) = self.comic.save_comic_metadata() {
            let err_title = "保存元数据失败";
            let message = err.to_message();
            tracing::error!(err_title, message);

            self.set_state(DownloadTaskState::Failed);
            self.emit_download_task_update_event();

            return;
        }

        let should_download_cover = self.app.get_config().read().should_download_cover;
        if should_download_cover {
            if let Err(err) = self.download_cover().await {
                let err_title = "下载封面失败";
                let message = err.to_message();
                tracing::error!(err_title, message);

                self.set_state(DownloadTaskState::Failed);
                self.emit_download_task_update_event();

                return;
            }
        }

        let Some(urls_with_block_num) = self.get_urls_with_block_num(chapter_id).await else {
            return;
        };

        #[allow(clippy::cast_possible_truncation)]
        self.total_img_count
            .fetch_add(urls_with_block_num.len() as u32, Ordering::Relaxed);

        let Some(temp_download_dir) = self.create_temp_download_dir() else {
            return;
        };

        self.clean_temp_download_dir(&temp_download_dir);

        let mut join_set = JoinSet::new();
        for (i, (url, block_num)) in urls_with_block_num.into_iter().enumerate() {
            let temp_download_dir = temp_download_dir.clone();
            let download_img_task =
                DownloadImgTask::new(self.clone(), url, i, temp_download_dir, block_num);
            join_set.spawn(download_img_task.process().in_current_span());
        }
        join_set.join_all().await;

        tracing::trace!("所有图片下载任务完成");

        let downloaded_img_count = self.downloaded_img_count.load(Ordering::Relaxed);
        let total_img_count = self.total_img_count.load(Ordering::Relaxed);
        if downloaded_img_count != total_img_count {
            let err_title = "下载不完整";
            let message =
                eyre!("总共有`{total_img_count}`张图片，但只下载了`{downloaded_img_count}`张")
                    .to_message();
            tracing::error!(err_title, message);

            self.set_state(DownloadTaskState::Failed);
            self.emit_download_task_update_event();

            return;
        }

        if let Err(err) = self.rename_temp_download_dir(&temp_download_dir) {
            let err_title = "保存下载目录失败";
            let message = err.to_message();
            tracing::error!(err_title, message);

            self.set_state(DownloadTaskState::Failed);
            self.emit_download_task_update_event();

            return;
        }

        if let Err(err) = self.chapter_info.save_chapter_metadata() {
            let err_title = "保存章节元数据失败";
            let message = err.to_message();
            tracing::error!(err_title, message);
        }

        self.sleep_between_chapter().await;
        tracing::info!("章节下载成功");

        self.set_state(DownloadTaskState::Completed);
        self.emit_download_task_update_event();

        // 新的漫画目录/章节内容出现了，本地库索引要重建（否则"已下载"标记会等到 TTL 过期）
        crate::local_index::invalidate();
    }

    #[instrument(level = "error", skip_all)]
    async fn download_cover(&self) -> eyre::Result<()> {
        let cover_path = self.comic.get_cover_path().wrap_err("获取封面路径失败")?;

        let comic_id = self.comic.id;
        let url = format!("https://cdn-msp3.18comic.vip/media/albums/{comic_id}.jpg");

        let (img_data, _format) = self
            .app
            .get_jm_client()
            .get_img_data_and_format(&url)
            .await
            .wrap_err(format!("下载图片`{url}`失败"))?;

        std::fs::write(&cover_path, img_data)
            .wrap_err(format!("保存图片`{}`失败", cover_path.display()))?;

        Ok(())
    }

    #[instrument(level = "error", skip_all)]
    fn create_temp_download_dir(&self) -> Option<PathBuf> {
        let temp_download_dir = match self.chapter_info.get_temp_download_dir() {
            Ok(temp_download_dir) => temp_download_dir,
            Err(err) => {
                let err_title = "获取临时下载目录失败";
                let message = err.to_message();
                tracing::error!(err_title, message);

                self.set_state(DownloadTaskState::Failed);
                self.emit_download_task_update_event();

                return None;
            }
        };

        if let Err(err) = std::fs::create_dir_all(&temp_download_dir).map_err(eyre::Report::from) {
            let err_title = "创建临时下载目录失败";
            let message = err.to_message();
            tracing::error!(err_title, message);

            self.set_state(DownloadTaskState::Failed);
            self.emit_download_task_update_event();

            return None;
        }

        tracing::trace!("创建临时下载目录成功");

        Some(temp_download_dir)
    }

    #[instrument(level = "error", skip_all, fields(temp_download_dir = %temp_download_dir.display()))]
    fn rename_temp_download_dir(&self, temp_download_dir: &Path) -> eyre::Result<()> {
        let chapter_download_dir = self
            .chapter_info
            .chapter_download_dir
            .as_ref()
            .ok_or_eyre("`chapter_download_dir`字段为`None`")?;

        if chapter_download_dir.exists() {
            std::fs::remove_dir_all(chapter_download_dir)
                .wrap_err(format!("删除 `{}` 失败", chapter_download_dir.display()))?;
        }

        std::fs::rename(temp_download_dir, chapter_download_dir).wrap_err(format!(
            "将 `{}` 重命名为 `{}` 失败",
            temp_download_dir.display(),
            chapter_download_dir.display()
        ))?;

        Ok(())
    }

    #[instrument(level = "error", skip_all)]
    async fn get_urls_with_block_num(&self, chapter_id: i64) -> Option<Vec<(String, u32)>> {
        let jm_client = self.app.get_jm_client();

        let res = tokio::try_join!(
            jm_client.get_scramble_id(chapter_id),
            jm_client.get_chapter(chapter_id)
        );

        let (scramble_id, chapter_resp_data) = match res {
            Ok(data) => data,
            Err(err) => {
                let err_title = "获取图片下载链接失败";
                let message = err.to_message();
                tracing::error!(err_title, message);

                self.set_state(DownloadTaskState::Failed);
                self.emit_download_task_update_event();

                return None;
            }
        };

        let urls_with_block_num: Vec<(String, u32)> = chapter_resp_data
            .images
            .into_iter()
            .filter_map(|filename| {
                let file_path = Path::new(&filename);
                let ext = file_path.extension()?.to_str()?.to_lowercase();
                let url = format!("https://{IMAGE_DOMAIN}/media/photos/{chapter_id}/{filename}");
                if ext == "gif" {
                    return Some((url, 0));
                } else if ext != "webp" {
                    return None;
                }

                let filename_without_ext = file_path.file_stem()?.to_str()?;
                let block_num = calculate_block_num(scramble_id, chapter_id, filename_without_ext);
                Some((url, block_num))
            })
            .collect();

        tracing::trace!("获取图片链接成功");

        Some(urls_with_block_num)
    }

    #[instrument(level = "error", skip_all, fields(temp_download_dir = %temp_download_dir.display()))]
    fn clean_temp_download_dir(&self, temp_download_dir: &Path) {
        let entries = match std::fs::read_dir(temp_download_dir).map_err(eyre::Report::from) {
            Ok(entries) => entries,
            Err(err) => {
                let err_title = "读取临时下载目录失败";
                let message = err.to_message();
                tracing::error!(err_title, message);
                return;
            }
        };

        let download_format = self.app.get_config().read().download_format;
        let extension = download_format.extension();
        for path in entries.filter_map(Result::ok).map(|entry| entry.path()) {
            let should_keep = path
                .extension()
                .and_then(|ext| ext.to_str())
                .is_some_and(|ext| ext == "gif" || ext == extension);
            if should_keep {
                continue;
            }

            if let Err(err) = std::fs::remove_file(&path).map_err(eyre::Report::from) {
                let err_title = "删除临时下载目录中的文件失败";
                let message = err.to_message();
                tracing::error!(err_title, message);
            }
        }

        tracing::trace!("清理临时下载目录成功");
    }

    #[instrument(level = "error", skip_all)]
    async fn acquire_chapter_permit<'a>(&'a self, permit: &mut Option<SemaphorePermit<'a>>) {
        tracing::debug!("章节开始排队");

        self.emit_download_task_update_event();

        *permit = match permit.take() {
            Some(permit) => Some(permit),
            None => match self
                .app
                .get_download_manager()
                .inner()
                .chapter_sem
                .acquire()
                .await
                .map_err(eyre::Report::from)
            {
                Ok(permit) => Some(permit),
                Err(err) => {
                    let err_title = "获取下载章节的permit失败";
                    let message = err.to_message();
                    tracing::error!(err_title, message);

                    self.set_state(DownloadTaskState::Failed);
                    self.emit_download_task_update_event();
                    return;
                }
            },
        };

        if *self.state_sender.borrow() != DownloadTaskState::Pending {
            return;
        }

        if let Err(err) = self
            .state_sender
            .send(DownloadTaskState::Downloading)
            .map_err(eyre::Report::from)
        {
            let err_title = "发送状态`Downloading`失败";
            let message = err.to_message();
            tracing::error!(err_title, message);
            self.set_state(DownloadTaskState::Failed);
        }
    }

    #[instrument(level = "error", skip_all)]
    async fn handle_state_change<'a>(
        &'a self,
        permit: &mut Option<SemaphorePermit<'a>>,
        state_receiver: &mut watch::Receiver<DownloadTaskState>,
    ) {
        self.emit_download_task_update_event();

        let state = *state_receiver.borrow();
        if state == DownloadTaskState::Paused {
            sleep(Duration::from_millis(100)).await;
            tracing::debug!("下载任务已暂停");
            if let Some(permit) = permit.take() {
                drop(permit);
            }
        } else if state == DownloadTaskState::Failed {
            sleep(Duration::from_millis(100)).await;
            if let Some(permit) = permit.take() {
                drop(permit);
            }
        }
    }

    #[instrument(level = "error", skip_all)]
    async fn handle_delete_receiver_change<'a>(&'a self, permit: &mut Option<SemaphorePermit<'a>>) {
        let chapter_id = self.chapter_info.chapter_id;

        let _ = DownloadEvent::TaskDelete { chapter_id }.emit(&self.app);

        if permit.is_some() {
            sleep(Duration::from_millis(100)).await;
        }

        tracing::debug!("下载任务已删除");
    }

    #[instrument(level = "error", skip_all)]
    async fn sleep_between_chapter(&self) {
        let mut remaining_sec = self.app.get_config().read().chapter_download_interval_sec;
        while remaining_sec > 0 {
            let _ = DownloadEvent::Sleeping {
                chapter_id: self.chapter_info.chapter_id,
                remaining_sec,
            }
            .emit(&self.app);
            sleep(Duration::from_secs(1)).await;
            remaining_sec -= 1;
        }
    }

    #[instrument(
        level = "error",
        skip_all,
        fields(
            comic_id = self.comic.id,
            comic_title = self.comic.name,
            chapter_id = self.chapter_info.chapter_id,
            chapter_title = self.chapter_info.chapter_title,
            order = self.chapter_info.order
        )
    )]
    pub fn set_state(&self, state: DownloadTaskState) {
        if let Err(err) = self.state_sender.send(state).map_err(eyre::Report::from) {
            let err_title = format!("发送状态`{state:?}`失败");
            let message = err.to_message();
            tracing::error!(err_title, message);
        }
    }

    pub fn emit_download_task_update_event(&self) {
        let _ = DownloadEvent::TaskUpdate {
            chapter_id: self.chapter_info.chapter_id,
            state: *self.state_sender.borrow(),
            downloaded_img_count: self.downloaded_img_count.load(Ordering::Relaxed),
            total_img_count: self.total_img_count.load(Ordering::Relaxed),
        }
        .emit(&self.app);
    }

    fn emit_download_task_create_event(&self) {
        let _ = DownloadEvent::TaskCreate {
            state: *self.state_sender.borrow(),
            comic: Box::new(self.comic.as_ref().clone()),
            chapter_info: Box::new(self.chapter_info.as_ref().clone()),
            downloaded_img_count: self.downloaded_img_count.load(Ordering::Relaxed),
            total_img_count: self.total_img_count.load(Ordering::Relaxed),
        }
        .emit(&self.app);
    }
}
