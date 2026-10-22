use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use eyre::OptionExt;
use tauri::AppHandle;
use tracing::instrument;

use crate::{
    extensions::AppHandleExt,
    local_index,
    types::Comic,
};

pub fn filename_filter(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '\\' | '/' | '\n' => ' ',
            ':' => '：',
            '*' => '⭐',
            '?' => '？',
            '"' => '\'',
            '<' => '《',
            '>' => '》',
            '|' => '丨',
            _ => c,
        })
        .collect::<String>()
        .trim()
        .trim_end_matches('.')
        .trim()
        .to_string()
}

// 计算MD5哈希并返回十六进制字符串
pub fn md5_hex(data: &str) -> String {
    format!("{:x}", md5::compute(data))
}

/// 接口分页的总页数：`per_page` 是每页条数（接口可能返回 0，这时按只有一页算）
/// 收藏夹的三处翻页（全部收藏 / 收藏夹索引）共用这一份
pub fn page_count(total: i64, per_page: i64) -> i64 {
    if per_page > 0 {
        (total + per_page - 1) / per_page
    } else {
        1
    }
}

/// 路径的目录名（最后一段）；拿不到就是错误，文案对上层保持不变
pub fn path_dir_name(path: &Path) -> eyre::Result<String> {
    let name = path
        .file_name()
        .ok_or_eyre(format!("获取`{}`的目录名失败", path.display()))?;
    Ok(name.to_string_lossy().to_string())
}

/// 漫画ID -> 漫画下载目录
/// - 走 `local_index` 的缓存快照：这个函数被每次搜索 / 收藏翻页 / 每周必看 / 打开章节调用，
///   以前每次都全量扫盘并把每个元数据解析成 `serde_json::Value`，库大了会很慢
/// - 保留返回 `Result` 是为了不改动已有的调用点
#[instrument(level = "error", skip_all)]
pub fn create_id_to_dir_map(app: &AppHandle) -> eyre::Result<HashMap<i64, PathBuf>> {
    Ok(local_index::id_to_dir_map(app))
}

#[instrument(level = "error", skip_all, fields(aid = aid))]
pub async fn get_comic(app: AppHandle, aid: i64) -> eyre::Result<Comic> {
    let jm_client = app.get_jm_client();

    let comic_resp_data = jm_client.get_comic(aid).await?;

    let comic = Comic::from_comic_resp_data(&app, comic_resp_data)?;

    Ok(comic)
}
