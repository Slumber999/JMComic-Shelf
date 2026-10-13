use serde::{Deserialize, Serialize};
use specta::Type;

/// 执行签到的结果（官方 /daily_chk 解密后的 data，官方只保证有 msg）
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub struct SignInResult {
    /// 官方文案：成功是「签到成功」，今天签过是「已签到过」
    #[serde(default)]
    pub msg: String,
    /// 官方若带 status 就原样给出
    #[serde(default)]
    pub status: String,
    /// 这次签到用的活动 id（来自签到状态，官方不返回）
    #[serde(default)]
    pub daily_id: i64,
    /// 官方文案里带「已签到过」：今天已经签过了，按成功返回，前端同样可以把按钮收起来
    #[serde(default)]
    pub already_signed: bool,
}
