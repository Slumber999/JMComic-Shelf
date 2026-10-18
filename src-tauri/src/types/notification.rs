use serde::{Deserialize, Deserializer, Serialize};
use specta::Type;

use crate::responses::string_to_i64;

/// 一条官方通知（GET /notifications 的 list 元素）
/// - `read` 是官方已读标记：false 由界面亮红点，点「已读」后调 mark_notification_read
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct NotificationItem {
    #[serde(deserialize_with = "string_to_i64")]
    pub id: i64,
    /// 官方给的类型标识（如 system），界面暂不用，留着排查
    #[serde(rename = "type", default)]
    pub type_field: String,
    /// 官方已读标记：false 表示未读
    #[serde(default, deserialize_with = "bool_or_false")]
    pub read: bool,
    #[serde(default)]
    pub title: String,
    /// 正文：站点通知是 HTML 字符串，追更通知可能给字符串数组，这里统一折成文本
    #[serde(default, deserialize_with = "text_or_empty")]
    pub content: String,
    /// 官方给的是 date（形如 "2026-04-26 12:00:00"），兼容 time 和数字时间戳
    #[serde(default, alias = "time", deserialize_with = "text_or_empty")]
    pub date: String,
}

/// 通知列表：官方可能返回裸数组，也可能返回 {total, list}，由 get_notifications 统一成这个结构
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct NotificationPage {
    #[serde(default, deserialize_with = "string_to_i64")]
    pub total: i64,
    #[serde(default)]
    pub list: Vec<NotificationItem>,
}

/// 兼容字符串 / 数字 / null / 字符串数组（追更通知的 content 就是数组）
fn text_or_empty<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(match value {
        serde_json::Value::String(text) => text,
        serde_json::Value::Null => String::new(),
        serde_json::Value::Array(items) => items
            .into_iter()
            .map(|item| match item {
                serde_json::Value::String(text) => text,
                other => other.to_string(),
            })
            .collect::<Vec<_>>()
            .join("\n"),
        other => other.to_string(),
    })
}

/// read 官方给布尔；个别记录给 0/1 或 "1"/"true"，这里都认
fn bool_or_false<'de, D>(deserializer: D) -> Result<bool, D::Error>
where
    D: Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(match value {
        serde_json::Value::Bool(flag) => flag,
        serde_json::Value::Number(number) => number.as_i64().unwrap_or(0) != 0,
        serde_json::Value::String(text) => text == "1" || text.eq_ignore_ascii_case("true"),
        _ => false,
    })
}
