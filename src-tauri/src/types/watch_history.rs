use serde::{Deserialize, Serialize};
use specta::Type;

use crate::responses::{string_to_i64, CategoryRespData, CategorySubRespData};

/// 浏览记录里的一条（GET /watch_list 的 list 元素）
/// - id 就是漫画 id，删除时也用它当记录 id
/// - image 官方常常给空串，前端拿不到就自己按 id 取封面
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct WatchHistoryItem {
    #[serde(deserialize_with = "string_to_i64")]
    pub id: i64,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub image: String,
    #[serde(default)]
    pub category: CategoryRespData,
    #[serde(default)]
    pub category_sub: CategorySubRespData,
}

/// 一页浏览记录（GET /watch_list 的完整返回）
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct WatchHistoryPage {
    #[serde(default)]
    pub list: Vec<WatchHistoryItem>,
    #[serde(default, deserialize_with = "string_to_i64")]
    pub total: i64,
}

/// 删除浏览记录的返回体：{"status":1,"msg":"刪除成功"}
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct WatchHistoryDeleteResp {
    #[serde(default)]
    pub status: i64,
    #[serde(default)]
    pub msg: String,
}
