use std::{
    io::Cursor,
    ops::ControlFlow,
    path::{Path, PathBuf},
    sync::{atomic::Ordering, Arc},
    time::Duration,
};

use bytes::Bytes;
use eyre::WrapErr;
use image::codecs::png;
use image::codecs::png::PngEncoder;
use image::{ImageFormat, RgbImage};
use tauri::AppHandle;
use tokio::{
    sync::{watch, SemaphorePermit},
    time::sleep,
};
use tracing::instrument;

use crate::{
    downloader::{download_task::DownloadTask, download_task_state::DownloadTaskState},
    extensions::{AppHandleExt, EyreReportToMessage},
    types::DownloadFormat,
    utils,
};

pub struct DownloadImgTask {
    app: AppHandle,
    download_task: Arc<DownloadTask>,
    url: String,
    index: usize,
    temp_download_dir: PathBuf,
    block_num: u32,
}

impl DownloadImgTask {
    pub fn new(
        download_task: Arc<DownloadTask>,
        url: String,
        index: usize,
        temp_download_dir: PathBuf,
        block_num: u32,
    ) -> Self {
        Self {
            app: download_task.app.clone(),
            download_task,
            url,
            index,
            temp_download_dir,
            block_num,
        }
    }

    #[instrument(
        level = "error",
        skip_all,
        fields(
            index = self.index,
            url = self.url,
            comic_id = self.download_task.comic.id,
            comic_title = self.download_task.comic.name,
            chapter_id = self.download_task.chapter_info.chapter_id,
            chapter_title = self.download_task.chapter_info.chapter_title
        )
    )]
    pub async fn process(self) {
        let download_img_task = self.download_img();
        tokio::pin!(download_img_task);

        let mut state_receiver = self.download_task.state_sender.subscribe();
        state_receiver.mark_changed();

        let mut delete_receiver = self.download_task.delete_sender.subscribe();

        let mut permit = None;

        loop {
            let state_is_downloading = *state_receiver.borrow() == DownloadTaskState::Downloading;
            tokio::select! {
                () = &mut download_img_task, if state_is_downloading && permit.is_some() => break,

                control_flow = self.acquire_img_permit(&mut permit), if state_is_downloading && permit.is_none() => {
                    match control_flow {
                        ControlFlow::Continue(()) => {}
                        ControlFlow::Break(()) => break,
                    }
                }

                _ = state_receiver.changed() => {
                    match self.handle_state_change(&mut permit, &mut state_receiver).await {
                        ControlFlow::Continue(()) => {}
                        ControlFlow::Break(()) => break,
                    }
                }

                _ = delete_receiver.changed() => {
                    self.handle_delete_receiver_change(&mut permit).await;
                    return;
                }
            }
        }
    }

    #[instrument(level = "error", skip_all)]
    async fn download_img(&self) {
        let url = &self.url;

        let index_filename = format!("{:04}", self.index + 1);
        let download_format = self.app.get_config().read().download_format;
        let extension = download_format.extension();

        let user_format_path = self
            .temp_download_dir
            .join(format!("{index_filename}.{extension}"));
        let gif_path = self.temp_download_dir.join(format!("{index_filename}.gif"));

        if user_format_path.exists() || gif_path.exists() {
            self.download_task
                .downloaded_img_count
                .fetch_add(1, Ordering::Relaxed);

            self.download_task.emit_download_task_update_event();

            tracing::trace!("图片已存在，跳过下载");
            return;
        }

        tracing::trace!("开始下载图片");

        // 普通下载失败即放弃：只取一次，不重试
        let (img_data, format) = match fetch_img_data(&self.app, url, 1, Duration::ZERO).await {
            Ok(data) => data,
            Err(err) => {
                let err_title = "下载图片失败";
                let message = err.to_message();
                tracing::error!(err_title, message);
                return;
            }
        };
        let img_data_len = img_data.len() as u64;

        tracing::trace!("图片成功下载到内存");

        let save_path = if format == ImageFormat::Gif {
            gif_path
        } else {
            user_format_path
        };

        if let Err(err) = save_img(
            &save_path,
            download_format,
            self.block_num,
            img_data,
            format,
        )
        .await
        {
            let err_title = "保存图片失败";
            let message = err.to_message();
            tracing::error!(err_title, message);
            return;
        }

        tracing::trace!("图片成功保存到磁盘");

        self.app
            .get_download_manager()
            .byte_per_sec
            .fetch_add(img_data_len, Ordering::Relaxed);

        self.download_task
            .downloaded_img_count
            .fetch_add(1, Ordering::Relaxed);

        self.download_task.emit_download_task_update_event();

        let img_download_interval_sec = self.app.get_config().read().img_download_interval_sec;
        sleep(Duration::from_secs(img_download_interval_sec)).await;
    }

    #[instrument(level = "error", skip_all)]
    async fn acquire_img_permit<'a>(
        &'a self,
        permit: &mut Option<SemaphorePermit<'a>>,
    ) -> ControlFlow<()> {
        tracing::trace!("图片开始排队");

        *permit = match permit.take() {
            Some(permit) => Some(permit),
            None => match self
                .app
                .get_download_manager()
                .inner()
                .img_sem
                .acquire()
                .await
                .map_err(eyre::Report::from)
            {
                Ok(permit) => Some(permit),
                Err(err) => {
                    let err_title = "获取下载图片的permit失败";
                    let message = err.to_message();
                    tracing::error!(err_title, message);
                    return ControlFlow::Break(());
                }
            },
        };

        ControlFlow::Continue(())
    }

    #[instrument(level = "error", skip_all)]
    async fn handle_state_change<'a>(
        &'a self,
        permit: &mut Option<SemaphorePermit<'a>>,
        state_receiver: &mut watch::Receiver<DownloadTaskState>,
    ) -> ControlFlow<()> {
        let state = *state_receiver.borrow();
        if state == DownloadTaskState::Paused {
            sleep(Duration::from_millis(100)).await;
            tracing::trace!("图片暂停下载");
            if let Some(permit) = permit.take() {
                drop(permit);
            }
        } else if state == DownloadTaskState::Failed {
            sleep(Duration::from_millis(100)).await;
            tracing::trace!("图片取消下载");
            if let Some(permit) = permit.take() {
                drop(permit);
            }
        }

        ControlFlow::Continue(())
    }

    #[instrument(level = "error", skip_all)]
    async fn handle_delete_receiver_change<'a>(&'a self, permit: &mut Option<SemaphorePermit<'a>>) {
        if permit.is_some() {
            sleep(Duration::from_millis(100)).await;
        }

        tracing::trace!("图片取消下载");
    }
}

pub fn calculate_block_num(scramble_id: i64, id: i64, filename: &str) -> u32 {
    if id < scramble_id {
        0
    } else if id < 268_850 {
        10
    } else {
        let x = if id < 421_926 { 10 } else { 8 };
        let s = format!("{id}{filename}");
        let s = utils::md5_hex(&s);
        let mut block_num = s.chars().last().unwrap() as u32;
        block_num %= x;
        block_num = block_num * 2 + 2;
        block_num
    }
}

/// 取一张图片的原始数据：失败按 `attempt * retry_interval` 退避后重试，最多尝试 `retry_times` 次
/// - 重试次数与退避间隔由调用方决定，两条下载路径的策略不一样
pub async fn fetch_img_data(
    app: &AppHandle,
    url: &str,
    retry_times: u32,
    retry_interval: Duration,
) -> eyre::Result<(Bytes, ImageFormat)> {
    fetch_img_with_retry(app, url, retry_times, retry_interval, |img_data, format| {
        async move { Ok((img_data, format)) }
    })
    .await
}

/// 取一张图片并交给 `process` 处理：下载失败或处理失败都会按 `attempt * retry_interval`
/// 退避后重试，最多尝试 `retry_times` 次
/// - 重试次数与退避间隔由调用方决定：导出路径连解码失败一起重试，普通下载只取一次
pub async fn fetch_img_with_retry<T, F, Fut>(
    app: &AppHandle,
    url: &str,
    retry_times: u32,
    retry_interval: Duration,
    mut process: F,
) -> eyre::Result<T>
where
    F: FnMut(Bytes, ImageFormat) -> Fut,
    Fut: std::future::Future<Output = eyre::Result<T>>,
{
    let mut last_err = None;
    for attempt in 1..=retry_times {
        let result = match app.get_jm_client().get_img_data_and_format(url).await {
            Ok((img_data, format)) => process(img_data, format).await,
            Err(err) => Err(err),
        };
        match result {
            Ok(value) => return Ok(value),
            Err(err) => {
                last_err = Some(err);
                if attempt < retry_times {
                    sleep(retry_interval * attempt).await;
                }
            }
        }
    }

    Err(last_err.unwrap_or_else(|| eyre::eyre!("下载图片`{url}`失败")))
}

#[instrument(
    level = "error",
    skip_all,
    fields(
        save_path = %save_path.display(),
        download_format = ?download_format,
        block_num = block_num,
        src_format = ?src_format
    )
)]
async fn save_img(
    save_path: &Path,
    download_format: DownloadFormat,
    block_num: u32,
    src_img_data: Bytes,
    src_format: ImageFormat,
) -> eyre::Result<()> {
    let save_path = save_path.to_path_buf();
    let current_span = tracing::Span::current();
    let process_img = move || -> eyre::Result<()> {
        let _enter = current_span.enter();
        let dst_img_data =
            decode_and_encode_img(&src_img_data, src_format, block_num, download_format)?;
        std::fs::write(&save_path, dst_img_data)
            .wrap_err(format!("保存图片`{}`失败", save_path.display()))?;
        Ok(())
    };

    let (sender, receiver) = tokio::sync::oneshot::channel::<eyre::Result<()>>();
    rayon::spawn(move || {
        let _ = sender.send(process_img());
    });

    let result = receiver.await?;
    tracing::trace!("图片成功保存到磁盘");
    result
}

/// 把服务端返回的图片解码 → 还原拼图 → 按目标格式编码，返回图片字节
/// - gif 直接原样返回（不做解码和格式转换）
/// - 这个过程是 CPU 密集的，调用方应该放在 rayon 线程池里执行
pub fn decode_and_encode_img(
    src_img_data: &Bytes,
    src_format: ImageFormat,
    block_num: u32,
    download_format: DownloadFormat,
) -> eyre::Result<Vec<u8>> {
    if src_format == ImageFormat::Gif {
        return Ok(src_img_data.to_vec());
    }

    let mut src_img = image::load_from_memory(src_img_data)
        .wrap_err("解码图片失败")?
        .to_rgb8();

    let dst_img = if block_num == 0 {
        src_img
    } else {
        stitch_img(&mut src_img, block_num)
    };

    let mut dst_img_data = Vec::new();
    match download_format {
        DownloadFormat::Jpeg => {
            dst_img.write_to(&mut Cursor::new(&mut dst_img_data), ImageFormat::Jpeg)?;
        }
        DownloadFormat::Png => {
            let encoder = PngEncoder::new_with_quality(
                Cursor::new(&mut dst_img_data),
                png::CompressionType::Best,
                png::FilterType::default(),
            );
            dst_img.write_with_encoder(encoder)?;
        }
        DownloadFormat::Webp => {
            dst_img.write_to(&mut Cursor::new(&mut dst_img_data), ImageFormat::WebP)?;
        }
    }

    Ok(dst_img_data)
}

fn stitch_img(src_img: &mut RgbImage, block_num: u32) -> RgbImage {
    let (width, height) = src_img.dimensions();
    let mut stitched_img = image::ImageBuffer::new(width, height);
    let remainder_height = height % block_num;
    for i in 0..block_num {
        let mut block_height = height / block_num;
        let src_img_y_start = height - (block_height * (i + 1)) - remainder_height;
        let mut dst_img_y_start = block_height * i;
        if i == 0 {
            block_height += remainder_height;
        } else {
            dst_img_y_start += remainder_height;
        }

        for y in 0..block_height {
            let src_y = src_img_y_start + y;
            let dst_y = dst_img_y_start + y;
            for x in 0..width {
                stitched_img.put_pixel(x, dst_y, *src_img.get_pixel(x, src_y));
            }
        }
    }

    stitched_img
}
