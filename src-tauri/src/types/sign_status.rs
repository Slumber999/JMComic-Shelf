use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::responses::string_to_i64;

/// 今日签到状态（官方 /daily 的返回）
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct SignStatus {
    /// 今天是否已签到：官方不返回这个字段，由 record 里今天的日期推出来
    #[serde(default)]
    pub signed_today: bool,
    /// 签到活动 id，执行签到时原样传回
    #[serde(default, deserialize_with = "string_to_i64")]
    pub daily_id: i64,
    /// 三天签到总金币
    #[serde(default, deserialize_with = "string_to_i64")]
    pub three_days_coin: i64,
    /// 三天签到总经验
    #[serde(default, deserialize_with = "string_to_i64")]
    pub three_days_exp: i64,
    /// 七天签到总金币
    #[serde(default, deserialize_with = "string_to_i64")]
    pub seven_days_coin: i64,
    /// 七天签到总经验
    #[serde(default, deserialize_with = "string_to_i64")]
    pub seven_days_exp: i64,
    /// 活动名称，如「10月-「來都來了」」
    #[serde(default)]
    pub event_name: String,
    /// PC 背景图路径（官方没带域名）
    #[serde(default)]
    pub background_pc: String,
    /// 手机背景图路径（官方没带域名）
    #[serde(default)]
    pub background_phone: String,
    /// 当前进度，如 "0%"
    #[serde(rename = "currentProgress", default)]
    pub current_progress: String,
    /// 签到记录：周 × 天，日期是当月第几天
    #[serde(default)]
    pub record: Vec<Vec<SignRecordDay>>,
    /// 本月已补签次数
    #[serde(default, deserialize_with = "string_to_i64")]
    pub makeup_used: i64,
    /// 本月补签上限
    #[serde(default, deserialize_with = "string_to_i64")]
    pub makeup_limit: i64,
    /// 当前是否还能补签
    #[serde(default)]
    pub makeup_available: bool,
}

impl SignStatus {
    /// 官方不给「今天签没签」，只能拿今天去 record 里查
    pub fn resolve_signed_today(&mut self) {
        let today = today_day_of_month();
        self.signed_today = self
            .record
            .iter()
            .flatten()
            .any(|day| day.date == today && day.signed == Some(true));
    }
}

/// 签到记录里的一天
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct SignRecordDay {
    /// 当月第几天，两位数字符串，如 "01"
    #[serde(default)]
    pub date: String,
    /// 是否已签到；null 表示今天或之后（还没签到）
    #[serde(default)]
    pub signed: Option<bool>,
    /// 是否为奖励日
    #[serde(default)]
    pub bonus: bool,
}

/// 今天在当月是第几天，两位数字符串（与官方 record 里的 date 对齐）
/// - 官方按服务端时间（东八区）算日期，这里固定 +8 小时，不看系统时区
fn today_day_of_month() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs());
    let days = (seconds + 8 * 3600) / 86_400;
    format!("{:02}", day_of_month(days))
}

/// 1970-01-01 起的天数 → 当月第几天（Howard Hinnant 的 civil_from_days，输入恒为正）
fn day_of_month(days_since_epoch: u64) -> u64 {
    // 只要「当月第几天」，所以不算年份，era 那一步省掉
    let doe = (days_since_epoch + 719_468) % 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    doy - (153 * mp + 2) / 5 + 1
}
