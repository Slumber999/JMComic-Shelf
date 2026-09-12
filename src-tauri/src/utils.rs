use std::{collections::HashMap, path::PathBuf};

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
