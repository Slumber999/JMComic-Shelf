//! 线路管理：API 线路测速 + 图片线路自动 fallback
//!
//! - **API 线路**：设置页可以一键对官方 6 条线路做轻量测速（并发请求 `/categories`），按延迟排序后选用最快的；
//!   请求失败时按运行时顺序自动换线（见 jm_client::client 的 jm_request）
//! - **图片线路**：默认 host 是 `IMAGE_DOMAIN`，某条图片 CDN 挂掉就会出现「能搜到但下载 0 进度」
//!   （issue #195）。这里维护一个运行时的线路健康顺序：失败自动换下一条，成功的那条会被记到最前面，
//!   之后的图片就直接走它，不会每张图都先超时一次

use std::sync::LazyLock;
use std::time::Duration;

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::AppHandle;

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

/// 运行时选中的 API 线路：换线成功后固定下来，设置里改线路时清空
static API_LINE_ACTIVE: LazyLock<RwLock<Option<String>>> = LazyLock::new(|| RwLock::new(None));

/// 当前该用的 API 域名：运行时换过线就用换过的，否则用配置里选的
pub fn active_api_domain(configured: &str) -> String {
    API_LINE_ACTIVE
        .read()
        .clone()
        .unwrap_or_else(|| configured.to_string())
}

/// 某条 API 线路请求成功：之后的请求优先走它
pub fn mark_api_line_ok(domain: &str) {
    *API_LINE_ACTIVE.write() = Some(domain.to_string());
}

/// 设置里改了线路：清掉运行时选择，回到配置里的那条
pub fn reset_api_line() {
    *API_LINE_ACTIVE.write() = None;
}

/// 候选线路：当前线路 -> 其余官方线路（去重），最多 API_LINE_MAX_ATTEMPTS 条
pub fn api_line_candidates(active: &str) -> Vec<String> {
    let mut candidates = Vec::with_capacity(API_LINE_MAX_ATTEMPTS);
    let active = active.trim();
    if !active.is_empty() {
        candidates.push(active.to_string());
    }
    for (_, domain) in api_line_domains() {
        if candidates.iter().any(|candidate| candidate == domain) {
            continue;
        }
        candidates.push(domain.to_string());
        if candidates.len() >= API_LINE_MAX_ATTEMPTS {
            break;
        }
    }
    candidates
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
    fn api_line_active_should_follow_mark_and_reset() {
        reset_api_line();
        assert_eq!(active_api_domain("a.example.com"), "a.example.com");
        mark_api_line_ok("b.example.com");
        assert_eq!(active_api_domain("a.example.com"), "b.example.com");
        reset_api_line();
        assert_eq!(active_api_domain("a.example.com"), "a.example.com");
    }
}
