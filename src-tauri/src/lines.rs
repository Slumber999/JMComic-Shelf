//! 线路管理：API 线路测速 + 图片线路自动 fallback
//!
//! - **API 线路**：设置页可以一键对官方 6 条线路做轻量测速（并发请求 `/categories`），按延迟排序后选用最快的；
//!   请求失败时按运行时顺序自动换线（见 jm_client::client 的 jm_request）
//! - **图片线路**：默认 host 是 `IMAGE_DOMAIN`，某条图片 CDN 挂掉就会出现「能搜到但下载 0 进度」
//!   （issue #195）。这里维护一个运行时的线路健康顺序：失败自动换下一条，成功的那条会被记到最前面，
//!   之后的图片就直接走它，不会每张图都先超时一次

use std::collections::HashMap;
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager};

use crate::config::{api_line_domains, ApiDomainMode};
use crate::extensions::AppHandleExt;
use crate::jm_client;

/// 图片线路候选，前 5 条是禁漫 App 的图片 CDN（互为备份），最后一条是旧版硬编码的封面线路
pub const IMAGE_LINE_DOMAINS: [&str; 6] = [
    "cdn-msp2.jmapiproxy2.cc",
    "cdn-msp.jmapiproxy2.cc",
    "cdn-msp3.jmapiproxy2.cc",
    "cdn-msp2.jmapiproxy1.cc",
    "cdn-msp3.jmapiproxy1.cc",
    "cdn-msp3.18comic.vip",
];

/// 测速超时时间：线路不通时要尽快失败，不然测速会卡很久
pub const PROBE_TIMEOUT: Duration = Duration::from_secs(8);

/// 运行时线路顺序，第 0 个就是当前正在用的
static IMAGE_LINE_ORDER: LazyLock<RwLock<Vec<&'static str>>> =
    LazyLock::new(|| RwLock::new(IMAGE_LINE_DOMAINS.to_vec()));

/// 是否是已知的图片线路域名
pub fn is_image_line(domain: &str) -> bool {
    IMAGE_LINE_DOMAINS.contains(&domain)
}

/// 当前正在用的图片线路
pub fn active_image_domain() -> &'static str {
    IMAGE_LINE_ORDER
        .read()
        .first()
        .copied()
        .unwrap_or(IMAGE_LINE_DOMAINS[0])
}

/// 按「当前线路 -> 地址里原本的线路 -> 其它线路」的顺序给出候选（去重）
pub fn image_line_candidates(host: &str) -> Vec<&'static str> {
    let order = IMAGE_LINE_ORDER.read();
    let active = order.first().copied();
    let original = IMAGE_LINE_DOMAINS.iter().copied().find(|d| *d == host);

    let mut candidates: Vec<&'static str> = Vec::with_capacity(IMAGE_LINE_DOMAINS.len());
    for domain in active
        .into_iter()
        .chain(original)
        .chain(order.iter().copied())
    {
        if !candidates.contains(&domain) {
            candidates.push(domain);
        }
    }
    candidates
}

/// 某条线路下载成功：提到最前面，之后的请求优先走它
pub fn mark_image_line_ok(domain: &str) {
    let Some(known) = IMAGE_LINE_DOMAINS.iter().copied().find(|d| *d == domain) else {
        return;
    };
    let mut order = IMAGE_LINE_ORDER.write();
    if order.first() == Some(&known) {
        return;
    }
    order.retain(|d| *d != known);
    order.insert(0, known);
}

/// 某条线路失败：挪到最后，本次会话内不再优先尝试
pub fn mark_image_line_failed(domain: &str) {
    let mut order = IMAGE_LINE_ORDER.write();
    if let Some(index) = order.iter().position(|d| *d == domain) {
        let domain = order.remove(index);
        order.push(domain);
    }
}

/// 一次请求最多尝试的 API 线路数：失败自动换线，全部失败才报错
pub const API_LINE_MAX_ATTEMPTS: usize = 3;

/// 线路熔断窗口：某条线路刚失败后，其它请求在窗口内直接跳过它，
/// 不用各自再等同一条死线路超时。窗口很短，恢复后自动放行
pub const API_LINE_COOLDOWN: Duration = Duration::from_secs(10);

/// 运行时熔断记录：域名 -> 最近一次失败时间（连接/TLS/超时/5xx）
static API_LINE_FAILED_AT: LazyLock<RwLock<HashMap<String, Instant>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// 运行时选中的 API 线路：换线成功后固定下来，设置里改线路时清空
static API_LINE_ACTIVE: LazyLock<RwLock<Option<String>>> = LazyLock::new(|| RwLock::new(None));

/// 是否已经确定可用线路（冷启动抢跑只在还没确定时触发）
pub fn has_active_api_line() -> bool {
    API_LINE_ACTIVE.read().is_some()
}

/// 当前该用的 API 域名：运行时换过线就用换过的，否则用配置里选的
pub fn active_api_domain(configured: &str) -> String {
    API_LINE_ACTIVE
        .read()
        .clone()
        .unwrap_or_else(|| configured.to_string())
}

/// 上次成功线路的落盘文件：冷启动直接用这条，避免第一条请求先等一条挂掉的线路超时
const API_LINE_FILE: &str = "api-line.json";

#[derive(Serialize, Deserialize)]
struct ApiLineState {
    domain: String,
}

/// 某条 API 线路请求成功：之后的请求优先走它
pub fn mark_api_line_ok(domain: &str) {
    API_LINE_FAILED_AT.write().remove(domain);
    *API_LINE_ACTIVE.write() = Some(domain.to_string());
}

/// 请求成功 + 落盘（只在变化时写文件）：下次冷启动从这条线路开始
pub fn mark_api_line_ok_and_persist(app: &AppHandle, domain: &str) {
    let changed = API_LINE_ACTIVE.read().as_deref() != Some(domain);
    mark_api_line_ok(domain);
    if !changed {
        return;
    }
    let Ok(dir) = app.path().app_data_dir() else {
        return;
    };
    let state = ApiLineState {
        domain: domain.to_string(),
    };
    let text = serde_json::to_string(&state).unwrap_or_default();
    let _ = std::fs::write(dir.join(API_LINE_FILE), text);
}

/// 某条 API 线路失败（连接/TLS/超时/5xx）：熔断窗口内其它请求不再试它
pub fn mark_api_line_failed(domain: &str) {
    API_LINE_FAILED_AT
        .write()
        .insert(domain.to_string(), Instant::now());
}

/// 启动时恢复上次成功的线路（文件不存在 / 坏掉就安静忽略）
pub fn load_persisted_api_line(app: &AppHandle) {
    let Ok(dir) = app.path().app_data_dir() else {
        return;
    };
    let Ok(text) = std::fs::read_to_string(dir.join(API_LINE_FILE)) else {
        return;
    };
    let Ok(state) = serde_json::from_str::<ApiLineState>(&text) else {
        return;
    };
    let domain = state.domain.trim();
    if domain.is_empty() {
        return;
    }
    *API_LINE_ACTIVE.write() = Some(domain.to_string());
}

/// 设置里改了线路：清掉运行时选择和熔断记录，回到配置里的那条
pub fn reset_api_line() {
    *API_LINE_ACTIVE.write() = None;
    API_LINE_FAILED_AT.write().clear();
}

/// 同上，顺带删掉落盘记录（设置页改线路时用）
pub fn reset_api_line_and_file(app: &AppHandle) {
    reset_api_line();
    if let Ok(dir) = app.path().app_data_dir() {
        let _ = std::fs::remove_file(dir.join(API_LINE_FILE));
    }
}

/// 候选线路：当前线路 -> 其余官方线路（去重），熔断中的线路跳过，
/// 最多 API_LINE_MAX_ATTEMPTS 条；全部线路都在熔断时按原顺序立刻重试
pub fn api_line_candidates(active: &str) -> Vec<String> {
    let all = all_api_line_candidates(active);
    let selected = {
        let failed_at = API_LINE_FAILED_AT.read();
        select_api_line_candidates(&all, &failed_at, Instant::now())
    };
    selected.into_iter().take(API_LINE_MAX_ATTEMPTS).collect()
}

/// 全部候选线路（不截断）：当前线路在最前，后面是官方线路
fn all_api_line_candidates(active: &str) -> Vec<String> {
    let mut all = Vec::with_capacity(api_line_domains().len() + 1);
    let active = active.trim();
    if !active.is_empty() {
        all.push(active.to_string());
    }
    for (_, domain) in api_line_domains() {
        if !all.iter().any(|candidate| candidate == domain) {
            all.push(domain.to_string());
        }
    }
    all
}

/// 跳过熔断窗口内的线路；全部线路都在熔断时放行全部，避免把应用锁死
fn select_api_line_candidates(
    all: &[String],
    failed_at: &HashMap<String, Instant>,
    now: Instant,
) -> Vec<String> {
    let fresh: Vec<String> = all
        .iter()
        .filter(|domain| !is_api_line_circuit_open(failed_at, domain, now))
        .cloned()
        .collect();
    if fresh.is_empty() {
        all.to_vec()
    } else {
        fresh
    }
}

/// 这条线路是否还在熔断窗口内
fn is_api_line_circuit_open(
    failed_at: &HashMap<String, Instant>,
    domain: &str,
    now: Instant,
) -> bool {
    failed_at
        .get(domain)
        .is_some_and(|at| now.saturating_duration_since(*at) < API_LINE_COOLDOWN)
}

/// API 线路测速结果
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ApiLineProbeResult {
    /// 线路名，例如「线路1」
    pub label: String,
    /// 对应设置页的线路选项
    pub mode: ApiDomainMode,
    pub domain: String,
    pub ok: bool,
    pub latency_ms: Option<u64>,
    pub error: Option<String>,
}

/// 图片线路测速结果
#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ImageLineProbeResult {
    pub domain: String,
    pub ok: bool,
    pub latency_ms: Option<u64>,
    pub error: Option<String>,
}

/// 测速用的客户端：不做重试，免得延迟被重试放大；代理配置与其它客户端共用同一份
pub fn create_probe_client(app: &AppHandle) -> eyre::Result<reqwest::Client> {
    let builder = reqwest::ClientBuilder::new()
        .timeout(PROBE_TIMEOUT)
        .connect_timeout(PROBE_TIMEOUT);

    let config = app.get_config().read().clone();
    Ok(jm_client::apply_proxy(builder, &config)?.build()?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_line_order_follows_health() {
        // 注意：这几个函数操作的是全局状态，放在一个测试里串行验证
        let first = IMAGE_LINE_DOMAINS[0];
        let second = IMAGE_LINE_DOMAINS[1];

        assert!(is_image_line(first));
        assert!(!is_image_line("example.com"));

        // 默认第一条在最前面
        mark_image_line_ok(first);
        assert_eq!(active_image_domain(), first);
        assert_eq!(image_line_candidates(first)[0], first);

        // 第二条成功后会顶到最前面，之后所有请求都先试它
        mark_image_line_ok(second);
        assert_eq!(active_image_domain(), second);
        assert_eq!(image_line_candidates(first)[0], second);

        // 候选里不能有重复，并且要包含地址里原本的线路
        let candidates = image_line_candidates(first);
        assert_eq!(candidates.len(), IMAGE_LINE_DOMAINS.len());
        assert!(candidates.contains(&first));

        // 失败后挪到最后，但不会丢
        mark_image_line_failed(second);
        assert_eq!(active_image_domain(), first);
        assert_eq!(*IMAGE_LINE_ORDER.read().last().unwrap(), second);
        assert_eq!(IMAGE_LINE_ORDER.read().len(), IMAGE_LINE_DOMAINS.len());

        // 未知域名不影响现有顺序
        mark_image_line_ok("unknown.example.com");
        mark_image_line_failed("unknown.example.com");
        assert_eq!(active_image_domain(), first);
    }

    #[test]
    fn api_line_candidates_should_start_with_active_and_cap_attempts() {
        let known = |domain: &str| api_line_domains().iter().any(|(_, d)| *d == domain);

        let active = api_line_domains()[2].1;
        let candidates = api_line_candidates(active);
        assert_eq!(candidates.len(), API_LINE_MAX_ATTEMPTS);
        assert_eq!(candidates[0], active);
        assert!(candidates.iter().all(|domain| known(domain)));
        let unique: std::collections::HashSet<_> = candidates.iter().collect();
        assert_eq!(unique.len(), candidates.len());

        // 自定义线路也排第一，后面用官方线路兜底
        let custom = api_line_candidates("custom.example.com");
        assert_eq!(custom.len(), API_LINE_MAX_ATTEMPTS);
        assert_eq!(custom[0], "custom.example.com");
        assert!(custom[1..].iter().all(|domain| known(domain)));

        // 空值不会产出空线路
        let empty = api_line_candidates("  ");
        assert_eq!(empty.len(), API_LINE_MAX_ATTEMPTS);
        assert!(empty.iter().all(|domain| !domain.is_empty()));
    }

    #[test]
    fn circuit_open_lines_should_be_skipped_then_recover_after_window() {
        let now = Instant::now();
        let all = all_api_line_candidates(api_line_domains()[0].1);
        let dead = all[0].clone();

        let mut failed = HashMap::new();
        failed.insert(dead.clone(), now);

        // 刚失败：跳过它，其余线路顺序不变
        let picked = select_api_line_candidates(&all, &failed, now);
        assert_eq!(picked.len(), all.len() - 1);
        assert!(!picked.contains(&dead));
        assert_eq!(picked[0], all[1]);

        // 窗口过后自动恢复
        let picked =
            select_api_line_candidates(&all, &failed, now + API_LINE_COOLDOWN);
        assert_eq!(picked, all);
    }

    #[test]
    fn all_lines_failed_should_still_return_candidates() {
        let now = Instant::now();
        let all = all_api_line_candidates("probe.example.com");
        let failed: HashMap<String, Instant> =
            all.iter().map(|domain| (domain.clone(), now)).collect();

        // 全部熔断时放行全部：不能因为等窗口把应用锁死
        assert_eq!(select_api_line_candidates(&all, &failed, now), all);
    }

    #[test]
    fn api_line_active_should_follow_mark_and_reset() {
        reset_api_line();
        assert_eq!(active_api_domain("a.example.com"), "a.example.com");
        mark_api_line_ok("b.example.com");
        assert_eq!(active_api_domain("a.example.com"), "b.example.com");
        reset_api_line();
        assert_eq!(active_api_domain("a.example.com"), "a.example.com");
    }
}
