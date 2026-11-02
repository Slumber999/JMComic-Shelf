//! 登录态持久化：把登录后的 AVS 按域名存盘，启动时直接恢复。
//! 超过七天没启动过就作废，用户必须重新登录。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

/// 登录态文件：和 config.json 同目录
const SESSION_FILE: &str = "session.json";
/// 闲置多少天后登录态失效
const MAX_IDLE_SECS: i64 = 7 * 24 * 60 * 60;

/// 本次启动是否因为闲置超过七天而作废了登录态：前端据此跳过账号密码自动登录
static EXPIRED_ON_STARTUP: AtomicBool = AtomicBool::new(false);

#[derive(Default, Serialize, Deserialize)]
struct StoredSession {
    /// 域名 -> 登录态 AVS
    sessions: HashMap<String, String>,
    /// 域名 -> 该域名下的其余 cookie（PHPSESSID 之类，只存 AVS 服务端不认）
    #[serde(default)]
    cookies: HashMap<String, String>,
    /// 最后一次使用时间（unix 秒），用来判定七天闲置
    saved_at: i64,
}

/// 恢复出来的登录态
pub struct Restored {
    pub sessions: HashMap<String, String>,
    pub cookies: HashMap<String, String>,
}

fn path_of(app: &AppHandle) -> Option<PathBuf> {
    // Android 上 app_data_dir 可能给到包根目录（不可写），逐个试到能写为止
    let mut candidates = Vec::new();
    if let Ok(dir) = app.path().app_data_dir() {
        candidates.push(dir.join("files"));
        candidates.push(dir);
    }
    if let Ok(dir) = app.path().app_local_data_dir() {
        candidates.push(dir);
    }
    if let Ok(dir) = app.path().app_config_dir() {
        candidates.push(dir);
    }
    for dir in candidates {
        if std::fs::create_dir_all(&dir).is_err() {
            continue;
        }
        let probe = dir.join(".session-probe");
        if std::fs::write(&probe, b"1").is_ok() {
            let _ = std::fs::remove_file(&probe);
            tracing::debug!(path = %dir.join(SESSION_FILE).display(), "登录态：落盘目录");
            return Some(dir.join(SESSION_FILE));
        }
    }
    tracing::error!("登录态：找不到可写目录");
    None
}

fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| i64::try_from(elapsed.as_secs()).unwrap_or(0))
        .unwrap_or(0)
}

/// 读回登录态；过期的直接删文件并返回 None
pub fn load(app: &AppHandle) -> Option<Restored> {
    let path = match path_of(app) {
        Some(path) => path,
        None => {
            tracing::warn!("登录态：拿不到 app_data_dir");
            return None;
        }
    };
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) => {
            tracing::debug!(path = %path.display(), message = %err, "登录态：没有可用的文件");
            return None;
        }
    };
    let stored: StoredSession = match serde_json::from_str(&text) {
        Ok(stored) => stored,
        Err(err) => {
            tracing::warn!(message = %err, "登录态文件损坏，按未登录处理");
            let _ = std::fs::remove_file(&path);
            return None;
        }
    };
    if stored.sessions.is_empty() {
        return None;
    }
    if now_secs() - stored.saved_at > MAX_IDLE_SECS {
        tracing::info!("登录态闲置超过七天，已作废");
        let _ = std::fs::remove_file(&path);
        // 记一笔：这次要强制退出，不能再用记住的账号密码自动登进去
        EXPIRED_ON_STARTUP.store(true, Ordering::SeqCst);
        return None;
    }
    // 这次启动也算一次使用：把时间戳刷新掉，七天从最后一次启动算起
    save(app, &stored.sessions, &stored.cookies);
    tracing::info!(
        "恢复登录态：{} 个域名，{} 份 cookie",
        stored.sessions.len(),
        stored.cookies.len()
    );
    Some(Restored {
        sessions: stored.sessions,
        cookies: stored.cookies,
    })
}

/// 覆盖写入当前登录态（保存失败只记日志，不影响本次运行）
pub fn save(app: &AppHandle, sessions: &HashMap<String, String>, cookies: &HashMap<String, String>) {
    let Some(path) = path_of(app) else {
        tracing::error!("登录态：拿不到可写目录，放弃写入");
        return;
    };
    tracing::debug!(sessions = sessions.len(), cookies = cookies.len(), "登录态：准备写入");
    let stored = StoredSession {
        sessions: sessions.clone(),
        cookies: cookies.clone(),
        saved_at: now_secs(),
    };
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    match serde_json::to_string(&stored) {
        Ok(text) => {
            match std::fs::write(&path, &text) {
                Ok(()) => tracing::debug!(
                    path = %path.display(),
                    sessions = sessions.len(),
                    "登录态已写入"
                ),
                Err(err) => tracing::warn!(path = %path.display(), message = %err, "登录态写入失败"),
            }
        }
        Err(err) => tracing::warn!(message = %err, "登录态序列化失败"),
    }
}

/// 取走「本次启动因闲置而过期」的标记（读一次就清掉）
pub fn take_expired() -> bool {
    EXPIRED_ON_STARTUP.swap(false, Ordering::SeqCst)
}

/// 退出登录：直接删掉文件
pub fn clear(app: &AppHandle) {
    if let Some(path) = path_of(app) {
        let _ = std::fs::remove_file(path);
    }
}