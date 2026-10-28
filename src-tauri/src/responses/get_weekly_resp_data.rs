use serde::{Deserialize, Serialize};
use specta::Type;

use crate::{
    responses::string_to_i64,
    types::{Category, CategorySub},
};

/// 时间戳有的接口给字符串、有的给数字：两种都收下
fn string_or_i64<'de, D>(deserializer: D) -> Result<i64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(match value {
        Some(serde_json::Value::Number(number)) => number.as_i64().unwrap_or_default(),
        Some(serde_json::Value::String(text)) => text.trim().parse::<i64>().unwrap_or_default(),
        _ => 0,
    })
}

/// 接口里这些字段偶尔给 null：统一当成空串，别让整页解析失败
fn null_to_string<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Option::<String>::deserialize(deserializer)?.unwrap_or_default())
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct GetWeeklyRespData {
    /// 和 id / update_at 一样：接口偶尔把数字写成字符串；缺字段时当 0
    #[serde(default, deserialize_with = "string_to_i64")]
    pub total: i64,
    pub list: Vec<ComicInWeeklyRespData>,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct ComicInWeeklyRespData {
    #[serde(deserialize_with = "string_to_i64")]
    pub id: i64,
    #[serde(default, deserialize_with = "null_to_string")]
    pub author: String,
    #[serde(default, deserialize_with = "null_to_string")]
    pub description: String,
    pub name: String,
    pub image: String,
    pub category: Category,
    pub category_sub: CategorySub,
    #[serde(default)]
    pub liked: bool,
    /// 每周连载更新接口用的是 favorite，每周必看用 is_favorite
    #[serde(alias = "favorite", default)]
    pub is_favorite: bool,
    #[serde(default, deserialize_with = "string_or_i64")]
    pub update_at: i64,
}
