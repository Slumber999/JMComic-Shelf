//! 漫画标签缓存：全进程共用一份 comicId -> tags，收藏夹筛选和多关键词搜索都走它
//!
//! 收藏夹列表和搜索结果条目都不带标签（只有 id / 标题 / 作者 / 分类），标签只在 /album 详情里，
//! 所以只能逐本请求；但标签本身不会变，**每本一辈子只取一次**：
//! 内存一份 + app_data_dir/收藏索引/comic-tags.json 一份，不跟账号走（标签是漫画的属性）。
//! 收藏夹的 build / update 绝不碰标签——3000 本收藏就是 3000 次请求，只能按需补。

use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, LazyLock},
};

use eyre::{eyre, WrapErr};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::AppHandle;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;

use crate::extensions::AppHandleExt;
use crate::favorites_index;
use crate::jm_client::JmClient;
use crate::local_index::{self, LocalTag};

/// 补标签的并发数：和搜索页逐本核对标签一致，再高容易被官方限流
const FETCH_CONCURRENCY: usize = 8;
/// 攒够这么多新标签就落一次盘：既不每取一本就重写整个文件，也不把结果只留在内存里
const FLUSH_EVERY: usize = 20;

/// comicId -> 标签，外加「还没落盘多少条」
#[derive(Default)]
struct TagCache {
    tags: HashMap<i64, Vec<String>>,
    pending: usize,
    loaded: bool,
}

static CACHE: LazyLock<RwLock<TagCache>> = LazyLock::new(|| RwLock::new(TagCache::default()));

/// 标签缓存里的一本漫画的标签
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FavoritesComicTags {
    pub id: i64,
    pub tags: Vec<String>,
}

/// 收藏夹的标签快照：标签云（按出现次数倒序）+ 已经取到标签的每本漫画
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FavoritesTagsSnapshot {
    pub tags: Vec<LocalTag>,
    pub comics: Vec<FavoritesComicTags>,
    /// 该收藏夹索引里的漫画总数
    pub total: usize,
    /// 其中已经取到标签的本数
    pub cached: usize,
}

/// 补标签的结果：pending 是这批漫画里这次调用后仍没有标签的本数，前端据此报进度
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FetchComicTagsResult {
    pub fetched: usize,
    pub failed: usize,
    pub pending: usize,
}

/// 缓存文件：app_data_dir/收藏索引/comic-tags.json
fn cache_path(app: &AppHandle) -> eyre::Result<PathBuf> {
    Ok(favorites_index::index_dir(app)?.join("comic-tags.json"))
}

/// 首次用到标签时才把整份缓存读进内存
fn load(app: &AppHandle, cache: &mut TagCache) {
    if cache.loaded {
        return;
    }
    cache.loaded = true;

    let Ok(path) = cache_path(app) else {
        return;
    };
    if !path.exists() {
        return;
    }
    let Ok(text) = std::fs::read_to_string(&path) else {
        tracing::error!(path = %path.display(), "漫画标签缓存读取失败，按没有缓存处理");
        return;
    };

    match serde_json::from_str::<HashMap<i64, Vec<String>>>(&text) {
        Ok(tags) => cache.tags = tags,
        Err(err) => {
            tracing::error!(message = %err, path = %path.display(), "漫画标签缓存解析失败，按没有缓存处理");
        }
    }
}

/// 把内存里的缓存写回磁盘：压缩成一行（几万条标签 pretty 会白涨三成体积）
fn flush(app: &AppHandle, cache: &mut TagCache) -> eyre::Result<()> {
    let path = cache_path(app)?;
    let text = serde_json::to_string(&cache.tags)?;
    std::fs::write(&path, text)
        .wrap_err_with(|| format!("写入漫画标签缓存失败: {}", path.display()))?;
    cache.pending = 0;
    Ok(())
}

/// 攒够一批就落盘；落盘失败只记日志，不影响这次筛选
fn flush_if_needed(app: &AppHandle, cache: &mut TagCache) {
    if cache.pending < FLUSH_EVERY {
        return;
    }
    flush_quietly(app, cache);
}

/// 立刻落盘（一批核对完就存一次，不等攒够）；失败同样只记日志
fn flush_quietly(app: &AppHandle, cache: &mut TagCache) {
    if cache.pending == 0 {
        return;
    }
    if let Err(err) = flush(app, cache) {
        tracing::error!(message = ?err, "漫画标签缓存落盘失败");
    }
}

/// 缓存里有没有这本的标签
pub fn cached(app: &AppHandle, comic_id: i64) -> Option<Vec<String>> {
    let mut cache = CACHE.write();
    load(app, &mut cache);
    cache.tags.get(&comic_id).cloned()
}

/// 记下几本的标签：攒够 FLUSH_EVERY 条落一次盘
pub fn remember(app: &AppHandle, items: &[(i64, Vec<String>)]) {
    if items.is_empty() {
        return;
    }

    let mut cache = CACHE.write();
    load(app, &mut cache);
    for (comic_id, tags) in items {
        cache.tags.insert(*comic_id, tags.clone());
    }
    cache.pending += items.len();
    flush_if_needed(app, &mut cache);
}

/// 取一本的标签：缓存 → 本地元数据（已下载 / 已导出）→ 请求详情
pub async fn tags_of(app: &AppHandle, comic_id: i64) -> eyre::Result<Vec<String>> {
    if let Some(tags) = cached(app, comic_id) {
        return Ok(tags);
    }

    // 本地元数据里就有的（下载过 / 导出过）不用请求
    if let Some(tags) = local_index::comic_tags(app, comic_id) {
        remember(app, &[(comic_id, tags.clone())]);
        return Ok(tags);
    }

    let jm_client = app.get_jm_client().inner().clone();
    let tags = jm_client.get_comic(comic_id).await?.tags;
    remember(app, &[(comic_id, tags.clone())]);
    Ok(tags)
}

/// 给这批漫画补标签：最多请求 limit 本，已经在缓存或本地元数据里的不请求
/// - 单本失败只记数，不废掉整批
/// - 一本都没拿到（多半是没登录 / 断网）直接报错，比留下一半结果好
pub async fn fetch_many(
    app: &AppHandle,
    comic_ids: Vec<i64>,
    limit: usize,
) -> eyre::Result<FetchComicTagsResult> {
    // 先分两拨：本地元数据里就有标签的（零请求），和真要请求详情的
    let mut local: Vec<(i64, Vec<String>)> = Vec::new();
    let mut pending: Vec<i64> = Vec::new();
    for comic_id in comic_ids {
        if cached(app, comic_id).is_some() {
            continue;
        }
        match local_index::comic_tags(app, comic_id) {
            Some(tags) => local.push((comic_id, tags)),
            None => pending.push(comic_id),
        }
    }
    let fetched_from_local = local.len();
    remember(app, &local);

    let attempts = pending.len().min(limit);
    let (mut fetched, mut failed) = (fetched_from_local, 0);
    if attempts > 0 {
        let (tags, failures) = fetch_remote(app, pending[..attempts].to_vec()).await;
        failed = failures;
        fetched += tags.len();
        remember(app, &tags);
    }
    // 这次调用无论取到多少都落一次盘：进度不能只留在内存里
    {
        let mut cache = CACHE.write();
        load(app, &mut cache);
        flush_quietly(app, &mut cache);
    }

    if attempts > 0 && fetched == 0 && failed > 0 {
        return Err(eyre!(
            "这批 {failed} 本漫画的标签一本都没取到，请检查网络连接或登录状态"
        ));
    }

    // 没轮到的 + 请求失败的，都算这次之后仍然缺标签
    let still_missing = pending.len() - attempts + failed;
    Ok(FetchComicTagsResult {
        fetched,
        failed,
        pending: still_missing,
    })
}

/// 并发拉一批详情，只要标签；单本失败不中断整批
async fn fetch_remote(app: &AppHandle, comic_ids: Vec<i64>) -> (Vec<(i64, Vec<String>)>, usize) {
    let jm_client = app.get_jm_client().inner().clone();
    let sem = Arc::new(Semaphore::new(FETCH_CONCURRENCY));
    let mut join_set = JoinSet::new();

    for comic_id in comic_ids {
        let jm_client: JmClient = jm_client.clone();
        let sem = Arc::clone(&sem);
        join_set.spawn(async move {
            let Ok(_permit) = sem.acquire().await else {
                return (comic_id, None);
            };
            let tags = jm_client.get_comic(comic_id).await.ok().map(|comic| comic.tags);
            (comic_id, tags)
        });
    }

    let mut tags = Vec::new();
    let mut failed = 0_usize;
    while let Some(result) = join_set.join_next().await {
        match result {
            Ok((comic_id, Some(comic_tags))) => tags.push((comic_id, comic_tags)),
            Ok((_, None)) | Err(_) => failed += 1,
        }
    }
    (tags, failed)
}

/// 该收藏夹的标签快照：标签云只统计已经取到标签的漫画，没取到的本不参与
pub async fn favorites_snapshot(app: &AppHandle, folder_id: i64) -> FavoritesTagsSnapshot {
    let Some(index) = favorites_index::get(app, folder_id).await else {
        return FavoritesTagsSnapshot::default();
    };
    // 只把该收藏夹用到的标签捞出来：缓存里别的漫画（别的收藏夹 / 搜索页取的）不算进来
    let mut cache = CACHE.write();
    load(app, &mut cache);
    let comics: Vec<FavoritesComicTags> = index
        .comics
        .iter()
        .filter_map(|comic| {
            cache.tags.get(&comic.id).map(|tags| FavoritesComicTags {
                id: comic.id,
                tags: tags.clone(),
            })
        })
        .collect();

    let per_comic: HashMap<i64, Vec<String>> = comics
        .iter()
        .map(|comic| (comic.id, comic.tags.clone()))
        .collect();

    FavoritesTagsSnapshot {
        tags: local_index::count_tags(per_comic),
        total: index.comics.len(),
        cached: comics.len(),
        comics,
    }
}
