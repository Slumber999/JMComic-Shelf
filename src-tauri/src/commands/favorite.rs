use std::sync::Arc;

use eyre::eyre;
use tauri::AppHandle;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tracing::instrument;

use crate::comic_tags::{self, FavoritesTagsSnapshot, FetchComicTagsResult};
use crate::errors::{CommandError, CommandResult};
use crate::extensions::AppHandleExt;
use crate::favorites_index::{
    self, BuildFavoritesIndexResult, FavoritesFolderIndex, UpdateFavoritesIndexResult,
};
use crate::local_index;
use crate::responses::{FavoriteFolderRespData, ToggleType};
use crate::types::{ComicInFavorite, FavoriteSort, GetFavoriteResult};
use crate::utils;
#[tauri::command]
#[specta::specta]
#[instrument(level = "error", skip_all, fields(comic_id = comic_id))]
pub async fn toggle_favorite(app: AppHandle, comic_id: i64) -> CommandResult<()> {
    let jm_client = app.get_jm_client();

    let resp = jm_client
        .toggle_favorite_comic(comic_id)
        .await
        .map_err(|err| CommandError::from("收藏/取消收藏失败", err))?;

    // 取消收藏后本地索引里这本也要删掉；索引更新失败不影响这次取消收藏
    if resp.toggle_type == ToggleType::Remove {
        if let Err(err) = favorites_index::remove_comic(&app, comic_id).await {
            tracing::error!(message = ?err, "取消收藏后删除收藏夹索引失败");
        }
    }

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

    // 收藏夹列表里就有账号 uid，顺手记给索引用，省掉一次请求
    favorites_index::remember_account_uid(&favorites.folder_list);

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
    let page_count = utils::page_count(total, count);

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
    let exported_ids = local_index::exported_id_set(&app);
    let comics = comic_resp_datas
        .into_iter()
        .map(|comic| ComicInFavorite::from_resp_data(comic, &id_to_dir_map, &exported_ids))
        .collect::<eyre::Result<Vec<_>>>()
        .map_err(|err| CommandError::from("获取收藏夹失败", err))?;

    Ok(comics)
}

/// 读本地收藏夹索引；没有索引（或拿不到索引文件）返回 None，不影响收藏夹页
#[tauri::command(async)]
#[specta::specta]
#[instrument(level = "error", skip_all, fields(folder_id = folder_id))]
pub async fn get_favorites_index(app: AppHandle, folder_id: i64) -> Option<FavoritesFolderIndex> {
    favorites_index::get(&app, folder_id).await
}

/// 全量扫描该收藏夹，整层覆盖写入索引
#[tauri::command(async)]
#[specta::specta]
#[instrument(level = "error", skip_all, fields(folder_id = folder_id))]
pub async fn build_favorites_index(
    app: AppHandle,
    folder_id: i64,
) -> CommandResult<BuildFavoritesIndexResult> {
    favorites_index::build(&app, folder_id)
        .await
        .map_err(|err| CommandError::from("建立收藏夹索引失败", err))
}

/// 增量扫描该收藏夹：从第 1 页往后扫，整页没有新漫画就停
#[tauri::command(async)]
#[specta::specta]
#[instrument(level = "error", skip_all, fields(folder_id = folder_id))]
pub async fn update_favorites_index(
    app: AppHandle,
    folder_id: i64,
) -> CommandResult<UpdateFavoritesIndexResult> {
    favorites_index::update(&app, folder_id)
        .await
        .map_err(|err| CommandError::from("更新收藏夹索引失败", err))
}

/// 从该账号所有收藏夹层里删掉这本漫画
#[tauri::command(async)]
#[specta::specta]
#[instrument(level = "error", skip_all, fields(comic_id = comic_id))]
pub async fn remove_from_favorites_index(app: AppHandle, comic_id: i64) -> CommandResult<()> {
    favorites_index::remove_comic(&app, comic_id)
        .await
        .map_err(|err| CommandError::from("更新收藏夹索引失败", err))
}

/// 收藏夹的标签快照：标签云（按出现次数倒序）+ 已经取到标签的每本漫画
/// - 纯本地读缓存，不发请求；没缓存过的本不参与统计
#[tauri::command(async)]
#[specta::specta]
#[instrument(level = "error", skip_all, fields(folder_id = folder_id))]
pub async fn get_favorites_tags(app: AppHandle, folder_id: i64) -> FavoritesTagsSnapshot {
    comic_tags::favorites_snapshot(&app, folder_id).await
}

/// 给这些漫画补标签：最多请求 limit 本（并发 8）
/// - 已经在缓存里、或本地元数据里就有标签的本，一次请求都不发
#[tauri::command(async)]
#[specta::specta]
#[instrument(level = "error", skip_all, fields(limit = limit))]
pub async fn fetch_comic_tags(
    app: AppHandle,
    comic_ids: Vec<i64>,
    limit: usize,
) -> CommandResult<FetchComicTagsResult> {
    comic_tags::fetch_many(&app, comic_ids, limit)
        .await
        .map_err(|err| CommandError::from("获取漫画标签失败", err))
}

/// 一本漫画的标签：缓存优先（收藏夹标签筛选和多关键词搜索共用这一份缓存）
#[tauri::command]
#[specta::specta]
#[instrument(level = "error", skip_all, fields(comic_id = comic_id))]
pub async fn get_comic_tags(app: AppHandle, comic_id: i64) -> CommandResult<Vec<String>> {
    comic_tags::tags_of(&app, comic_id)
        .await
        .map_err(|err| CommandError::from("获取漫画信息失败", err))
}
