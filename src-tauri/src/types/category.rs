use serde::{Deserialize, Serialize};
use specta::Type;

/// 官方分类树里的一个主分类
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CategoryNode {
    pub id: i64,
    pub name: String,
    pub slug: String,
    /// 该分类下的作品总数
    pub total_albums: i64,
    pub sub_categories: Vec<SubCategoryNode>,
}

/// 分类树里的子分类
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct SubCategoryNode {
    pub cid: i64,
    pub name: String,
    pub slug: String,
}

/// 官方「常用标签」分组（/categories 的 blocks 字段）
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TagBlock {
    pub title: String,
    pub content: Vec<String>,
}

/// /categories 的完整返回：分类树 + 常用标签分组
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CategoryResp {
    pub categories: Vec<CategoryNode>,
    pub blocks: Vec<TagBlock>,
}
