use std::{
    io::Write,
    path::Path,
    sync::{
        atomic::{AtomicU32, Ordering},
        Arc,
    },
    time::Duration,
};

use bytes::Bytes;
use eyre::{eyre, WrapErr};
use tauri::AppHandle;
use tauri_specta::Event;
use tokio::{sync::Semaphore, task::JoinSet, time::sleep};
use tracing::instrument;
use zip::{write::SimpleFileOptions, ZipWriter};

use crate::{
    config::ExportSkipMode,
    downloader::download_img_task::{calculate_block_num, decode_and_encode_img},
    events::ExportCbzEvent,
    extensions::AppHandleExt,
    jm_client::IMAGE_DOMAIN,
    types::{ChapterInfo, Comic, ComicInfo, DownloadFormat},
    utils,
};

/// 单张图片的下载重试次数
const IMG_RETRY_TIMES: u32 = 3;

/// 导出失败的兜底事件
struct CbzErrorEventGuard {
    uuid: String,
    app: AppHandle,
    success: bool,
}

impl Drop for CbzErrorEventGuard {
    fn drop(&mut self) {
        if self.success {
            return;
        }
        let uuid = self.uuid.clone();
        let _ = ExportCbzEvent::Error { uuid }.emit(&self.app);
    }
}

/// 上报导出进度：已完成的章节数 + 当前章节的图片数
fn emit_progress(
    app: &AppHandle,
    uuid: &str,
    current: usize,
    img_current: Option<u32>,
    img_total: Option<u32>,
    chapter_title: Option<&str>,
) {
    let _ = ExportCbzEvent::Progress {
        uuid: uuid.to_string(),
        current: u32::try_from(current).unwrap_or(u32::MAX),
        img_current,
        img_total,
        chapter_title: chapter_title.map(str::to_string),
    }
    .emit(app);
}

/// 直接导出 cbz，不把图片下载到下载目录
/// - 输出：导出目录/{漫画名}/cbz/{章节标题}.cbz 和 cover.jpg
/// - 进度通过 ExportCbzEvent 上报，会显示在右侧面板的「导出」里
/// - 每本漫画单独处理，失败不影响其他漫画
#[instrument(level = "error", skip_all, fields(comic_ids = ?comic_ids))]
pub async fn export_cbz_without_download(
    app: AppHandle,
    comic_ids: Vec<i64>,
) -> eyre::Result<()> {
    let (interval_sec, img_concurrency) = {
        let config = app.get_config();
        let config = config.read();
        (
            config.download_all_favorites_interval_sec,
            config.img_concurrency,
        )
    };

    for comic_id in comic_ids {
        let comic = match utils::get_comic(app.clone(), comic_id).await {
            Ok(comic) => comic,
            Err(err) => {
                let err_title = format!("获取ID为`{comic_id}`的漫画失败，已跳过");
                tracing::error!(err_title, message = format!("{err:?}"));
                sleep(Duration::from_secs(interval_sec)).await;
                continue;
            }
        };

        if let Err(err) = export_comic_cbz(&app, &comic, img_concurrency).await {
            let err_title = format!("漫画`{}`导出cbz失败", comic.name);
            tracing::error!(err_title, message = format!("{err:?}"));
        }

        sleep(Duration::from_secs(interval_sec)).await;
    }

    Ok(())
}

#[instrument(
    level = "error",
    skip_all,
    fields(comic_id = comic.id, comic_title = comic.name, chapters = comic.chapter_infos.len())
)]
async fn export_comic_cbz(
    app: &AppHandle,
    comic: &Comic,
    img_concurrency: usize,
) -> eyre::Result<()> {
    let (export_dir, skip_mode, download_format) = {
        let config = app.get_config();
        let config = config.read();
        (
            config.export_dir.clone(),
            config.export_skip_mode,
            config.download_format,
        )
    };

    let comic_export_dir = export_dir.join(utils::filename_filter(&comic.name));
    let cbz_dir = comic_export_dir.join("cbz");
    std::fs::create_dir_all(&cbz_dir).wrap_err(format!("创建目录`{}`失败", cbz_dir.display()))?;

    // 导出目录也写一份元数据，这样「本地库存」可以直接读导出目录
    if let Err(err) = comic.save_metadata_to_dir(&comic_export_dir) {
        let err_title = format!("漫画`{}`写入导出目录元数据失败", comic.name);
        tracing::error!(err_title, message = format!("{err:?}"));
    }
    // 导出目录内容变了，本地库索引（本地标签云等）要重建
    crate::local_index::invalidate();

    let uuid = uuid::Uuid::new_v4().to_string();
    let total = comic.chapter_infos.len();
    let _ = ExportCbzEvent::Start {
        uuid: uuid.clone(),
        comic_title: comic.name.clone(),
        total: u32::try_from(total).unwrap_or(u32::MAX),
    }
    .emit(app);

    let mut error_event_guard = CbzErrorEventGuard {
        uuid: uuid.clone(),
        app: app.clone(),
        success: false,
    };

    // 封面失败不影响章节导出
    if let Err(err) = download_cover(app, comic, &cbz_dir).await {
        let err_title = format!("漫画`{}`下载封面失败", comic.name);
        tracing::error!(err_title, message = format!("{err:?}"));
    }

    let sem = Arc::new(Semaphore::new(img_concurrency.max(1)));

    for (index, chapter_info) in comic.chapter_infos.iter().enumerate() {
        let chapter_title = &chapter_info.chapter_title;
        let cbz_path = cbz_dir.join(format!("{}.cbz", utils::filename_filter(chapter_title)));

        // 跳过策略：SkipExisting 和 SkipExported 都以「文件已存在」为准
        // （直接导出没有下载目录，所以无法记录「曾导出过」的状态）
        let should_skip = match skip_mode {
            ExportSkipMode::None => false,
            _ => cbz_path.exists(),
        };

        if should_skip {
            emit_progress(app, &uuid, index + 1, None, None, Some(chapter_title));
            continue;
        }

        match export_chapter_cbz(
            app,
            comic,
            chapter_info,
            &cbz_path,
            download_format,
            sem.clone(),
            &uuid,
            index,
        )
        .await
        {
            Ok(()) => tracing::info!("章节`{chapter_title}`导出cbz成功"),
            Err(err) => {
                let err_title = format!("章节`{chapter_title}`导出cbz失败");
                tracing::error!(err_title, message = format!("{err:?}"));
                emit_progress(app, &uuid, index + 1, None, None, Some(chapter_title));
            }
        }
    }

    error_event_guard.success = true;

    let _ = ExportCbzEvent::End {
        uuid,
        comic_id: comic.id,
        chapter_export_dir: cbz_dir,
    }
    .emit(app);

    Ok(())
}

#[allow(clippy::too_many_arguments)]
#[instrument(
    level = "error",
    skip_all,
    fields(chapter_id = chapter_info.chapter_id, chapter_title = chapter_info.chapter_title)
)]
async fn export_chapter_cbz(
    app: &AppHandle,
    comic: &Comic,
    chapter_info: &ChapterInfo,
    cbz_path: &Path,
    download_format: DownloadFormat,
    sem: Arc<Semaphore>,
    uuid: &str,
    chapters_done: usize,
) -> eyre::Result<()> {
    let chapter_id = chapter_info.chapter_id;
    let jm_client = app.get_jm_client();

    let (scramble_id, chapter_resp) = tokio::try_join!(
        jm_client.get_scramble_id(chapter_id),
        jm_client.get_chapter(chapter_id)
    )
    .wrap_err("获取章节图片链接失败")?;

    // (原始序号, 图片url, block_num, 扩展名)
    let mut img_tasks: Vec<(usize, String, u32, String)> = Vec::new();
    for (index, filename) in chapter_resp.images.into_iter().enumerate() {
        let Some(file_stem) = Path::new(&filename).file_stem().and_then(|stem| stem.to_str()) else {
            continue;
        };
        let Some(ext) = Path::new(&filename)
            .extension()
            .and_then(|ext| ext.to_str())
            .map(str::to_lowercase)
        else {
            continue;
        };

        let url = format!("https://{IMAGE_DOMAIN}/media/photos/{chapter_id}/{filename}");

        if ext == "gif" {
            img_tasks.push((index, url, 0, "gif".to_string()));
        } else if ext == "webp" {
            let block_num = calculate_block_num(scramble_id, chapter_id, file_stem);
            img_tasks.push((index, url, block_num, download_format.extension().to_string()));
        }
    }

    if img_tasks.is_empty() {
        return Err(eyre!("没有可导出的图片"));
    }

    let img_total = u32::try_from(img_tasks.len()).unwrap_or(u32::MAX);
    let img_done = Arc::new(AtomicU32::new(0));

    // 这一章刚开始，先报一次进度
    emit_progress(
        app,
        uuid,
        chapters_done,
        Some(0),
        Some(img_total),
        Some(&chapter_info.chapter_title),
    );

    let chapter_title = chapter_info.chapter_title.clone();
    let mut join_set = JoinSet::new();
    for (index, url, block_num, ext) in img_tasks {
        let app = app.clone();
        let sem = sem.clone();
        let uuid = uuid.to_string();
        let img_done = img_done.clone();
        let chapter_title = chapter_title.clone();

        join_set.spawn(async move {
            let _permit = sem.acquire_owned().await.map_err(eyre::Report::from)?;

            let mut last_err = None;
            for attempt in 1..=IMG_RETRY_TIMES {
                match download_and_encode_img(&app, &url, block_num, download_format).await {
                    Ok(data) => {
                        let done = img_done.fetch_add(1, Ordering::Relaxed) + 1;
                        emit_progress(
                            &app,
                            &uuid,
                            chapters_done,
                            Some(done),
                            Some(img_total),
                            Some(&chapter_title),
                        );
                        return Ok::<_, eyre::Report>((index, ext, data));
                    }
                    Err(err) => {
                        last_err = Some(err);
                        if attempt < IMG_RETRY_TIMES {
                            sleep(Duration::from_millis(u64::from(attempt) * 500)).await;
                        }
                    }
                }
            }

            Err(last_err.unwrap_or_else(|| eyre!("下载图片`{url}`失败")))
        });
    }

    let mut images: Vec<(usize, String, Vec<u8>)> = Vec::new();
    while let Some(result) = join_set.join_next().await {
        let (index, ext, data) = result.map_err(eyre::Report::from)??;
        images.push((index, ext, data));
    }
    images.sort_by_key(|(index, _, _)| *index);

    // 生成 ComicInfo.xml
    let comic_info = ComicInfo::from(comic, chapter_info);
    let cfg = yaserde::ser::Config {
        perform_indent: true,
        ..Default::default()
    };
    let comic_info_xml = yaserde::ser::to_string_with_config(&comic_info, &cfg)
        .map_err(|err_msg| eyre!("序列化`ComicInfo.xml`失败: {err_msg}"))?;

    let tmp_path = cbz_path.with_extension("cbz.tmp");
    let tmp_path_for_write = tmp_path.clone();
    let cbz_path_for_write = cbz_path.to_path_buf();
    let write_result = tokio::task::spawn_blocking(move || {
        write_cbz(
            &tmp_path_for_write,
            &cbz_path_for_write,
            &comic_info_xml,
            &images,
        )
    })
    .await
    .map_err(eyre::Report::from)?;

    if write_result.is_err() {
        let _ = std::fs::remove_file(&tmp_path);
    }
    write_result?;

    // 这一章完成
    emit_progress(
        app,
        uuid,
        chapters_done + 1,
        Some(img_total),
        Some(img_total),
        Some(&chapter_info.chapter_title),
    );

    Ok(())
}

async fn download_and_encode_img(
    app: &AppHandle,
    url: &str,
    block_num: u32,
    download_format: DownloadFormat,
) -> eyre::Result<Vec<u8>> {
    let (img_data, src_format) = app.get_jm_client().get_img_data_and_format(url).await?;
    let img_data: Bytes = img_data;

    tokio::task::spawn_blocking(move || {
        decode_and_encode_img(&img_data, src_format, block_num, download_format)
    })
    .await
    .map_err(eyre::Report::from)?
}

async fn download_cover(app: &AppHandle, comic: &Comic, cbz_dir: &Path) -> eyre::Result<()> {
    let url = format!("https://cdn-msp3.18comic.vip/media/albums/{}.jpg", comic.id);
    let (img_data, _format) = app
        .get_jm_client()
        .get_img_data_and_format(&url)
        .await
        .wrap_err(format!("下载封面`{url}`失败"))?;

    std::fs::write(cbz_dir.join("cover.jpg"), img_data).wrap_err("保存封面失败")?;

    Ok(())
}

/// 先写临时文件，成功后再替换，避免留下半成品
fn write_cbz(
    tmp_path: &Path,
    cbz_path: &Path,
    comic_info_xml: &str,
    images: &[(usize, String, Vec<u8>)],
) -> eyre::Result<()> {
    let file = std::fs::File::create(tmp_path)
        .wrap_err(format!("创建文件`{}`失败", tmp_path.display()))?;
    let mut zip_writer = ZipWriter::new(file);
    // 图片本身已经压缩过，用 Stored 最快（zip 的默认值在启用 deflate 特性后会变成 Deflated）
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);

    zip_writer
        .start_file("ComicInfo.xml", options)
        .wrap_err("在cbz中创建`ComicInfo.xml`失败")?;
    zip_writer
        .write_all(comic_info_xml.as_bytes())
        .wrap_err("写入`ComicInfo.xml`失败")?;

    for (index, ext, data) in images {
        let name = format!("{:04}.{ext}", index + 1);
        zip_writer
            .start_file(name, options)
            .wrap_err("在cbz中创建图片条目失败")?;
        zip_writer.write_all(data).wrap_err("写入图片失败")?;
    }

    zip_writer
        .finish()
        .wrap_err(format!("关闭`{}`失败", tmp_path.display()))?;

    if cbz_path.exists() {
        std::fs::remove_file(cbz_path)
            .wrap_err(format!("删除旧的cbz`{}`失败", cbz_path.display()))?;
    }
    std::fs::rename(tmp_path, cbz_path).wrap_err(format!(
        "将`{}`重命名为`{}`失败",
        tmp_path.display(),
        cbz_path.display()
    ))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use super::*;

    /// 验证 cbz 内的结构：ComicInfo.xml + 按序号命名的图片
    #[test]
    fn write_cbz_should_write_comic_info_and_sorted_images() {
        let dir = std::env::temp_dir().join(format!(
            "jmcomic-shelf-cbz-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();

        let tmp_path = dir.join("第1话.cbz.tmp");
        let cbz_path = dir.join("第1话.cbz");
        let images = vec![
            (0usize, "jpg".to_string(), b"first".to_vec()),
            (1usize, "jpg".to_string(), b"second".to_vec()),
        ];

        write_cbz(&tmp_path, &cbz_path, "<ComicInfo />", &images).unwrap();
        assert!(!tmp_path.exists());
        assert!(cbz_path.exists());

        let file = std::fs::File::open(&cbz_path).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();
        let mut names = (0..archive.len())
            .map(|i| archive.by_index(i).unwrap().name().to_string())
            .collect::<Vec<String>>();
        names.sort();
        assert_eq!(
            names,
            vec![
                "0001.jpg".to_string(),
                "0002.jpg".to_string(),
                "ComicInfo.xml".to_string(),
            ]
        );

        let mut content = String::new();
        archive
            .by_name("0002.jpg")
            .unwrap()
            .read_to_string(&mut content)
            .unwrap();
        assert_eq!(content, "second");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
