use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use eyre::{eyre, WrapErr};
use indexmap::IndexMap;
use tauri::AppHandle;
use tauri_specta::Event;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tokio::time::sleep;
use tracing::{instrument, Instrument};

use crate::config::LocalLibrarySource;
use crate::errors::{CommandError, CommandResult};
use crate::events::{DownloadAllFavoritesEvent, UpdateDownloadedComicsEvent};
use crate::export;
use crate::extensions::{AppHandleExt, EyreReportToMessage};
use crate::local_index;
use crate::types::{ChapterInfo, Comic, FavoriteSort};
use crate::utils;
/// 标签云：下载目录 + 导出目录共用的一份标签统计
#[tauri::command]
#[specta::specta]
#[instrument(level = "error", skip_all)]
pub fn get_local_tags_all(app: AppHandle) -> Vec<local_index::LocalTag> {
    local_index::all_local_tags(&app)
}

#[allow(clippy::needless_pass_by_value)]
#[tauri::command(async)]
#[specta::specta]
#[instrument(
    level = "error",
    skip_all,
    fields(comic_id = comic.id, comic_title = comic.name, chapter_id = chapter_id)
)]
pub fn create_download_task(app: AppHandle, comic: Comic, chapter_id: i64) -> CommandResult<()> {
    let download_manager = app.get_download_manager();

    download_manager
        .create_download_task(comic, chapter_id)
        .map_err(|err| CommandError::from("创建下载任务失败", err))?;
    Ok(())
}

#[allow(clippy::needless_pass_by_value)]
#[tauri::command(async)]
#[specta::specta]
#[instrument(level = "error", skip_all, fields(comic_id = comic.id, comic_title = comic.name))]
pub fn create_download_tasks(app: AppHandle, comic: Comic, chapter_ids: Vec<i64>) {
    let download_manager = app.get_download_manager();

    download_manager.create_download_tasks(comic, &chapter_ids);
}

#[allow(clippy::needless_pass_by_value)]
#[tauri::command(async)]
#[specta::specta]
#[instrument(level = "error", skip_all, fields(chapter_id = chapter_id))]
pub fn pause_download_task(app: AppHandle, chapter_id: i64) -> CommandResult<()> {
    let download_manager = app.get_download_manager();

    download_manager
        .pause_download_task(chapter_id)
        .map_err(|err| CommandError::from("暂停下载任务失败", err))?;
    Ok(())
}

#[allow(clippy::needless_pass_by_value)]
#[tauri::command(async)]
#[specta::specta]
#[instrument(level = "error", skip_all, fields(chapter_id = chapter_id))]
pub fn resume_download_task(app: AppHandle, chapter_id: i64) -> CommandResult<()> {
    let download_manager = app.get_download_manager();

    download_manager
        .resume_download_task(chapter_id)
        .map_err(|err| CommandError::from("恢复下载任务失败", err))?;
    Ok(())
}

#[allow(clippy::needless_pass_by_value)]
#[tauri::command(async)]
#[specta::specta]
#[instrument(level = "error", skip_all, fields(chapter_id = chapter_id))]
pub fn delete_download_task(
    app: AppHandle,
    chapter_id: i64,
    delete_files: bool,
) -> CommandResult<()> {
    let download_manager = app.get_download_manager();

    download_manager
        .delete_download_task(chapter_id, delete_files)
        .map_err(|err| CommandError::from("删除下载任务失败", err))?;
    Ok(())
}

/// 前端挂载后同步一次下载任务，把恢复出来的任务补进进度列表
#[tauri::command(async)]
#[specta::specta]
pub fn sync_download_tasks(app: AppHandle) {
    app.get_download_manager().sync_tasks();
}

#[tauri::command(async)]
#[specta::specta]
#[instrument(level = "error", skip_all, fields(aid = aid))]
pub async fn download_comic(app: AppHandle, aid: i64) -> CommandResult<()> {
    let download_manager = app.get_download_manager();

    let comic = utils::get_comic(app.clone(), aid)
        .await
        .map_err(|err| CommandError::from("获取漫画信息失败", err))?;

    let comic_title = &comic.name;

    let chapter_ids: Vec<i64> = comic
        .chapter_infos
        .iter()
        .filter(|chapter_info| chapter_info.is_downloaded != Some(true))
        .map(|chapter_info| chapter_info.chapter_id)
        .collect();

    if chapter_ids.is_empty() {
        let err = eyre!("漫画`{comic_title}`的所有章节都已存在于下载目录，无需重复下载");
        return Err(CommandError::from("所有章节都已下载", err));
    }

    for chapter_id in &chapter_ids {
        download_manager
            .create_download_task(comic.clone(), *chapter_id)
            .map_err(|err| CommandError::from("一键下载漫画失败", err))?;
    }

    tracing::debug!("一键下载漫画成功，已为所有需要下载的章节创建下载任务");
    Ok(())
}

#[allow(clippy::cast_possible_wrap)]
#[tauri::command(async)]
#[specta::specta]
#[instrument(level = "error", skip_all)]
pub async fn download_all_favorites(app: AppHandle) -> CommandResult<()> {
    let config = app.get_config();
    let jm_client = app.get_jm_client().inner().clone();
    let download_manager = app.get_download_manager();

    let mut favorite_comics = Vec::new();
    // 发送正在获取收藏夹事件
    let _ = DownloadAllFavoritesEvent::GetFavoritesStart.emit(&app);
    // 获取收藏夹第一页
    let first_page = jm_client
        .get_favorite_folder(0, 1, FavoriteSort::FavoriteTime)
        .await
        .map_err(|err| CommandError::from("获取收藏夹失败", err))?;
    favorite_comics.extend(first_page.list);
    // 计算总页数
    let count = first_page.count;
    let total = first_page
        .total
        .parse::<i64>()
        .map_err(|err| CommandError::from("获取收藏夹失败", err))?;
    let page_count = utils::page_count(total, count);
    // 获取收藏夹剩余页
    let sem = Arc::new(Semaphore::new(5));
    let mut join_set = JoinSet::new();
    for page in 2..=page_count {
        let jm_client = jm_client.clone();
        let sem = sem.clone();
        let get_favorite_task = async move {
            let _permit = sem.acquire().await?;
            let page = jm_client
                .get_favorite_folder(0, page, FavoriteSort::FavoriteTime)
                .await?;
            Ok::<_, eyre::Report>(page)
        };
        join_set.spawn(get_favorite_task.in_current_span());
    }
    // 等待所有请求完成
    while let Some(Ok(get_favorite_result)) = join_set.join_next().await {
        // 如果有请求失败，直接返回错误
        let page = get_favorite_result.map_err(|err| CommandError::from("获取收藏夹失败", err))?;
        favorite_comics.extend(page.list);
    }
    // 至此，收藏夹已经全部获取完毕
    let total = favorite_comics.len() as i64;

    let interval_sec = config.read().download_all_favorites_interval_sec;
    for (i, favorite_comic) in favorite_comics.into_iter().enumerate() {
        let comic_title = &favorite_comic.name;
        let comic_id = match favorite_comic
            .id
            .parse::<i64>()
            .wrap_err("将id解析为i64失败")
        {
            Ok(id) => id,
            Err(err) => {
                let err_title = format!("下载收藏夹过程中，获取漫画`{comic_title}`失败，已跳过");
                let message = err.to_message();
                tracing::error!(err_title, message);
                sleep(Duration::from_secs(interval_sec)).await;
                continue;
            }
        };

        let comic = match utils::get_comic(app.clone(), comic_id).await {
            Ok(comic) => comic,
            Err(err) => {
                let err_title = format!("下载收藏夹过程中，获取漫画`{comic_title}`失败，已跳过");
                let err = err.wrap_err("可能是频率太高，请手动去`配置`里调整`下载整个收藏夹时，每处理完一个收藏夹中的漫画后休息`");
                let message = err.to_message();
                tracing::error!(err_title, message);
                sleep(Duration::from_secs(interval_sec)).await;
                continue;
            }
        };

        let current = (i + 1) as i64;
        let _ = DownloadAllFavoritesEvent::GetComicsProgress { current, total }.emit(&app);

        // 给每个漫画未下载的章节创建下载任务
        let chapter_infos: Vec<&ChapterInfo> = comic
            .chapter_infos
            .iter()
            .filter(|chapter_info| chapter_info.is_downloaded != Some(true))
            .collect();

        if chapter_infos.is_empty() {
            sleep(Duration::from_secs(interval_sec)).await;
            continue;
        }

        let _ = DownloadAllFavoritesEvent::StartCreateDownloadTasks {
            comic_id: comic.id,
            comic_title: comic.name.clone(),
            current: 0,
            total: chapter_infos.len() as i64,
        }
        .emit(&app);

        for (current, chapter_info) in chapter_infos.into_iter().enumerate() {
            let current = current as i64 + 1;
            let _ = download_manager.create_download_task(comic.clone(), chapter_info.chapter_id);

            let _ = DownloadAllFavoritesEvent::CreatingDownloadTask {
                comic_id: comic.id,
                current,
            }
            .emit(&app);

            sleep(Duration::from_millis(100)).await;
        }

        let _ = DownloadAllFavoritesEvent::EndCreateDownloadTasks { comic_id: comic.id }.emit(&app);

        sleep(Duration::from_secs(interval_sec)).await;
    }
    // 至此，所有收藏夹漫画的下载任务已经全部创建完毕
    let _ = DownloadAllFavoritesEvent::GetComicsEnd.emit(&app);

    Ok(())
}

#[allow(clippy::cast_possible_wrap)]
#[tauri::command(async)]
#[specta::specta]
#[instrument(level = "error", skip_all)]
pub async fn update_downloaded_comics(app: AppHandle) -> CommandResult<()> {
    let config = app.get_config();
    let download_manager = app.get_download_manager();

    // 从下载目录中获取已下载的漫画
    let downloaded_comics = get_downloaded_comics(app.clone());

    let total = downloaded_comics.len() as i64;
    let interval_sec = config.read().update_downloaded_comics_interval_sec;
    let _ = UpdateDownloadedComicsEvent::GetComicStart { total }.emit(&app);

    for (i, downloaded_comic) in downloaded_comics.into_iter().enumerate() {
        let comic_title = &downloaded_comic.name;
        let comic_id = downloaded_comic.id;
        let current = (i + 1) as i64;
        let _ = UpdateDownloadedComicsEvent::GetComicProgress { current, total }.emit(&app);

        let comic = match utils::get_comic(app.clone(), comic_id)
            .await
            .wrap_err(format!("获取ID为`{comic_id}`的漫画失败"))
        {
            Ok(comic) => comic,
            Err(err) => {
                let err_title = format!("更新库存过程中，获取漫画`{comic_title}`失败，已跳过");
                let err = err.wrap_err("可能是频率太高，请手动去`配置`里调整`更新库存时，每处理完一个已下载的漫画后休息`");
                let message = err.to_message();
                tracing::error!(err_title, message);
                sleep(Duration::from_secs(interval_sec)).await;
                continue;
            }
        };

        // 至少有一个章节已下载
        let has_downloaded_chapter = comic
            .chapter_infos
            .iter()
            .any(|chapter_info| chapter_info.is_downloaded == Some(true));

        if !has_downloaded_chapter {
            sleep(Duration::from_secs(interval_sec)).await;
            continue;
        }

        let chapter_infos: Vec<&ChapterInfo> = comic
            .chapter_infos
            .iter()
            .filter(|chapter| chapter.is_downloaded != Some(true))
            .collect();

        if chapter_infos.is_empty() {
            sleep(Duration::from_secs(interval_sec)).await;
            continue;
        }

        let _ = UpdateDownloadedComicsEvent::CreateDownloadTasksStart {
            comic_id: comic.id,
            comic_title: comic.name.clone(),
            current: 0,
            total: chapter_infos.len() as i64,
        }
        .emit(&app);

        for (i, chapter_info) in chapter_infos.into_iter().enumerate() {
            let chapter_id = chapter_info.chapter_id;
            let current = (i + 1) as i64;

            let _ = download_manager.create_download_task(comic.clone(), chapter_id);

            let _ = UpdateDownloadedComicsEvent::CreateDownloadTaskProgress {
                comic_id: comic.id,
                current,
            }
            .emit(&app);

            sleep(Duration::from_millis(100)).await;
        }

        let _ =
            UpdateDownloadedComicsEvent::CreateDownloadTasksEnd { comic_id: comic.id }.emit(&app);

        sleep(Duration::from_secs(interval_sec)).await;
    }

    let _ = UpdateDownloadedComicsEvent::GetComicEnd.emit(&app);

    Ok(())
}

/// 读取本地库存
/// - `source` 由前端显式传入，而不是从后端配置里读：切换「下载目录/导出目录」时，
///   前端会同时发起 `save_config` 和这个命令，如果在这里读配置就会和写配置竞争，
///   经常读到旧的来源，表现为「点了没切换，再点一次才行」
#[allow(clippy::needless_pass_by_value)]
#[tauri::command(async)]
#[specta::specta]
#[instrument(level = "error", skip_all)]
pub fn get_local_comics(app: AppHandle, source: LocalLibrarySource) -> Vec<Comic> {
    match source {
        LocalLibrarySource::DownloadDir => get_downloaded_comics(app),
        LocalLibrarySource::ExportDir => get_exported_comics(app),
    }
}

/// 读取导出目录里的漫画（只导出过 cbz、没有下载过的漫画也能显示）
/// - 以 `元数据.json` 为准，和搜索/收藏页的「已导出」标记、空间统计用同一套判定
#[allow(clippy::needless_pass_by_value)]
#[instrument(level = "error", skip_all)]
pub fn get_exported_comics(app: AppHandle) -> Vec<Comic> {
    let mut comic_dirs_with_modify_time = local_index::export_comics(&app)
        .into_iter()
        .map(|(_, _, comic_export_dir)| {
            let modify_time = comic_export_dir
                .metadata()
                .and_then(|metadata| metadata.modified())
                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
            (comic_export_dir, modify_time)
        })
        .collect::<Vec<_>>();
    // 按照修改时间排序，最新的排在最前面
    comic_dirs_with_modify_time.sort_by(|(_, a), (_, b)| b.cmp(a));

    let mut comics = Vec::new();
    for (comic_export_dir, _) in comic_dirs_with_modify_time {
        let metadata_path = comic_export_dir.join("元数据.json");
        if !metadata_path.exists() {
            tracing::warn!(
                "导出目录`{}`里没有元数据，已跳过（重新导出一次即可生成）",
                comic_export_dir.display()
            );
            continue;
        }

        let mut comic = match Comic::from_metadata(&metadata_path) {
            Ok(comic) => comic,
            Err(err) => {
                let err_title = "获取导出目录中的漫画时遇到错误，已跳过";
                let message = err.to_message();
                tracing::error!(err_title, message);
                continue;
            }
        };

        mark_exported_chapters(&mut comic, &comic_export_dir, false);

        // 指向导出目录，方便「打开目录」按钮；同时标记为未下载（只是导出过）
        comic.comic_download_dir = Some(comic_export_dir);
        comic.is_downloaded = Some(false);
        comics.push(comic);
    }

    // 按照漫画ID分组去重
    let mut comics_by_id: IndexMap<i64, Comic> = IndexMap::new();
    for comic in comics {
        comics_by_id.entry(comic.id).or_insert(comic);
    }

    comics_by_id.into_values().collect()
}



/// 根据导出目录里**真实存在**的 pdf / cbz 文件，重算每个章节的导出状态
/// - 元数据里的标记可能是旧的（导出之后又把文件删了，标记不会自己消失）
/// - reset=true：以文件为准（本地下载的漫画能拿到章节下载目录名，匹配可靠）
/// - reset=false：只用文件**补上**标记、不清除（导出目录里的漫画拿不到下载目录名，
///   文件名可能对不上，清掉会误伤）
#[instrument(level = "error", skip_all, fields(comic_id = comic.id, comic_title = comic.name))]
fn mark_exported_chapters(comic: &mut Comic, comic_export_dir: &Path, reset: bool) {
    let file_names = |sub_dir: &str, extension: &str| -> Vec<String> {
        let Ok(read_dir) = std::fs::read_dir(comic_export_dir.join(sub_dir)) else {
            return Vec::new();
        };
        read_dir
            .filter_map(Result::ok)
            .map(|entry| entry.file_name().to_string_lossy().to_string())
            .filter(|name| name.to_lowercase().ends_with(extension))
            .collect()
    };

    let pdf_files = file_names(export::ExportFormat::Pdf.extension(), ".pdf");
    let cbz_files = file_names(export::ExportFormat::Cbz.extension(), ".cbz");

    for chapter_info in &mut comic.chapter_infos {
        let chapter_title = utils::filename_filter(&chapter_info.chapter_title);
        let mut pdf_names = vec![format!("{chapter_title}.pdf")];
        let mut cbz_names = vec![format!("{chapter_title}.cbz")];
        // 本地导出的文件名用的是「章节下载目录名」，和章节标题不一定完全一致
        if let Some(dir_name) = chapter_info
            .chapter_download_dir
            .as_ref()
            .and_then(|dir| dir.file_name())
        {
            let dir_name = dir_name.to_string_lossy();
            pdf_names.push(format!("{dir_name}.pdf"));
            cbz_names.push(format!("{dir_name}.cbz"));
        }

        let pdf_exists = pdf_names.iter().any(|name| pdf_files.contains(name));
        let cbz_exists = cbz_names.iter().any(|name| cbz_files.contains(name));

        if pdf_exists || reset {
            chapter_info.is_pdf_exported = pdf_exists;
        }
        if cbz_exists || reset {
            chapter_info.is_cbz_exported = cbz_exists;
        }
    }
}

#[allow(clippy::needless_pass_by_value)]
#[allow(clippy::too_many_lines)]
#[tauri::command(async)]
#[specta::specta]
#[instrument(level = "error", skip_all)]
pub fn get_downloaded_comics(app: AppHandle) -> Vec<Comic> {
    // 元数据文件列表（路径 + 修改时间，已按修改时间倒序）直接从 local_index 的快照里取，
    // 省掉一次全量 WalkDir
    let metadata_path_with_modify_time = local_index::download_metadata_files(&app);

    let mut downloaded_comics = Vec::new();
    for (metadata_path, _) in metadata_path_with_modify_time {
        match Comic::from_metadata(&metadata_path) {
            Ok(comic) => downloaded_comics.push(comic),
            Err(err) => {
                let err_title = "获取已下载漫画的过程中遇到错误，已跳过";
                let message = err.to_message();
                tracing::error!(err_title, message);
            }
        }
    }
    // 按照漫画ID分组，以方便去重
    let mut comics_by_id: IndexMap<i64, Vec<Comic>> = IndexMap::new();
    for comic in downloaded_comics {
        comics_by_id.entry(comic.id).or_default().push(comic);
    }

    let mut unique_comics = Vec::new();
    for (_comic_id, mut comics) in comics_by_id {
        // 该漫画ID对应的所有漫画下载目录，可能有多个版本，所以需要去重
        let comic_download_dirs: Vec<&PathBuf> = comics
            .iter()
            .filter_map(|comic| comic.comic_download_dir.as_ref())
            .collect();

        if comic_download_dirs.is_empty() {
            // 其实这种情况不应该发生，因为漫画元数据文件应该总是有下载目录的
            continue;
        }

        // 选第一个作为保留的漫画
        let chosen_download_dir = comic_download_dirs[0];

        if comics.len() > 1 {
            let dir_paths_string = comic_download_dirs
                .iter()
                .map(|path| format!("`{}`", path.display()))
                .collect::<Vec<String>>()
                .join(", ");
            // 如果有重复的漫画，打印错误信息
            let comic_title = &comics[0].name;
            let err_title = "获取已下载漫画的过程中遇到错误";
            let message = eyre!("所有版本路径: [{dir_paths_string}]")
                .wrap_err(format!(
                    "此次获取已下载漫画的结果中只保留版本`{}`",
                    chosen_download_dir.display()
                ))
                .wrap_err(format!(
                    "漫画`{comic_title}`在下载目录里有多个版本，请手动处理，只保留一个版本"
                ))
                .to_message();
            tracing::error!(err_title, message);
        }
        // 取第一个作为保留的漫画
        let mut chosen_comic = comics.remove(0);
        // 导出状态以导出目录里真实的文件为准（元数据里的标记可能是旧的）
        if let Ok(comic_export_dir) = chosen_comic.get_comic_export_dir(&app) {
            mark_exported_chapters(&mut chosen_comic, &comic_export_dir, true);
        }
        unique_comics.push(chosen_comic);
    }

    unique_comics
}
