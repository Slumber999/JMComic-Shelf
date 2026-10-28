use eyre::{eyre, OptionExt};
use tauri::{AppHandle, Manager};
use tracing::instrument;

use crate::errors::{CommandError, CommandResult};
use crate::extensions::AppHandleExt;
use crate::local_index;
use crate::reader::{ReaderComic, ReaderState};
use crate::responses::GetWeeklyInfoRespData;
use crate::types::{
    CategoryResp, Comic, ComicInFavorite, ComicInSearch, ComicInWeekly, GetWeeklyResult,
    NovelChapter, NovelDetail, NovelPage, SearchResult, SearchResultVariant, SearchSort,
};
use crate::utils;
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

/// 漫画评论（只读）：aid 传漫画 id 就是整本评论，不传就是全站最新评论；
/// chapter_id 传某一话的章节 id 时取这一话的评论（官方评论按章节归档）
#[tauri::command]
#[specta::specta]
#[instrument(level = "error", skip_all, fields(aid = ?aid, page = page, chapter_id = ?chapter_id))]
pub async fn get_comments(
    app: AppHandle,
    aid: Option<i64>,
    page: i64,
    chapter_id: Option<i64>,
) -> CommandResult<crate::types::CommentPage> {
    let jm_client = app.get_jm_client();

    jm_client
        .get_comments(aid, page, chapter_id)
        .await
        .map_err(|err| CommandError::from("获取评论失败", err))
}

/// 小说评论（只读）：官方 /forum 走 mode=novel + nid，返回结构和漫画评论一致
#[tauri::command]
#[specta::specta]
#[instrument(level = "error", skip_all, fields(nid = nid, page = page))]
pub async fn get_novel_comments(
    app: AppHandle,
    nid: i64,
    page: i64,
) -> CommandResult<crate::types::CommentPage> {
    let jm_client = app.get_jm_client();

    jm_client
        .get_novel_comments(nid, page)
        .await
        .map_err(|err| CommandError::from("获取小说评论失败", err))
}

/// 小说列表：`o` 是排序（"mr" = 最近更新），`page` 从 1 开始
#[tauri::command]
#[specta::specta]
#[instrument(level = "error", skip_all, fields(order = o, page = page))]
pub async fn get_novel_list(app: AppHandle, o: String, page: i64) -> CommandResult<NovelPage> {
    let jm_client = app.get_jm_client();

    jm_client
        .get_novel_list(&o, page)
        .await
        .map_err(|err| CommandError::from("获取小说列表失败", err))
}

#[tauri::command]
#[specta::specta]
#[instrument(level = "error", skip_all, fields(nid = nid))]
pub async fn get_novel_detail(app: AppHandle, nid: String) -> CommandResult<NovelDetail> {
    let jm_client = app.get_jm_client();

    jm_client
        .get_novel_detail(&nid)
        .await
        .map_err(|err| CommandError::from("获取小说详情失败", err))
}

/// 小说章节正文：`content` 是 HTML，后端不剥标签，原样返回给前端
#[tauri::command]
#[specta::specta]
#[instrument(level = "error", skip_all, fields(ncid = ncid, lang = lang))]
pub async fn get_novel_chapter(
    app: AppHandle,
    ncid: String,
    lang: String,
) -> CommandResult<NovelChapter> {
    let jm_client = app.get_jm_client();

    jm_client
        .get_novel_chapter(&ncid, &lang)
        .await
        .map_err(|err| CommandError::from("获取小说章节失败", err))
}

#[tauri::command]
#[specta::specta]
#[instrument(level = "error", skip_all, fields(search_query = query))]
pub async fn search_novels(app: AppHandle, query: String) -> CommandResult<NovelPage> {
    let jm_client = app.get_jm_client();

    jm_client
        .search_novels(&query)
        .await
        .map_err(|err| CommandError::from("搜索小说失败", err))
}

#[tauri::command]
#[specta::specta]
#[instrument(level = "error", skip_all, fields(aid = aid))]
pub async fn get_comic(app: AppHandle, aid: i64) -> CommandResult<Comic> {
    // 点「去详情」的入口：进入/完成时间 + 线程名，卡顿时对着封面时间线看谁在等谁
    let entered = crate::measure::enter("get_comic", &format!("aid={aid}"));
    let result = get_comic_inner(app, aid).await;
    crate::measure::finish("get_comic", entered, &format!("aid={aid}"));
    result
}

async fn get_comic_inner(app: AppHandle, aid: i64) -> CommandResult<Comic> {
    let comic = utils::get_comic(app.clone(), aid)
        .await
        .map_err(|err| CommandError::from("获取漫画信息失败", err))?;

    Ok(comic)
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

#[tauri::command(async)]
#[specta::specta]
#[instrument(
    level = "error",
    skip_all,
    fields(type_id = type_id, date = date, page = page)
)]
pub async fn get_serialization(
    app: AppHandle,
    type_id: String,
    date: i64,
    page: i64,
) -> CommandResult<GetWeeklyResult> {
    let jm_client = app.get_jm_client();

    let resp_data = jm_client
        .get_serialization(&type_id, date, page)
        .await
        .map_err(|err| CommandError::from("获取连载更新失败", err))?;

    let result = GetWeeklyResult::from_resp_data(&app, resp_data)
        .map_err(|err| CommandError::from("获取连载更新失败", err))?;

    Ok(result)
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

/// 本地优先的章节登记：下载目录 → 导出目录；都没有（或本地读不出来）返回 None，由调用方走在线
fn open_local_reader(app: &AppHandle, comic_id: i64) -> Option<ReaderComic> {
    let candidates = [
        local_index::comic_dir_and_name(app, comic_id),
        local_index::export_comic_dir_and_name(app, comic_id),
    ];
    let state = app.state::<ReaderState>();
    candidates
        .into_iter()
        .flatten()
        .find_map(|(comic_dir, comic_title)| {
            crate::reader::open_reader(&state, &comic_title, &comic_dir).ok()
        })
}

/// 按漫画ID打开阅读器：本地有就读本地，没有就走在线阅读
#[tauri::command(async)]
#[specta::specta]
#[instrument(level = "error", skip_all, fields(comic_id = comic_id))]
pub async fn open_reader_by_id(app: AppHandle, comic_id: i64) -> CommandResult<ReaderComic> {
    // 点「阅读」的入口：进入/完成时间 + 线程名，卡顿时对着封面时间线看谁在等谁
    let entered = crate::measure::enter("open_reader_by_id", &format!("comic_id={comic_id}"));
    let result = open_reader_by_id_inner(app, comic_id).await;
    crate::measure::finish(
        "open_reader_by_id",
        entered,
        &format!("comic_id={comic_id}"),
    );
    result
}

async fn open_reader_by_id_inner(app: AppHandle, comic_id: i64) -> CommandResult<ReaderComic> {
    // 先查本地索引：已经下载 / 导出过的漫画直接开，不用先请求一次接口
    // （以前只查下载目录，只导出过 cbz 的漫画会白跑一次接口，离线时直接打不开）
    if let Some(reader_comic) = open_local_reader(&app, comic_id) {
        tracing::debug!("阅读器打开成功(本地)，章节数={}", reader_comic.chapters.len());
        return Ok(reader_comic);
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
    // 打开章节要等图片地址 + 预热第一页：这里同样记进入/完成与线程
    let entered = crate::measure::enter("prepare_reader_chapter", &format!("token={token}"));
    let state = app.state::<ReaderState>();
    let result = crate::reader::prepare_chapter(&app, &state, token)
        .await
        .map_err(|err| CommandError::from("加载章节失败", err));
    crate::measure::finish(
        "prepare_reader_chapter",
        entered,
        &format!("token={token}"),
    );
    result
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
    let exported_ids = local_index::exported_id_set(&app);

    comic.update_fields(&id_to_dir_map, &exported_ids);

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
