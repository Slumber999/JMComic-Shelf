//! 卡顿取证用的统一打点：进入 / 完成时间 + 所在线程名，一行一条 debug 日志。
//!
//! 桌面端日志文件里搜 `measure` 就能拿到完整时间线：谁在什么线程上、等了多久。
//! Android 上这些行同样会进文件日志；封面/线路的 logcat 打点（JMCOVER/JMAPI）保持原样。

use std::time::{SystemTime, UNIX_EPOCH};

/// 当前线程名：用来判断一段活到底跑在 UI 线程（`main`）还是 tokio 工作线程上
pub fn thread_name() -> String {
    std::thread::current()
        .name()
        .unwrap_or("unnamed")
        .to_string()
}

/// 当前时间（epoch 毫秒）：和前端/日志时间戳对齐用
pub fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis())
        .unwrap_or_default()
}

/// 打一条进入时间线，返回起始时间给 [`finish`] 用
pub fn enter(op: &str, detail: &str) -> u128 {
    let at = now_ms();
    tracing::debug!(
        op,
        detail,
        thread = %thread_name(),
        at_ms = at,
        "measure enter"
    );
    at
}

/// 打一条完成时间线：`dt_ms` 是相对 [`enter`] 的耗时
pub fn finish(op: &str, started: u128, detail: &str) {
    let at = now_ms();
    tracing::debug!(
        op,
        detail,
        thread = %thread_name(),
        at_ms = at,
        dt_ms = at.saturating_sub(started),
        "measure exit"
    );
}
