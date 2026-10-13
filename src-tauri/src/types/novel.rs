use serde::{Deserialize, Deserializer, Serialize};
use specta::Type;

use crate::responses::string_to_i64;

// 字段名一律与官方 JSON 的 snake_case 对齐，
// 只有 `NCID`、`new` 这种不是合法 Rust 字段名的才用 rename 显式映射。

/// 小说列表里的一本（/novels、/search_novels 的 list 元素）
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct NovelMeta {
    #[serde(deserialize_with = "string_to_i64")]
    pub id: i64,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub image: String,
    #[serde(default)]
    pub liked: bool,
    /// 未登录时官方给的是 null，所以这里保留三态
    #[serde(default)]
    pub is_favorite: Option<bool>,
    #[serde(default, deserialize_with = "string_to_i64")]
    pub update_at: i64,
    #[serde(default, deserialize_with = "string_to_i64")]
    pub likes: i64,
    #[serde(default, deserialize_with = "string_to_i64")]
    pub last_chapter_index: i64,
    #[serde(default)]
    pub last_chapter_title: String,
}

/// 一页小说（/novels 与 /search_novels 的完整返回）
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct NovelPage {
    #[serde(default)]
    pub list: Vec<NovelMeta>,
    #[serde(deserialize_with = "string_to_i64")]
    pub total: i64,
}

/// 小说详情里的一个章节条目（官方 series 数组的元素）
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct NovelChapterMeta {
    /// 官方写作 `NCID`
    #[serde(rename = "NCID", default)]
    pub ncid: String,
    #[serde(default)]
    pub title: String,
    #[serde(default, deserialize_with = "string_to_i64")]
    pub sort: i64,
    #[serde(default)]
    pub created_at: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub on_at: String,
    #[serde(default)]
    pub buy_nc: String,
    /// "1" 表示需要购买
    #[serde(default)]
    pub is_need_buy_nc: String,
    /// "1" 表示需要登录
    #[serde(default)]
    pub is_need_login: String,
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    /// 官方写作 `new`
    #[serde(rename = "new", default)]
    pub is_new: bool,
    #[serde(default)]
    pub purchased: bool,
}

/// 小说详情（/novel 的返回）
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct NovelDetail {
    #[serde(deserialize_with = "string_to_i64")]
    pub id: i64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub tags: Vec<String>,
    /// "1" 表示已完结
    #[serde(default)]
    pub is_end: String,
    #[serde(default)]
    pub serial_status: String,
    #[serde(default, deserialize_with = "string_to_i64")]
    pub total_views: i64,
    #[serde(default, deserialize_with = "string_to_i64")]
    pub likes: i64,
    #[serde(default)]
    pub is_favorite: bool,
    /// 官方有时给条数、有时直接给评论数组，这里统一折成条数
    #[serde(default, deserialize_with = "count_or_len_to_i64")]
    pub comment_total: i64,
    #[serde(default)]
    pub series: Vec<NovelChapterMeta>,
}

/// 小说章节正文（/novelchapters 的返回）
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct NovelChapter {
    #[serde(deserialize_with = "string_to_i64")]
    pub ncid: i64,
    #[serde(deserialize_with = "string_to_i64")]
    pub nid: i64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub title: String,
    /// 正文 HTML，原样返回给前端渲染，后端不剥标签
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub last_chapter_title: String,
    #[serde(default, deserialize_with = "string_to_i64")]
    pub total_favorites: i64,
    #[serde(default, deserialize_with = "string_to_i64")]
    pub total_views: i64,
    #[serde(default, deserialize_with = "string_to_i64")]
    pub total_likes: i64,
    #[serde(default)]
    pub is_favorite: bool,
    #[serde(default)]
    pub liked: bool,
}

/// 数字、数字字符串、数组（取长度）都能折成 i64
fn count_or_len_to_i64<'de, D>(deserializer: D) -> Result<i64, D::Error>
where
    D: Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;

    Ok(match value {
        serde_json::Value::Array(items) => i64::try_from(items.len()).unwrap_or(i64::MAX),
        serde_json::Value::Number(number) => number.as_i64().unwrap_or(0),
        serde_json::Value::String(text) => text.parse().unwrap_or(0),
        _ => 0,
    })
}

