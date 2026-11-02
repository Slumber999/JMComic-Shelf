use serde::{Deserialize, Serialize};
use specta::Type;

use super::{string_to_i64, CategoryRespData, CategorySubRespData, GetComicRespData};

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RedirectRespData {
    #[serde(rename = "search_query")]
    pub search_query: String,
    pub total: i64,
    #[serde(rename = "redirect_aid")]
    pub redirect_aid: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub enum SearchResp {
    SearchRespData(SearchRespData),
    // 用Box包装GetComicRespData，因为GetComicRespData比SearchRespData大得多
    // 如果不用Box包装，即使SearchResp的类型是SearchRespData，也会占用与GetComicRespData一样大的内存
    ComicRespData(Box<GetComicRespData>),
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SearchRespData {
    #[serde(rename = "search_query")]
    pub search_query: String,
    #[serde(deserialize_with = "string_to_i64")]
    pub total: i64,
    pub content: Vec<ComicInSearchRespData>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ComicInSearchRespData {
    pub id: String,
    pub author: String,
    pub name: String,
    pub image: String,
    pub category: CategoryRespData,
    #[serde(rename = "category_sub")]
    pub category_sub: CategorySubRespData,
    pub liked: bool,
    #[serde(rename = "is_favorite")]
    pub is_favorite: bool,
    #[serde(rename = "update_at")]
    pub update_at: i64,
    /// 列表接口偶尔会给标签：字符串数组才用，数字 id 一律丢掉（只用来做屏蔽判断）
    #[serde(default, deserialize_with = "lenient_tags")]
    pub tags: Vec<String>,
}

fn lenient_tags<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<Vec<serde_json::Value>>::deserialize(deserializer)?;
    Ok(value
        .unwrap_or_default()
        .into_iter()
        .filter_map(|item| match item {
            serde_json::Value::String(text) => Some(text),
            _ => None,
        })
        .collect())
}
