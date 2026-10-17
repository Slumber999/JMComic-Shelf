use serde::{Deserialize, Deserializer, Serialize};
use specta::Type;

use crate::responses::string_to_i64;

/// 一条漫画评论（官方 App 接口 /forum 的返回）
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct Comment {
    /// 评论 id
    #[serde(rename = "CID")]
    pub cid: String,
    /// 所属漫画 id（全站评论靠它定位来源）；小说的评论这里给的是 null
    #[serde(rename = "AID", default, deserialize_with = "null_to_string")]
    pub aid: String,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub nickname: String,
    /// 正文：后端已去掉 HTML 标签，只留文字
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub likes: String,
    /// 形如 "Aug 30, 2026"
    #[serde(default)]
    pub addtime: String,
    /// 父评论 id，"0" 表示主评论；小说的评论这里是 null
    #[serde(rename = "parent_CID", default, deserialize_with = "null_to_string")]
    pub parent_cid: String,
    /// "1" 表示剧透
    #[serde(default)]
    pub spoiler: String,
    /// 回复：官方 JSON 用的键是 replys，可能缺省、为 null，或只给单个对象
    #[serde(rename = "replys", default, deserialize_with = "replies_or_empty")]
    pub replies: Vec<Comment>,
}

impl Comment {
    /// 评论正文是 HTML，去掉标签、还原常见实体
    pub fn strip_html(&mut self) {
        self.content = html_to_text(&self.content);
        for reply in &mut self.replies {
            reply.strip_html();
        }
    }
}

/// 一页评论
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct CommentPage {
    #[serde(default, deserialize_with = "null_to_empty")]
    pub list: Vec<Comment>,
    #[serde(default, deserialize_with = "string_to_i64")]
    pub total: i64,
}

impl CommentPage {
    /// 把每条评论的 HTML 正文转成纯文本
    pub fn strip_html(&mut self) {
        for comment in &mut self.list {
            comment.strip_html();
        }
    }
}

fn html_to_text(input: &str) -> String {
    // <br> 之类的换行先还原，再整体去掉标签
    let with_breaks = input
        .replace("<br>", "\n")
        .replace("<br/>", "\n")
        .replace("<br />", "\n");

    let mut text = String::with_capacity(with_breaks.len());
    let mut in_tag = false;
    for ch in with_breaks.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => text.push(ch),
            _ => {}
        }
    }

    text.replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_to_text_strips_tags_and_entities() {
        let html = "<div style='color:red'>你好 &amp; 再见</div><br>第二行&nbsp;结束";
        assert_eq!(html_to_text(html), "你好 & 再见\n第二行 结束");
    }

    #[test]
    fn comment_page_parses_api_json() {
        // 官方 /forum 返回的字段形态：id 都是字符串，total 是数字
        let json = r#"{
            "total": 42,
            "list": [
                {
                    "CID": "10991925",
                    "AID": "456688",
                    "username": "test",
                    "nickname": "test",
                    "content": "<div>正文</div>",
                    "likes": "0",
                    "addtime": "Aug 30, 2026",
                    "parent_CID": "0",
                    "spoiler": "1",
                    "gender": "null",
                    "replys": null
                }
            ]
        }"#;

        let mut page = serde_json::from_str::<CommentPage>(json).expect("解析评论失败");
        page.strip_html();

        assert_eq!(page.total, 42);
        assert_eq!(page.list.len(), 1);
        assert_eq!(page.list[0].cid, "10991925");
        assert_eq!(page.list[0].content, "正文");
        assert_eq!(page.list[0].spoiler, "1");
        assert!(page.list[0].replies.is_empty());

        // 小说的评论（mode=novel）里 AID / parent_CID 都是 null，应折成空串
        let novel_json = r#"{
            "total": "4",
            "list": [
                {
                    "AID": null,
                    "CID": "949",
                    "NID": "94",
                    "username": "gouging",
                    "nickname": "",
                    "content": "<div>正文</div>",
                    "likes": "0",
                    "addtime": "Sep 10, 2026",
                    "parent_CID": null,
                    "spoiler": "1"
                }
            ]
        }"#;

        let novel_page = serde_json::from_str::<CommentPage>(novel_json).expect("解析小说评论失败");
        assert_eq!(novel_page.total, 4);
        assert_eq!(novel_page.list[0].cid, "949");
        assert!(novel_page.list[0].aid.is_empty());
        assert!(novel_page.list[0].parent_cid.is_empty());
    }

    #[test]
    fn comment_page_parses_nested_replys() {
        // 真实 /forum 返回：主评论的 replys 是数组，里面每条回复自己可能还有 replys；
        // 回复对象不带 AID，只有一个回复时官方偶尔直接给对象而不是数组
        let json = r#"{
            "total": 272,
            "list": [
                {
                    "CID": "11037661",
                    "AID": "1044827",
                    "username": "a",
                    "nickname": "甲",
                    "content": "<div>主评论</div>",
                    "parent_CID": "0",
                    "replys": [
                        {
                            "CID": "11067784",
                            "username": "b",
                            "nickname": "乙",
                            "content": "<div>回复 &amp; 二级</div>",
                            "parent_CID": "11037661",
                            "replys": [
                                {
                                    "CID": "11067799",
                                    "username": "c",
                                    "nickname": "丙",
                                    "content": "<div>回复的回复</div>",
                                    "parent_CID": "11067784"
                                }
                            ]
                        }
                    ]
                },
                {
                    "CID": "11037641",
                    "AID": "1044827",
                    "username": "d",
                    "nickname": "丁",
                    "content": "<div>另一条</div>",
                    "parent_CID": "0",
                    "replys": {
                        "CID": "11037820",
                        "username": "e",
                        "nickname": "戊",
                        "content": "<div>单个回复</div>",
                        "parent_CID": "11037641"
                    }
                }
            ]
        }"#;

        let mut page = serde_json::from_str::<CommentPage>(json).expect("解析多级评论失败");
        page.strip_html();

        assert_eq!(page.list[0].replies.len(), 1);
        assert_eq!(page.list[0].replies[0].cid, "11067784");
        assert_eq!(page.list[0].replies[0].content, "回复 & 二级");
        assert_eq!(page.list[0].replies[0].parent_cid, "11037661");
        assert_eq!(page.list[0].replies[0].replies[0].content, "回复的回复");
        assert_eq!(page.list[1].replies.len(), 1);
        assert_eq!(page.list[1].replies[0].content, "单个回复");
    }
}

/// 兼容字符串字段给 null 的情况（小说的评论里 AID / parent_CID 都是 null）
fn null_to_string<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Option::<String>::deserialize(deserializer)?.unwrap_or_default())
}

/// 兼容 replys 缺失 / 为 null / 只给单个对象（不是数组）三种形态
fn replies_or_empty<'de, D>(deserializer: D) -> Result<Vec<Comment>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Replies {
        Many(Vec<Comment>),
        One(Comment),
    }

    Ok(match Option::<Replies>::deserialize(deserializer)? {
        Some(Replies::Many(list)) => list,
        Some(Replies::One(one)) => vec![one],
        None => Vec::new(),
    })
}

/// 兼容字段缺失或为 null 的数组
fn null_to_empty<'de, D, T>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Ok(Option::<Vec<T>>::deserialize(deserializer)?.unwrap_or_default())
}
