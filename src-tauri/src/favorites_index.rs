//! 收藏夹本地索引：一个账号一份文件，搜索时纯本地过滤
//!
//! 接口只有「按收藏时间倒序」的分页，没有收藏夹搜索，所以本地存一份；
//! 增量更新从第 1 页往后扫，整页没有新漫画就停。

use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{Arc, LazyLock},
    time::{SystemTime, UNIX_EPOCH},
};

use eyre::{eyre, WrapErr};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager};
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

use crate::extensions::AppHandleExt;
use crate::jm_client::JmClient;
use crate::responses::{ComicInFavoriteRespData, FavoriteFolderRespData};
use crate::types::FavoriteSort;

/// 翻页并发数：和 get_all_favorite_comics 保持一致
const PAGE_CONCURRENCY: usize = 5;

/// 索引里的一本漫画：只留搜索要用的三个字段
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FavoritesIndexComic {
    pub id: i64,
    pub name: String,
    pub author: String,
}

/// 一个收藏夹层的索引，comics 按收藏时间倒序
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FavoritesFolderIndex {
    pub folder_name: String,
    /// ISO8601（UTC）
    pub updated_at: String,
    pub total: usize,
    pub comics: Vec<FavoritesIndexComic>,
}

/// 索引文件内容：收藏夹 id -> 该层索引
type IndexFile = HashMap<String, FavoritesFolderIndex>;

/// 全量扫描结果：这次比原索引多收了几本、扫完一共几本
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct BuildFavoritesIndexResult {
    pub added: usize,
    pub total: usize,
}

/// 增量扫描结果：added / removed 都是这次扫描相对原索引的变化量
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdateFavoritesIndexResult {
    pub added: usize,
    pub removed: usize,
    pub total: usize,
}

/// 当前账号 uid：进程内缓存，省掉每次读写索引都请求一次收藏夹列表
static ACCOUNT_UID: LazyLock<RwLock<Option<String>>> = LazyLock::new(|| RwLock::new(None));

/// 登出时清掉：换账号后索引文件不是同一份
pub fn reset_account_uid() {
    *ACCOUNT_UID.write() = None;
}

/// 收藏夹列表里带账号 uid，拿到列表的地方顺手登记一下
pub fn remember_account_uid(folder_list: &[FavoriteFolderRespData]) {
    if let Some(uid) = folder_list
        .iter()
        .map(|folder| folder.uid.as_str())
        .find(|uid| !uid.is_empty())
    {
        *ACCOUNT_UID.write() = Some(uid.to_string());
    }
}

/// 读取某层的索引；拿不到 uid、没有文件、文件读坏都当「还没有索引」
pub async fn get(app: &AppHandle, folder_id: i64) -> Option<FavoritesFolderIndex> {
    let uid = match account_uid(app).await {
        Ok(uid) => uid,
        Err(err) => {
            tracing::error!(message = ?err, "读取收藏夹索引失败");
            return None;
        }
    };

    load_index_file(app, &uid).remove(&folder_id.to_string())
}

/// 全量扫描该收藏夹的所有页，整层覆盖写入
pub async fn build(
    app: &AppHandle,
    folder_id: i64,
) -> eyre::Result<BuildFavoritesIndexResult> {
    let jm_client = app.get_jm_client().inner().clone();

    let first_page = jm_client
        .get_favorite_folder(folder_id, 1, FavoriteSort::FavoriteTime)
        .await?;
    remember_account_uid(&first_page.folder_list);
    let uid = account_uid(app).await?;

    let count = first_page.count;
    let total = first_page
        .total
        .parse::<i64>()
        .wrap_err("将total解析为i64失败")?;
    let page_count = if count > 0 { (total + count - 1) / count } else { 1 };

    let mut comic_resp_datas = first_page.list;
    comic_resp_datas.extend(fetch_pages(&jm_client, folder_id, 2, page_count).await?);
    // 页内顺序 + 页码顺序拼起来就是收藏时间倒序
    let comics: Vec<FavoritesIndexComic> = comic_resp_datas
        .into_iter()
        .filter_map(index_comic)
        .collect();

    let mut index_file = load_index_file(app, &uid);
    let key = folder_id.to_string();
    let old_ids: HashSet<i64> = index_file
        .get(&key)
        .map(|index| index.comics.iter().map(|comic| comic.id).collect())
        .unwrap_or_default();
    let added = comics
        .iter()
        .filter(|comic| !old_ids.contains(&comic.id))
        .count();
    let folder_name = folder_name_of(&first_page.folder_list, folder_id)
        .or_else(|| index_file.get(&key).map(|index| index.folder_name.clone()))
        .unwrap_or_else(|| format!("收藏夹 {folder_id}"));

    let total = comics.len();
    index_file.insert(
        key,
        FavoritesFolderIndex {
            folder_name,
            updated_at: now_iso8601(),
            total,
            comics,
        },
    );
    save_index_file(app, &uid, &index_file)?;

    Ok(BuildFavoritesIndexResult { added, total })
}

/// 从第 1 页往后扫，整页没有新 id 就停；同时按扫到的结果更新名称 / 作者
pub async fn update(
    app: &AppHandle,
    folder_id: i64,
) -> eyre::Result<UpdateFavoritesIndexResult> {
    let jm_client = app.get_jm_client().inner().clone();

    let first_page = jm_client
        .get_favorite_folder(folder_id, 1, FavoriteSort::FavoriteTime)
        .await?;
    remember_account_uid(&first_page.folder_list);
    let uid = account_uid(app).await?;

    let count = first_page.count;
    let total = first_page
        .total
        .parse::<i64>()
        .wrap_err("将total解析为i64失败")?;
    let page_count = if count > 0 { (total + count - 1) / count } else { 1 };

    let mut index_file = load_index_file(app, &uid);
    let key = folder_id.to_string();
    let old = index_file.remove(&key).unwrap_or_default();
    let folder_name = folder_name_of(&first_page.folder_list, folder_id)
        .or_else(|| Some(old.folder_name.clone()))
        .unwrap_or_else(|| format!("收藏夹 {folder_id}"));

    let mut known: HashSet<i64> = old.comics.iter().map(|comic| comic.id).collect();
    // 扫到的旧条目：用来对齐旧索引，算出哪些已经不在这层里了
    let mut seen_old_ids: Vec<i64> = Vec::new();
    // 扫到的旧条目按 id 存一份，用新的名称 / 作者覆盖
    let mut refreshed: HashMap<i64, FavoritesIndexComic> = HashMap::new();
    let mut added_comics: Vec<FavoritesIndexComic> = Vec::new();

    let mut page: i64 = 1;
    let mut page_list = first_page.list;
    loop {
        let mut page_added = 0_usize;
        for comic in page_list {
            let Some(item) = index_comic(comic) else {
                continue;
            };
            if known.insert(item.id) {
                added_comics.push(item);
                page_added += 1;
            } else {
                seen_old_ids.push(item.id);
                refreshed.insert(item.id, item);
            }
        }

        // 整页没有新 id 就停：再往后只会更旧
        if page_added == 0 || page >= page_count {
            break;
        }
        page += 1;
        page_list = jm_client
            .get_favorite_folder(folder_id, page, FavoriteSort::FavoriteTime)
            .await?
            .list;
    }

    // 接口说这层一本都没有：旧索引里剩的都是过期条目
    let removed_ids = if total == 0 {
        old.comics.iter().map(|comic| comic.id).collect()
    } else {
        removed_old_ids(&old.comics, &seen_old_ids)
    };
    let added = added_comics.len();
    let removed = removed_ids.len();

    // 新收藏在前，旧条目保持原顺序（已被取消收藏的那几条顺手删掉）
    let mut comics = added_comics;
    for comic in old.comics {
        if removed_ids.contains(&comic.id) {
            continue;
        }
        match refreshed.remove(&comic.id) {
            Some(item) => comics.push(item),
            None => comics.push(comic),
        }
    }

    let total = comics.len();
    index_file.insert(
        key,
        FavoritesFolderIndex {
            folder_name,
            updated_at: now_iso8601(),
            total,
            comics,
        },
    );
    save_index_file(app, &uid, &index_file)?;

    Ok(UpdateFavoritesIndexResult {
        added,
        removed,
        total,
    })
}

/// 从该账号所有收藏夹层里删掉这本漫画（本地取消收藏后调用）
pub async fn remove_comic(app: &AppHandle, comic_id: i64) -> eyre::Result<()> {
    let uid = account_uid(app).await?;
    let mut index_file = load_index_file(app, &uid);

    let mut changed = false;
    for index in index_file.values_mut() {
        let before = index.comics.len();
        index.comics.retain(|comic| comic.id != comic_id);
        if index.comics.len() != before {
            index.total = index.comics.len();
            index.updated_at = now_iso8601();
            changed = true;
        }
    }

    // 一条都没动就不落盘，也不因为取消收藏凭空建出索引文件
    if !changed {
        return Ok(());
    }
    save_index_file(app, &uid, &index_file)
}

/// 解析当前账号 uid：缓存 → 收藏夹列表 → 用户信息（一个自建收藏夹都没有时列表是空的）
async fn account_uid(app: &AppHandle) -> eyre::Result<String> {
    if let Some(uid) = ACCOUNT_UID.read().clone() {
        return Ok(uid);
    }

    let jm_client = app.get_jm_client();
    let favorites = jm_client
        .get_favorite_folder(0, 1, FavoriteSort::FavoriteTime)
        .await?;
    remember_account_uid(&favorites.folder_list);

    if let Some(uid) = ACCOUNT_UID.read().clone() {
        return Ok(uid);
    }

    let uid = jm_client.get_user_profile().await?.uid;
    if uid.is_empty() {
        return Err(eyre!("没拿到当前账号的uid"));
    }
    *ACCOUNT_UID.write() = Some(uid.clone());
    Ok(uid)
}

/// 本地索引目录：app_data_dir/收藏索引（不存在就建）；漫画标签缓存也放这里
pub(crate) fn index_dir(app: &AppHandle) -> eyre::Result<PathBuf> {
    let dir = app.path().app_data_dir()?.join("收藏索引");
    if !dir.exists() {
        std::fs::create_dir_all(&dir)
            .wrap_err_with(|| format!("创建索引目录失败: {}", dir.display()))?;
    }

    Ok(dir)
}

/// uid 只进文件名，过滤掉可能的路径分隔符
pub(crate) fn sanitize_uid(uid: &str) -> eyre::Result<String> {
    let uid: String = uid.chars().filter(char::is_ascii_alphanumeric).collect();
    if uid.is_empty() {
        return Err(eyre!("账号uid不合法"));
    }
    Ok(uid)
}

/// 索引文件：app_data_dir/收藏索引/favorites-index-<uid>.json（一个账号一份）
fn index_path(app: &AppHandle, uid: &str) -> eyre::Result<PathBuf> {
    Ok(index_dir(app)?.join(format!("favorites-index-{}.json", sanitize_uid(uid)?)))
}

/// 读索引文件：文件不存在、读不出来、解析失败都返回空索引
fn load_index_file(app: &AppHandle, uid: &str) -> IndexFile {
    let Ok(path) = index_path(app, uid) else {
        return IndexFile::new();
    };
    if !path.exists() {
        return IndexFile::new();
    }

    let Ok(text) = std::fs::read_to_string(&path) else {
        tracing::error!(path = %path.display(), "收藏夹索引文件读取失败，按没有索引处理");
        return IndexFile::new();
    };

    match serde_json::from_str(&text) {
        Ok(index_file) => index_file,
        Err(err) => {
            tracing::error!(message = %err, path = %path.display(), "收藏夹索引文件解析失败，按没有索引处理");
            IndexFile::new()
        }
    }
}

/// 写索引文件：失败由调用方记日志并返回错误
fn save_index_file(app: &AppHandle, uid: &str, index_file: &IndexFile) -> eyre::Result<()> {
    let path = index_path(app, uid)?;
    let text = serde_json::to_string_pretty(index_file)?;
    std::fs::write(&path, text)
        .wrap_err_with(|| format!("写入收藏夹索引文件失败: {}", path.display()))?;
    Ok(())
}

/// 并发 5 翻页取 [from, to]，结果按页码拼好
async fn fetch_pages(
    jm_client: &JmClient,
    folder_id: i64,
    from: i64,
    to: i64,
) -> eyre::Result<Vec<ComicInFavoriteRespData>> {
    let sem = Arc::new(Semaphore::new(PAGE_CONCURRENCY));
    let mut join_set = JoinSet::new();
    for page in from..=to {
        let jm_client = jm_client.clone();
        let sem = Arc::clone(&sem);
        join_set.spawn(async move {
            let _permit = sem.acquire().await?;
            let resp = jm_client
                .get_favorite_folder(folder_id, page, FavoriteSort::FavoriteTime)
                .await?;
            Ok::<(i64, Vec<ComicInFavoriteRespData>), eyre::Report>((page, resp.list))
        });
    }

    let mut pages = Vec::new();
    while let Some(result) = join_set.join_next().await {
        let (page, list) = result.map_err(eyre::Report::from)??;
        pages.push((page, list));
    }
    pages.sort_by_key(|(page, _)| *page);

    Ok(pages.into_iter().flat_map(|(_, list)| list).collect())
}

/// 把扫到的旧条目按顺序对回旧索引：中间被跳过的那些已经不在这层收藏夹里了。
/// 一旦对不上（旧条目又被重新收藏到了最前），分不清「移除」还是「前移」，返回空表示这次不删。
fn removed_old_ids(old_comics: &[FavoritesIndexComic], seen_old_ids: &[i64]) -> Vec<i64> {
    let mut removed = Vec::new();
    let mut cursor = 0_usize;
    for id in seen_old_ids {
        let Some(offset) = old_comics[cursor..]
            .iter()
            .position(|comic| comic.id == *id)
        else {
            return Vec::new();
        };
        removed.extend(old_comics[cursor..cursor + offset].iter().map(|comic| comic.id));
        cursor += offset + 1;
    }
    removed
}

/// 收藏夹名：列表里按 FID 找；folder_id=0 是「全部收藏」，它不在列表里
fn folder_name_of(folder_list: &[FavoriteFolderRespData], folder_id: i64) -> Option<String> {
    let fid = folder_id.to_string();
    if let Some(folder) = folder_list.iter().find(|folder| folder.fid == fid) {
        return Some(folder.name.clone());
    }
    (folder_id == 0).then(|| "全部收藏".to_string())
}

/// 索引里只需要这三项；id 解析不出来就跳过这本
fn index_comic(comic: ComicInFavoriteRespData) -> Option<FavoritesIndexComic> {
    let id = comic.id.parse::<i64>().ok()?;
    Some(FavoritesIndexComic {
        id,
        name: comic.name,
        author: comic.author,
    })
}

/// 当前时间 ISO8601（UTC），如 2025-01-02T03:04:05Z
fn now_iso8601() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    let (days, rest) = (seconds / 86_400, seconds % 86_400);
    let (year, month, day) = civil_from_days(days as i64);
    let (hour, minute, second) = (rest / 3600, (rest % 3600) / 60, rest % 60);
    format!("{year:04}-{month:02}-{day:02}T{hour:02}:{minute:02}:{second:02}Z")
}

/// Unix 天数 -> (年, 月, 日)，Howard Hinnant 的 civil_from_days
fn civil_from_days(days: i64) -> (i64, u64, u64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe as i64 + era * 400 + i64::from(month <= 2);
    (year, month, day)
}
