use std::sync::Arc;
use std::time::Duration;
use std::{
    fs::File,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
};

// TODO: 用`#![allow(clippy::used_underscore_binding)]`来消除警告
use eyre::{eyre, OptionExt, WrapErr};
use indexmap::IndexMap;
use tauri::{AppHandle, Manager};
use tauri_plugin_opener::OpenerExt;
use tauri_specta::Event;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tokio::time::sleep;
use tracing::{instrument, Instrument};
use walkdir::WalkDir;

use crate::config::{api_line_domains, ApiDomainMode, Config, ExportSkipMode, LocalLibrarySource};
use crate::export::manager::{ExportTaskKind, ExportTaskState};
use crate::errors::{CommandError, CommandResult};
use crate::events::{DownloadAllFavoritesEvent, UpdateDownloadedComicsEvent};
use crate::extensions::{AppHandleExt, EyreReportToMessage};
use crate::jm_client;
use crate::lines::{self, ApiLineProbeResult, ImageLineProbeResult};
use crate::local_index;
use crate::quick_reader::QuickReaderCandidate;
use crate::reader::{ReaderComic, ReaderState};
use crate::responses::{FavoriteFolderRespData, GetUserProfileRespData, GetWeeklyInfoRespData};
use crate::storage::{self, StorageStats};
use crate::types::{
    CategoryResp, ChapterInfo, Comic, ComicInFavorite, ComicInSearch, ComicInWeekly, FavoriteSort,
    GetFavoriteResult, GetWeeklyResult, LogMetadata, SearchResult, SearchResultVariant, SearchSort,
};
use crate::{export, logger, utils};

#[tauri::command]
#[specta::specta]
pub fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[tauri::command]
#[specta::specta]
#[allow(clippy::needless_pass_by_value)]
#[instrument(level = "error", skip_all)]
pub fn get_config(app: AppHandle) -> Config {
    app.get_config().read().clone()
}

#[tauri::command(async)]
#[specta::specta]
#[allow(clippy::needless_pass_by_value)]
#[instrument(level = "error", skip_all)]
pub fn save_config(app: AppHandle, config: Config) -> CommandResult<()> {
    // 代理地址非法就直接拒绝，不写进配置文件：否则下次启动建客户端会失败
    jm_client::validate_proxy_settings(&config.proxy_mode, &config.proxy_host, config.proxy_port)
        .map_err(|err| CommandError::from("代理设置不合法", err))?;

    let config_state = app.get_config();
    let jm_client = app.get_jm_client();

    let proxy_changed = {
        let config_state = config_state.read();
        config_state.proxy_mode != config.proxy_mode
            || config_state.proxy_host != config.proxy_host
            || config_state.proxy_port != config.proxy_port
    };

    let enable_file_logger = config.enable_file_logger;
    let file_logger_changed = config_state.read().enable_file_logger != enable_file_logger;

    {
        let mut config_state = config_state.write();
        *config_state = config;
        config_state
            .save(&app)
            .map_err(|err| CommandError::from("保存配置失败", err))?;
        tracing::debug!("保存配置成功");
    }

    if proxy_changed {
        jm_client
            .reload_client()
            .map_err(|err| CommandError::from("应用代理设置失败", err))?;
    }

    if file_logger_changed {
        if enable_file_logger {
            logger::reload_file_logger()
                .map_err(|err| CommandError::from("重新加载文件日志失败", err))?;
        } else {
            logger::disable_file_logger()
                .map_err(|err| CommandError::from("禁用文件日志失败", err))?;
        }
    }

    Ok(())
}

#[tauri::command]
#[specta::specta]
#[instrument(level = "error", skip_all)]
pub async fn login(
    app: AppHandle,
    username: String,
    password: String,
) -> CommandResult<GetUserProfileRespData> {
    let jm_client = app.get_jm_client();

    let user_profile = jm_client
        .login(&username, &password)
        .await
        .map_err(|err| CommandError::from("登录失败", err))?;

    Ok(user_profile)
}

/// 官方分类树 + 常用标签分组
#[tauri::command]
#[specta::specta]
#[instrument(level = "error", skip_all)]
pub async fn get_categories(app: AppHandle) -> CommandResult<CategoryResp> {
    let jm_client = app.get_jm_client();

    let categories = jm_client
        .get_categories()
        .await
        .map_err(|err| CommandError::from("获取分类失败", err))?;

    Ok(categories)
}

/// 对官方 API 线路做轻量测速，按延迟排序（失败的在最后）
#[tauri::command]
#[specta::specta]
#[instrument(level = "error", skip_all)]
pub async fn probe_api_lines(app: AppHandle) -> CommandResult<Vec<ApiLineProbeResult>> {
    let client = lines::create_probe_client(&app)
        .map_err(|err| CommandError::from("创建测速客户端失败", err))?;

    let custom_api_domain = app.get_config().read().custom_api_domain.clone();
    let mut targets: Vec<(String, ApiDomainMode, String)> = api_line_domains()
        .into_iter()
        .enumerate()
        .map(|(index, (mode, domain))| (format!("线路{}", index + 1), mode, domain.to_string()))
        .collect();
    if !custom_api_domain.is_empty() {
        targets.push(("自定义".to_string(), ApiDomainMode::Custom, custom_api_domain));
    }

    let mut join_set = JoinSet::new();
    for (label, mode, domain) in targets {
        let client = client.clone();
        join_set.spawn(async move {
            let result = jm_client::probe_api_domain(&client, &domain).await;
            (label, mode, domain, result)
        });
    }

    let mut results = Vec::new();
    while let Some(task) = join_set.join_next().await {
        let Ok((label, mode, domain, result)) = task else {
            continue;
        };
        results.push(match result {
            Ok(elapsed) => ApiLineProbeResult {
                label,
                mode,
                domain,
                ok: true,
                latency_ms: Some(u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)),
                error: None,
            },
            Err(err) => ApiLineProbeResult {
                label,
                mode,
                domain,
                ok: false,
                latency_ms: None,
                error: Some(err.to_string()),
            },
        });
    }

    results.sort_by_key(|item| (!item.ok, item.latency_ms.unwrap_or(u64::MAX)));

    Ok(results)
}

/// 对图片线路做轻量测速（图片 CDN 没有可用性接口，这里只测连通性和延迟）
#[tauri::command]
#[specta::specta]
#[instrument(level = "error", skip_all)]
pub async fn probe_image_lines(app: AppHandle) -> CommandResult<Vec<ImageLineProbeResult>> {
    let client = lines::create_probe_client(&app)
        .map_err(|err| CommandError::from("创建测速客户端失败", err))?;

    let mut join_set = JoinSet::new();
    for domain in lines::IMAGE_LINE_DOMAINS {
        let client = client.clone();
        join_set.spawn(async move {
            let result = jm_client::probe_image_domain(&client, domain).await;
            (domain, result)
        });
    }

    let mut results = Vec::new();
    while let Some(task) = join_set.join_next().await {
        let Ok((domain, result)) = task else {
            continue;
        };
        results.push(match result {
            Ok(elapsed) => ImageLineProbeResult {
                domain: domain.to_string(),
                ok: true,
                latency_ms: Some(u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)),
                error: None,
            },
            Err(err) => ImageLineProbeResult {
                domain: domain.to_string(),
                ok: false,
                latency_ms: None,
                error: Some(err.to_string()),
            },
        });
    }

    results.sort_by_key(|item| (!item.ok, item.latency_ms.unwrap_or(u64::MAX)));

    Ok(results)
}

/// 本地库存的标签统计（标签云用）
/// - 只回「标签 + 次数」，前端不用把整库漫画都拉下来
#[tauri::command]
#[specta::specta]
#[instrument(level = "error", skip_all)]
pub fn get_local_tags(app: AppHandle, source: LocalLibrarySource) -> Vec<local_index::LocalTag> {
    local_index::local_tags(&app, source)
}

/// 标签云：下载目录 + 导出目录共用的一份标签统计
#[tauri::command]
#[specta::specta]
#[instrument(level = "error", skip_all)]
pub fn get_local_tags_all(app: AppHandle) -> Vec<local_index::LocalTag> {
    local_index::all_local_tags(&app)
}

/// 当前正在使用的图片线路（失败会自动切换）
#[tauri::command]
#[specta::specta]
pub fn get_active_image_domain() -> String {
    lines::active_image_domain().to_string()
}

#[tauri::command]
#[specta::specta]
#[instrument(level = "error", skip_all)]
pub async fn get_user_profile(app: AppHandle) -> CommandResult<GetUserProfileRespData> {
    let jm_client = app.get_jm_client();

    let user_profile = jm_client
        .get_user_profile()
        .await
        .map_err(|err| CommandError::from("获取用户信息失败", err))?;

    Ok(user_profile)
}

#[tauri::command]
#[specta::specta]
#[instrument(
    level = "error",
    skip_all,
    fields(keyword = keyword, page = page, sort = ?sort)
)]
pub async fn search(
    app: AppHandle,
    keyword: String,
    page: i64,
    sort: SearchSort,
    year: Option<i64>,
    month: Option<i64>,
) -> CommandResult<SearchResultVariant> {
    let jm_client = app.get_jm_client();

    let search_resp = jm_client
        .search(&keyword, page, sort, year, month)
        .await
        .map_err(|err| CommandError::from("搜索失败", err))?;

    let search_result = SearchResultVariant::from_search_resp(&app, search_resp)
        .map_err(|err| CommandError::from("搜索失败", err))?;

    Ok(search_result)
}

#[tauri::command]
#[specta::specta]
#[instrument(level = "error", skip_all, fields(comic_id = comic_id))]
pub async fn toggle_favorite(app: AppHandle, comic_id: i64) -> CommandResult<()> {
    let jm_client = app.get_jm_client();

    jm_client
        .toggle_favorite_comic(comic_id)
        .await
        .map_err(|err| CommandError::from("收藏/取消收藏失败", err))?;

    Ok(())
}

#[tauri::command]
#[specta::specta]
#[instrument(level = "error", skip_all)]
pub async fn get_favorite_folders(app: AppHandle) -> CommandResult<Vec<FavoriteFolderRespData>> {
    let jm_client = app.get_jm_client();

    let favorites = jm_client
        .get_favorite_folder(0, 1, FavoriteSort::FavoriteTime)
        .await
        .map_err(|err| CommandError::from("获取收藏夹失败", err))?;

    Ok(favorites.folder_list)
}

#[tauri::command]
#[specta::specta]
#[instrument(level = "error", skip_all, fields(comic_id = comic_id, folder_id = folder_id))]
pub async fn move_favorite_to_folder(
    app: AppHandle,
    comic_id: i64,
    folder_id: String,
) -> CommandResult<()> {
    let jm_client = app.get_jm_client();

    jm_client
        .move_favorite_to_folder(comic_id, &folder_id)
        .await
        .map_err(|err| CommandError::from("移动收藏夹失败", err))?;

    Ok(())
}

#[tauri::command]
#[specta::specta]
#[instrument(
    level = "error",
    skip_all,
    fields(category = category, order = order, page = page)
)]
pub async fn get_ranking(
    app: AppHandle,
    category: String,
    order: String,
    page: i64,
) -> CommandResult<SearchResult> {
    let jm_client = app.get_jm_client();

    let ranking_resp_data = jm_client
        .get_ranking(&category, &order, page)
        .await
        .map_err(|err| CommandError::from("获取排行榜失败", err))?;

    let ranking_result = SearchResult::from_resp_data(&app, ranking_resp_data)
        .map_err(|err| CommandError::from("获取排行榜失败", err))?;

    Ok(ranking_result)
}

#[tauri::command]
#[specta::specta]
#[instrument(level = "error", skip_all, fields(aid = aid))]
pub async fn get_comic(app: AppHandle, aid: i64) -> CommandResult<Comic> {
    let comic = utils::get_comic(app.clone(), aid)
        .await
        .map_err(|err| CommandError::from("获取漫画信息失败", err))?;

    Ok(comic)
}

#[tauri::command(async)]
#[specta::specta]
#[instrument(
    level = "error",
    skip_all,
    fields(folder_id = folder_id, page = page, sort = ?sort)
)]
pub async fn get_favorite_folder(
    app: AppHandle,
    folder_id: i64,
    page: i64,
    sort: FavoriteSort,
) -> CommandResult<GetFavoriteResult> {
    let jm_client = app.get_jm_client();

    let get_favorite_resp_data = jm_client
        .get_favorite_folder(folder_id, page, sort)
        .await
        .map_err(|err| CommandError::from("获取收藏夹失败", err))?;

    let get_favorite_result = GetFavoriteResult::from_resp_data(&app, get_favorite_resp_data)
        .map_err(|err| CommandError::from("获取收藏夹失败", err))?;

    Ok(get_favorite_result)
}

#[tauri::command(async)]
#[specta::specta]
#[instrument(level = "error", skip_all)]
pub async fn get_weekly_info(app: AppHandle) -> CommandResult<GetWeeklyInfoRespData> {
    let jm_client = app.get_jm_client();

    let weekly_info = jm_client
        .get_weekly_info()
        .await
        .map_err(|err| CommandError::from("获取每周必看信息失败", err))?;

    Ok(weekly_info)
}

#[tauri::command(async)]
#[specta::specta]
#[instrument(
    level = "error",
    skip_all,
    fields(category_id = category_id, type_id = type_id)
)]
pub async fn get_weekly(
    app: AppHandle,
    category_id: String,
    type_id: String,
) -> CommandResult<GetWeeklyResult> {
    let jm_client = app.get_jm_client();

    let get_weekly_resp_data = jm_client
        .get_weekly(&category_id, &type_id)
        .await
        .map_err(|err| CommandError::from("获取每周必看失败", err))?;

    let get_weekly_result = GetWeeklyResult::from_resp_data(&app, get_weekly_resp_data)
        .map_err(|err| CommandError::from("获取每周必看失败", err))?;

    Ok(get_weekly_result)
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
        return Err(CommandError::from("一键下载漫画失败", err));
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
    let page_count = (total / count) + 1;
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

/// 导出目录的「更新库存」：去接口拉每本导出漫画的最新章节，只补导还没导出过的那些
/// - 返回补导了几本漫画
#[tauri::command(async)]
#[specta::specta]
#[instrument(level = "error", skip_all)]
pub async fn update_exported_comics(app: AppHandle) -> CommandResult<u32> {
    let interval_sec = app.get_config().read().update_downloaded_comics_interval_sec;
    let exported_comics = local_index::export_comics(&app);
    if exported_comics.is_empty() {
        return Ok(0);
    }

    let mut updated = 0;
    for (comic_id, comic_title, _) in exported_comics {
        let comic = match utils::get_comic(app.clone(), comic_id).await {
            Ok(comic) => comic,
            Err(err) => {
                let err_title = format!("更新导出目录过程中，获取漫画`{comic_title}`失败，已跳过");
                let err = err.wrap_err("可能是频率太高，请手动去`配置`里调整`更新库存时，每处理完一个已下载的漫画后休息`");
                tracing::error!(err_title, message = err.to_message());
                sleep(Duration::from_secs(interval_sec)).await;
                continue;
            }
        };

        match export::export_missing_chapters(&app, &comic).await {
            Ok(true) => updated += 1,
            Ok(false) => {}
            Err(err) => {
                let err_title = format!("漫画`{comic_title}`补导章节失败");
                tracing::error!(err_title, message = format!("{err:?}"));
            }
        }

        sleep(Duration::from_secs(interval_sec)).await;
    }

    Ok(updated)
}

#[allow(clippy::needless_pass_by_value)]
#[tauri::command(async)]
#[specta::specta]
#[instrument(level = "error", skip_all, fields(path = path))]
pub fn show_path_in_file_manager(app: AppHandle, path: &str) -> CommandResult<()> {
    app.opener()
        .reveal_item_in_dir(path)
        .map_err(|err| CommandError::from("在文件管理器中打开失败", err))?;
    Ok(())
}

#[tauri::command(async)]
#[specta::specta]
#[instrument(level = "error", skip_all)]
pub async fn sync_favorite_folder(app: AppHandle) -> CommandResult<()> {
    let jm_client = app.get_jm_client();
    // 同步收藏夹的方式是随便收藏一个漫画
    // 调用两次toggle是因为要把新收藏的漫画取消收藏
    let task1 = jm_client.toggle_favorite_comic(468_984);
    let task2 = jm_client.toggle_favorite_comic(468_984);
    let (resp1, resp2) =
        tokio::try_join!(task1, task2).map_err(|err| CommandError::from("同步收藏夹失败", err))?;
    if resp1.toggle_type == resp2.toggle_type {
        let toggle_type = resp1.toggle_type;
        let err_title = "同步收藏夹失败";
        let err = eyre!("两个请求都是`{toggle_type:?}`操作");
        return Err(CommandError::from(err_title, err));
    }

    Ok(())
}

/// 导出「单文件 HTML」快速阅读器：图片以 base64 内嵌，一个文件就能读
/// - 手机浏览器禁止 file:// 页面读取子目录图片，只有内嵌才能在手机上直接打开
/// - split_by_chapter = true 时每章一个文件
#[allow(clippy::needless_pass_by_value)]
#[tauri::command(async)]
#[specta::specta]
#[instrument(level = "error", skip_all, fields(source = ?source, target_dir = ?target_dir, split_by_chapter = split_by_chapter))]
pub async fn export_quick_reader_single_file(
    app: AppHandle,
    source: LocalLibrarySource,
    keys: Vec<String>,
    target_dir: Option<String>,
    split_by_chapter: bool,
) -> CommandResult<Vec<PathBuf>> {
    let app_for_task = app.clone();
    let files = tauri::async_runtime::spawn_blocking(move || {
        crate::quick_reader::export_quick_reader_single_file(
            &app_for_task,
            source,
            keys,
            target_dir,
            split_by_chapter,
        )
    })
    .await
    .map_err(|err| CommandError::from("导出单文件阅读器失败", eyre::Report::from(err)))?
    .map_err(|err| CommandError::from("导出单文件阅读器失败", err))?;

    tracing::debug!("单文件阅读器已导出 {} 个文件", files.len());
    Ok(files)
}

/// 列出某个目录里可以导出进快速阅读器的漫画（给前端勾选）
#[allow(clippy::needless_pass_by_value)]
#[tauri::command(async)]
#[specta::specta]
#[instrument(level = "error", skip_all, fields(source = ?source))]
pub async fn list_quick_reader_candidates(
    app: AppHandle,
    source: LocalLibrarySource,
) -> CommandResult<Vec<QuickReaderCandidate>> {
    let app_for_task = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::quick_reader::list_candidates(&app_for_task, source)
    })
    .await
    .map_err(|err| CommandError::from("读取漫画列表失败", eyre::Report::from(err)))?
    .map_err(|err| CommandError::from("读取漫画列表失败", err))
}

/// 导出一份可以在浏览器里直接打开的快速阅读器（分享包）
/// - source = 从哪个目录收集漫画（导出目录 / 下载目录）
/// - keys = 选中的漫画（相对来源目录的路径）；为空表示全部
/// - dir_name = 阅读器文件夹名，重名会自动加 -2、-3 后缀
/// - target_dir = 写到哪个目录（绝对路径）；为空则写到来源目录
#[allow(clippy::needless_pass_by_value)]
#[tauri::command(async)]
#[specta::specta]
#[instrument(level = "error", skip_all, fields(source = ?source, dir_name = ?dir_name, target_dir = ?target_dir))]
pub async fn export_quick_reader(
    app: AppHandle,
    source: LocalLibrarySource,
    keys: Vec<String>,
    dir_name: Option<String>,
    target_dir: Option<String>,
) -> CommandResult<PathBuf> {
    let app_for_task = app.clone();
    let dir = tauri::async_runtime::spawn_blocking(move || {
        crate::quick_reader::export_quick_reader(&app_for_task, source, keys, dir_name, target_dir)
    })
    .await
    .map_err(|err| CommandError::from("导出快速阅读器失败", eyre::Report::from(err)))?
    .map_err(|err| CommandError::from("导出快速阅读器失败", err))?;

    tracing::debug!("快速阅读器已导出到 {}", dir.display());
    Ok(dir)
}

#[tauri::command]
#[specta::specta]
#[instrument(level = "error", skip_all, fields(comic_id = comic.id, comic_title = comic.name))]
pub fn open_comic_reader(app: AppHandle, comic: Comic) -> CommandResult<ReaderComic> {
    let comic_dir = comic
        .comic_download_dir
        .clone()
        .ok_or_eyre("`comic_download_dir`字段为`None`")
        .map_err(|err| CommandError::from("打开阅读器失败", err))?;

    let state = app.state::<ReaderState>();
    let reader_comic = crate::reader::open_reader(&state, &comic.name, &comic_dir)
        .map_err(|err| CommandError::from("打开阅读器失败", err))?;

    tracing::debug!("阅读器打开成功，章节数={}", reader_comic.chapters.len());
    Ok(reader_comic)
}

/// 按漫画ID打开阅读器：本地有就读本地，没有就走在线阅读
#[tauri::command(async)]
#[specta::specta]
#[instrument(level = "error", skip_all, fields(comic_id = comic_id))]
pub async fn open_reader_by_id(app: AppHandle, comic_id: i64) -> CommandResult<ReaderComic> {
    // 先查本地索引：已经下载过的漫画直接开，不用先请求一次接口
    // （以前是"先请求 /album 拿漫画信息，再判断本地有没有"，离线时已下载的漫画也打不开）
    if let Some((comic_dir, comic_title)) = local_index::comic_dir_and_name(&app, comic_id) {
        let state = app.state::<ReaderState>();
        if let Ok(reader_comic) = crate::reader::open_reader(&state, &comic_title, &comic_dir) {
            tracing::debug!("阅读器打开成功(本地)，章节数={}", reader_comic.chapters.len());
            return Ok(reader_comic);
        }
    }

    let comic = utils::get_comic(app.clone(), comic_id)
        .await
        .map_err(|err| CommandError::from("打开阅读器失败", err))?;

    // 索引里没有、但接口返回里带了下载目录，再试一次本地（比如刚下载完还没被索引扫到）
    if let Some(comic_dir) = comic.comic_download_dir.clone() {
        let state = app.state::<ReaderState>();
        if let Ok(reader_comic) = crate::reader::open_reader(&state, &comic.name, &comic_dir) {
            tracing::debug!("阅读器打开成功(本地)，章节数={}", reader_comic.chapters.len());
            return Ok(reader_comic);
        }
    }

    // 没下载过：在线阅读
    let chapters = comic
        .chapter_infos
        .iter()
        .map(|chapter_info| (chapter_info.chapter_id, chapter_info.chapter_title.clone()))
        .collect::<Vec<_>>();

    if chapters.is_empty() {
        return Err(CommandError::from(
            "打开阅读器失败",
            eyre!("这部漫画没有章节"),
        ));
    }

    let state = app.state::<ReaderState>();
    let reader_comic = crate::reader::open_reader_remote(&state, &comic.name, chapters);
    tracing::debug!(
        "阅读器打开成功(在线)，章节数={}",
        reader_comic.chapters.len()
    );
    Ok(reader_comic)
}

/// 准备在线章节（请求图片地址），返回页数
#[tauri::command(async)]
#[specta::specta]
#[instrument(level = "error", skip_all, fields(token = token))]
pub async fn prepare_reader_chapter(app: AppHandle, token: u32) -> CommandResult<usize> {
    let state = app.state::<ReaderState>();
    crate::reader::prepare_chapter(&app, &state, token)
        .await
        .map_err(|err| CommandError::from("加载章节失败", err))
}

#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
#[specta::specta]
#[instrument(level = "error", skip_all)]
pub fn close_reader(app: AppHandle) {
    let state = app.state::<ReaderState>();
    crate::reader::close_reader(&state);
    tracing::debug!("阅读器已关闭，缓存已释放");
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
/// - 导出时会把 `元数据.json` 一并写到导出目录，这里直接读它
/// - 没有元数据的目录（本次改动之前导出的）会被跳过，重新导出一次即可生成
#[allow(clippy::needless_pass_by_value)]
#[tauri::command(async)]
#[specta::specta]
#[instrument(level = "error", skip_all)]
pub fn get_exported_comics(app: AppHandle) -> Vec<Comic> {
    let export_dir = app.get_config().read().export_dir.clone();

    // 以「cbz 子目录」为锚点，找出所有导出过 cbz 的漫画目录
    let mut comic_dirs_with_modify_time = Vec::new();
    for entry in WalkDir::new(&export_dir).into_iter().filter_map(Result::ok) {
        if !entry.file_type().is_dir() || entry.file_name() != "cbz" {
            continue;
        }
        let cbz_dir = entry.path();
        if !has_cbz_file(cbz_dir) {
            continue;
        }
        let Some(comic_export_dir) = cbz_dir.parent() else {
            continue;
        };

        let modify_time = comic_export_dir
            .metadata()
            .and_then(|metadata| metadata.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        comic_dirs_with_modify_time.push((comic_export_dir.to_path_buf(), modify_time));
    }
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

        mark_cbz_exported_chapters(&mut comic, &comic_export_dir);

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

/// 判断目录里是否至少有一个 cbz 文件
fn has_cbz_file(dir: &Path) -> bool {
    let Ok(read_dir) = std::fs::read_dir(dir) else {
        return false;
    };

    read_dir
        .filter_map(Result::ok)
        .any(|entry| entry.path().extension().is_some_and(|ext| ext.eq_ignore_ascii_case("cbz")))
}

/// 根据导出目录里已存在的 cbz 文件，标记对应章节的导出状态
#[instrument(level = "error", skip_all, fields(comic_id = comic.id, comic_title = comic.name))]
fn mark_cbz_exported_chapters(comic: &mut Comic, comic_export_dir: &Path) {
    let cbz_dir = comic_export_dir.join("cbz");
    let Ok(read_dir) = std::fs::read_dir(&cbz_dir) else {
        return;
    };

    let cbz_file_names: Vec<String> = read_dir
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().to_string())
        .filter(|name| name.to_lowercase().ends_with(".cbz"))
        .collect();

    for chapter_info in &mut comic.chapter_infos {
        let cbz_file_name = format!("{}.cbz", utils::filename_filter(&chapter_info.chapter_title));
        if cbz_file_names.contains(&cbz_file_name) {
            chapter_info.is_cbz_exported = true;
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
        let chosen_comic = comics.remove(0);
        unique_comics.push(chosen_comic);
    }

    unique_comics
}

#[tauri::command(async)]
#[specta::specta]
#[allow(clippy::needless_pass_by_value)]
#[instrument(level = "error", skip_all, fields(comic_id = comic.id, comic_title = comic.name))]
pub fn export_cbz(app: AppHandle, comic: Comic) -> CommandResult<()> {
    let skip_mode = app.get_config().read().export_skip_mode;
    let total = u32::try_from(
        comic
            .chapter_infos
            .iter()
            .filter(|chapter| chapter.is_downloaded.unwrap_or(false))
            .count(),
    )
    .unwrap_or(u32::MAX);
    let comic_export_dir = comic
        .get_comic_export_dir(&app)
        .map_err(|err| CommandError::from("导出cbz失败", err))?;

    let task = app.get_export_manager().create_task(
        uuid::Uuid::new_v4().to_string(),
        comic.id,
        comic.name.clone(),
        ExportTaskKind::Cbz,
        total,
        comic_export_dir,
    );

    export::cbz(&app, &comic, &task, skip_mode).map_err(|err| {
        task.set_state(ExportTaskState::Failed);
        CommandError::from("导出cbz失败", err)
    })?;

    Ok(())
}

#[tauri::command(async)]
#[specta::specta]
#[allow(clippy::needless_pass_by_value)]
#[instrument(level = "error", skip_all, fields(comic_id = comic.id, comic_title = comic.name))]
pub fn export_pdf(app: AppHandle, comic: Comic) -> CommandResult<()> {
    export::pdf(&app, &comic).map_err(|err| CommandError::from("导出pdf失败", err))?;
    Ok(())
}

#[tauri::command(async)]
#[specta::specta]
#[allow(clippy::needless_pass_by_value)]
pub fn export_cbz_chapters(
    app: AppHandle,
    comic: Comic,
    chapter_ids: Vec<i64>,
) -> CommandResult<()> {
    let comic_title = comic.name.clone();
    let total = u32::try_from(
        comic
            .chapter_infos
            .iter()
            .filter(|chapter| {
                chapter.is_downloaded.unwrap_or(false) && chapter_ids.contains(&chapter.chapter_id)
            })
            .count(),
    )
    .unwrap_or(u32::MAX);
    let comic_export_dir = comic
        .get_comic_export_dir(&app)
        .map_err(|err| CommandError::from("导出指定章节cbz失败", err))?;

    let task = app.get_export_manager().create_task(
        uuid::Uuid::new_v4().to_string(),
        comic.id,
        comic_title.clone(),
        ExportTaskKind::Cbz,
        total,
        comic_export_dir,
    );

    export::cbz_chapters(&app, &comic, chapter_ids, &task, ExportSkipMode::None)
        .wrap_err(format!("漫画`{comic_title}`导出指定章节cbz失败"))
        .map_err(|err| {
            task.set_state(ExportTaskState::Failed);
            CommandError::from("导出指定章节cbz失败", err)
        })?;
    Ok(())
}

#[tauri::command(async)]
#[specta::specta]
#[allow(clippy::needless_pass_by_value)]
pub fn export_pdf_chapters(
    app: AppHandle,
    comic: Comic,
    chapter_ids: Vec<i64>,
) -> CommandResult<()> {
    let comic_title = comic.name.clone();
    export::pdf_chapters(&app, &comic, chapter_ids)
        .wrap_err(format!("漫画`{comic_title}`导出指定章节pdf失败"))
        .map_err(|err| CommandError::from("导出指定章节pdf失败", err))?;
    Ok(())
}


/// 暂停导出任务：当前章节做完后停下，任务留在暂停状态
#[tauri::command(async)]
#[specta::specta]
#[instrument(level = "error", skip_all, fields(uuid = uuid))]
pub fn pause_export_task(app: AppHandle, uuid: String) -> CommandResult<()> {
    let Some(task) = app.get_export_manager().get(&uuid) else {
        return Err(CommandError::from(
            "暂停导出任务失败",
            eyre!("未找到导出任务 {uuid}"),
        ));
    };

    task.set_state(ExportTaskState::Paused);
    Ok(())
}

/// 继续导出任务：用「跳过已存在」重新跑一遍，已经导完的章节会跳过
#[tauri::command(async)]
#[specta::specta]
#[instrument(level = "error", skip_all, fields(uuid = uuid))]
pub async fn resume_export_task(app: AppHandle, uuid: String) -> CommandResult<()> {
    let Some(task) = app.get_export_manager().get(&uuid) else {
        return Err(CommandError::from(
            "继续导出任务失败",
            eyre!("未找到导出任务 {uuid}"),
        ));
    };

    task.set_state(ExportTaskState::Exporting);

    match task.kind {
        ExportTaskKind::Cbz => {
            let Some(comic) = load_local_comic(&app, task.comic_id) else {
                task.set_state(ExportTaskState::Failed);
                return Err(CommandError::from(
                    "继续导出任务失败",
                    eyre!("本地找不到这本漫画的元数据，无法继续导出"),
                ));
            };

            tauri::async_runtime::spawn_blocking(move || {
                if let Err(err) = export::cbz(&app, &comic, &task, ExportSkipMode::SkipExisting) {
                    tracing::error!(message = format!("{err:?}"), "继续导出cbz失败");
                    task.set_state(ExportTaskState::Failed);
                }
            });
        }
        ExportTaskKind::CbzDirect => {
            let comic = utils::get_comic(app.clone(), task.comic_id)
                .await
                .map_err(|err| {
                    task.set_state(ExportTaskState::Failed);
                    CommandError::from("继续导出任务失败", err)
                })?;

            tauri::async_runtime::spawn(async move {
                if let Err(err) = export::resume_comic_cbz(&app, &comic, &task).await {
                    tracing::error!(message = format!("{err:?}"), "继续导出cbz失败");
                    task.set_state(ExportTaskState::Failed);
                }
            });
        }
    }

    Ok(())
}

/// 删除导出任务；delete_files 为 true 时连这本漫画的导出目录一起删掉
#[tauri::command(async)]
#[specta::specta]
#[instrument(level = "error", skip_all, fields(uuid = uuid))]
pub fn delete_export_task(app: AppHandle, uuid: String, delete_files: bool) -> CommandResult<()> {
    app.get_export_manager()
        .delete_task(&uuid, delete_files)
        .map_err(|err| CommandError::from("删除导出任务失败", err))?;
    Ok(())
}

/// 前端挂载后同步一次导出任务，把恢复出来的任务补进列表
#[tauri::command(async)]
#[specta::specta]
pub fn sync_export_tasks(app: AppHandle) {
    app.get_export_manager().sync_tasks();
}

/// 从本地下载目录的元数据里还原 Comic（继续导出时用）
fn load_local_comic(app: &AppHandle, comic_id: i64) -> Option<Comic> {
    let (comic_download_dir, _comic_title) = local_index::comic_dir_and_name(app, comic_id)?;
    Comic::from_metadata(&comic_download_dir.join("元数据.json")).ok()
}

#[allow(clippy::cast_possible_wrap)]#[tauri::command(async)]
#[specta::specta]
#[instrument(level = "error", skip_all, fields(folder_id = folder_id, sort = ?sort))]
pub async fn get_all_favorite_comics(
    app: AppHandle,
    folder_id: i64,
    sort: FavoriteSort,
) -> CommandResult<Vec<ComicInFavorite>> {
    let jm_client = app.get_jm_client().inner().clone();

    let first_page = jm_client
        .get_favorite_folder(folder_id, 1, sort.clone())
        .await
        .map_err(|err| CommandError::from("获取收藏夹失败", err))?;

    let mut comic_resp_datas = first_page.list;
    let count = first_page.count;
    let total = first_page
        .total
        .parse::<i64>()
        .map_err(|err| CommandError::from("获取收藏夹失败", err))?;
    let page_count = if count > 0 { (total / count) + 1 } else { 1 };

    let sem = Arc::new(Semaphore::new(5));
    let mut join_set = JoinSet::new();
    for page in 2..=page_count {
        let jm_client = jm_client.clone();
        let sem = sem.clone();
        let sort = sort.clone();
        join_set.spawn(async move {
            let _permit = sem.acquire().await?;
            jm_client.get_favorite_folder(folder_id, page, sort).await
        });
    }

    while let Some(result) = join_set.join_next().await {
        let page = match result {
            Ok(Ok(page)) => page,
            Ok(Err(err)) => return Err(CommandError::from("获取收藏夹失败", err)),
            Err(err) => {
                return Err(CommandError::from(
                    "获取收藏夹失败",
                    eyre::Report::from(err),
                ))
            }
        };
        comic_resp_datas.extend(page.list);
    }

    let id_to_dir_map = utils::create_id_to_dir_map(&app)
        .map_err(|err| CommandError::from("获取收藏夹失败", err))?;
    let comics = comic_resp_datas
        .into_iter()
        .map(|comic| ComicInFavorite::from_resp_data(comic, &id_to_dir_map))
        .collect::<eyre::Result<Vec<_>>>()
        .map_err(|err| CommandError::from("获取收藏夹失败", err))?;

    Ok(comics)
}

#[allow(clippy::needless_pass_by_value)]
#[tauri::command(async)]
#[specta::specta]
#[instrument(level = "error", skip_all)]
pub async fn export_cbz_without_download(
    app: AppHandle,
    comic_ids: Vec<i64>,
) -> CommandResult<()> {
    export::export_cbz_without_download(app, comic_ids)
        .await
        .map_err(|err| CommandError::from("导出cbz失败", err))?;
    Ok(())
}

#[allow(clippy::needless_pass_by_value)]
#[tauri::command(async)]
#[specta::specta]
#[instrument(level = "error", skip_all)]
pub fn get_logs_dir_size(app: AppHandle) -> CommandResult<u64> {
    let logs_dir = logger::logs_dir(&app)
        .wrap_err("获取日志目录失败")
        .map_err(|err| CommandError::from("获取日志目录大小失败", err))?;
    let logs_dir_size = std::fs::read_dir(&logs_dir)
        .wrap_err(format!("读取日志目录`{}`失败", logs_dir.display()))
        .map_err(|err| CommandError::from("获取日志目录大小失败", err))?
        .filter_map(Result::ok)
        .filter_map(|entry| entry.metadata().ok())
        .map(|metadata| metadata.len())
        .sum::<u64>();
    tracing::debug!("获取日志目录大小成功");
    Ok(logs_dir_size)
}

/// 空间统计：下载目录 / 导出目录 / 日志目录的占用，以及可以清理的东西
#[tauri::command(async)]
#[specta::specta]
#[instrument(level = "error", skip_all)]
pub fn get_storage_stats(app: AppHandle) -> CommandResult<StorageStats> {
    storage::stats(&app).map_err(|err| CommandError::from("统计空间占用失败", err))
}

/// 清理下载残留（`.下载中-*`），返回释放的字节数
#[tauri::command(async)]
#[specta::specta]
#[instrument(level = "error", skip_all)]
pub fn clean_storage_leftovers(app: AppHandle) -> CommandResult<u64> {
    storage::clean_leftovers(&app).map_err(|err| CommandError::from("清理下载残留失败", err))
}

/// 清理快速阅读器分享包（只删带阅读器标识文件的目录），返回释放的字节数
#[tauri::command(async)]
#[specta::specta]
#[instrument(level = "error", skip_all)]
pub fn clean_storage_quick_readers(app: AppHandle, paths: Vec<String>) -> CommandResult<u64> {
    storage::clean_quick_readers(&app, paths)
        .map_err(|err| CommandError::from("清理快速阅读器分享包失败", err))
}

/// 清理旧日志（保留最近 24 小时内以及最新的一份），返回释放的字节数
#[tauri::command(async)]
#[specta::specta]
#[instrument(level = "error", skip_all)]
pub fn clean_storage_logs(app: AppHandle) -> CommandResult<u64> {
    storage::clean_logs(&app).map_err(|err| CommandError::from("清理旧日志失败", err))
}

/// 删除一本漫画在下载目录或导出目录里的文件夹，返回释放的字节数
#[tauri::command(async)]
#[specta::specta]
#[instrument(level = "error", skip_all, fields(comic_id = comic_id))]
pub fn delete_local_comic(
    app: AppHandle,
    comic_id: i64,
    source: LocalLibrarySource,
) -> CommandResult<u64> {
    storage::delete_comic_dir(&app, comic_id, source)
        .map_err(|err| CommandError::from("删除本地漫画失败", err))
}

#[allow(clippy::needless_pass_by_value)]
#[tauri::command(async)]
#[specta::specta]
#[instrument(level = "error", skip_all, fields(comic_id = comic.id, comic_title = comic.name))]
pub fn get_synced_comic(app: AppHandle, mut comic: Comic) -> CommandResult<Comic> {
    let id_to_dir_map = utils::create_id_to_dir_map(&app)
        .map_err(|err| CommandError::from("同步Comic字段失败", err))?;

    comic
        .update_fields(&id_to_dir_map)
        .map_err(|err| CommandError::from("同步Comic字段失败", err))?;

    Ok(comic)
}

#[allow(clippy::needless_pass_by_value)]
#[tauri::command(async)]
#[specta::specta]
#[instrument(level = "error", skip_all, fields(comic_id = comic.id, comic_title = comic.name))]
pub fn get_synced_comic_in_favorite(
    app: AppHandle,
    mut comic: ComicInFavorite,
) -> CommandResult<ComicInFavorite> {
    let id_to_dir_map = utils::create_id_to_dir_map(&app)
        .map_err(|err| CommandError::from("同步ComicInFavorite字段失败", err))?;

    comic.update_fields(&id_to_dir_map);

    Ok(comic)
}

#[allow(clippy::needless_pass_by_value)]
#[tauri::command(async)]
#[specta::specta]
#[instrument(level = "error", skip_all, fields(comic_id = comic.id, comic_title = comic.name))]
pub fn get_synced_comic_in_search(
    app: AppHandle,
    mut comic: ComicInSearch,
) -> CommandResult<ComicInSearch> {
    let id_to_dir_map = utils::create_id_to_dir_map(&app)
        .map_err(|err| CommandError::from("同步ComicInSearch字段失败", err))?;

    comic.update_fields(&id_to_dir_map);

    Ok(comic)
}

#[allow(clippy::needless_pass_by_value)]
#[tauri::command(async)]
#[specta::specta]
#[instrument(level = "error", skip_all, fields(comic_id = comic.id, comic_title = comic.name))]
pub fn get_synced_comic_in_weekly(
    app: AppHandle,
    mut comic: ComicInWeekly,
) -> CommandResult<ComicInWeekly> {
    let id_to_dir_map = utils::create_id_to_dir_map(&app)
        .map_err(|err| CommandError::from("同步ComicInWeekly字段失败", err))?;

    comic.update_fields(&id_to_dir_map);

    Ok(comic)
}

#[allow(clippy::needless_pass_by_value)]
#[tauri::command(async)]
#[specta::specta]
#[instrument(level = "error", skip_all, fields(path = path))]
pub fn open_log_file(path: &str) -> CommandResult<Vec<LogMetadata>> {
    let log_file = File::open(path).map_err(|err| CommandError::from("打开日志文件失败", err))?;
    let reader = BufReader::new(log_file);

    let mut logs = Vec::new();
    let mut line_num = 0;

    for line_result in reader.lines() {
        line_num += 1;

        let line = line_result
            .wrap_err(format!("读取日志文件的第`{line_num}`行失败"))
            .map_err(|err| CommandError::from("打开日志文件失败", err))?;

        if line.trim().is_empty() {
            continue;
        }

        let log = serde_json::from_str::<LogMetadata>(&line)
            .wrap_err(format!("将日志文件的第`{line_num}`行解析为LogMetadata失败"))
            .map_err(|err| CommandError::from("打开日志文件失败", err))?;

        logs.push(log);
    }

    Ok(logs)
}
