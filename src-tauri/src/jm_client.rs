use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use aes::cipher::generic_array::GenericArray;
use aes::cipher::{BlockDecrypt, KeyInit};
use aes::Aes256;
use base64::engine::general_purpose;
use base64::Engine;
use bytes::Bytes;
use eyre::{eyre, OptionExt, WrapErr};
use image::ImageFormat;
use parking_lot::RwLock;
use reqwest::cookie::{CookieStore, Jar};
use reqwest::header::{HeaderValue, SET_COOKIE};
use reqwest::StatusCode;
use reqwest::Url;
use reqwest_middleware::ClientWithMiddleware;
use reqwest_retry::policies::ExponentialBackoff;
use reqwest_retry::{Jitter, RetryTransientMiddleware};
use serde_json::json;
use tauri::AppHandle;
use tracing::instrument;

use crate::config::Config;
use crate::extensions::AppHandleExt;
use crate::lines;
use crate::responses::{
    FavoriteFolderActionRespData, GetChapterRespData, GetComicRespData, GetFavoriteRespData,
    GetUserProfileRespData, GetWeeklyInfoRespData, GetWeeklyRespData, JmResp, RedirectRespData,
    SearchResp, SearchRespData, ToggleFavoriteRespData,
};
use crate::types::{
    CategoryNode, CategoryResp, CommentPage, FavoriteSort, ProxyMode, SearchSort, SubCategoryNode, TagBlock,
};
use crate::utils;

/// 默认图片线路：真正的线路选择交给 `lines` 模块，失败会自动换线路
pub const IMAGE_DOMAIN: &str = "cdn-msp2.jmapiproxy2.cc";

/// 评论一页多少条：官方接口每页固定 10 条，我们一次请求 3 页合并成 30 条
pub const COMMENT_PAGE_SIZE: i64 = 30;

const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/128.0.0.0 Safari/537.36";

const APP_TOKEN_SECRET: &str = "18comicAPP";

const APP_TOKEN_SECRET_2: &str = "18comicAPPContent";

const APP_DATA_SECRET: &str = "185Hcomic3PAPP7R";

const APP_VERSION: &str = "2.0.13";

/// 自动重新登录失败后的冷却时间：这段时间内不再重登，免得请求一失败就反复登录
const RELOGIN_COOLDOWN: Duration = Duration::from_secs(60);

/// 登录态 AVS 自己拿着，其余 cookie 交给 reqwest 的 jar
/// - 禁漫对任何未登录请求都会下发一个游客 AVS，名字、域、路径跟登录态完全一样，
///   jar 正是按这几项存取，谁后到谁覆盖。所以登录成功后把 AVS 记在这里，
///   取 cookie 时直接盖掉 jar 里的那份，游客 AVS 就顶不掉了
#[derive(Default)]
struct SessionCookieStore {
    jar: Jar,
    /// 登录态 AVS，None 表示还没登录
    session: RwLock<Option<String>>,
}

impl SessionCookieStore {
    fn set_session(&self, avs: String) {
        *self.session.write() = Some(avs);
    }

    /// 退出登录：丢掉登录态 AVS，之后的请求退回游客身份
    fn clear_session(&self) {
        *self.session.write() = None;
    }

    /// 拼请求要带的 cookie：jar 里的原样保留，只把 AVS 换成登录态的那份
    fn cookies_text(&self, url: &Url) -> Option<String> {
        let jar_cookies = self
            .jar
            .cookies(url)
            .and_then(|value| value.to_str().ok().map(str::to_string));
        let session = self.session.read();
        let Some(avs) = session.as_ref() else {
            return jar_cookies;
        };

        let mut parts = jar_cookies
            .map(|text| {
                text.split("; ")
                    .filter(|part| !part.starts_with("AVS="))
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        parts.push(format!("AVS={avs}"));
        Some(parts.join("; "))
    }
}

impl CookieStore for SessionCookieStore {
    fn set_cookies(&self, cookie_headers: &mut dyn Iterator<Item = &HeaderValue>, url: &Url) {
        self.jar.set_cookies(cookie_headers, url);
    }

    fn cookies(&self, url: &Url) -> Option<HeaderValue> {
        HeaderValue::from_str(&self.cookies_text(url)?).ok()
    }
}

/// 从 /login 的响应头里取出登录态 AVS；拿不到就返回 None，后面还是走 jar
fn extract_session_avs(response: &reqwest::Response) -> Option<String> {
    response
        .headers()
        .get_all(SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find_map(|text| {
            let (name, rest) = text.split_once('=')?;
            if !name.trim().eq_ignore_ascii_case("AVS") {
                return None;
            }
            let avs = rest.split(';').next()?.trim();
            (!avs.is_empty()).then(|| avs.to_string())
        })
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum ApiPath {
    Login,
    GetUserProfile,
    Search,
    GetComic,
    GetChapter,
    GetScrambleId,
    GetFavoriteFolder,
    GetWeeklyInfo,
    GetWeekly,
    GetCategories,
    GetRanking,
    GetComments,
    ManageFavoriteFolder,
}

impl ApiPath {
    fn as_str(&self) -> &'static str {
        match self {
            // 没错，就是这么奇葩，获取用户信息也是用的/login
            // 带AVS去请求/login，就能获取用户信息，而不需要用户名密码
            // 如果AVS无效或过期，就走正常的登录流程
            ApiPath::Login | ApiPath::GetUserProfile => "/login",
            ApiPath::Search => "/search",
            ApiPath::GetComic => "/album",
            ApiPath::GetChapter => "/chapter",
            ApiPath::GetScrambleId => "/chapter_view_template",
            ApiPath::GetFavoriteFolder => "/favorite",
            ApiPath::GetWeeklyInfo => "/week",
            ApiPath::GetWeekly => "/week/filter",
            ApiPath::GetCategories => "/categories",
            ApiPath::GetRanking => "/categories/filter/",
            ApiPath::GetComments => "/forum",
            ApiPath::ManageFavoriteFolder => "/favorite_folder",
        }
    }
}

#[derive(Clone)]
pub struct JmClient {
    app: AppHandle,
    api_client: Arc<RwLock<ClientWithMiddleware>>,
    cookie_store: Arc<SessionCookieStore>,
    img_client: Arc<RwLock<ClientWithMiddleware>>,
    /// 重新登录的锁：并发请求同时撞到 401 时只登一次
    relogin_lock: Arc<tokio::sync::Mutex<()>>,
    /// 每成功登录一次加一：等锁的请求靠它判断别人是不是已经登好了
    session_generation: Arc<AtomicU64>,
    /// 上次自动重登失败的时间
    last_relogin_failure: Arc<RwLock<Option<Instant>>>,
}

/// 取出 url 的 host
fn url_host(url: &str) -> Option<&str> {
    url.split_once("://")?.1.split(['/', '?', '#']).next()
}

/// 换掉 url 的 host，路径和查询串保持不变
fn replace_url_host(url: &str, domain: &str) -> String {
    match url.split_once("://") {
        Some((scheme, rest)) => match rest.split_once('/') {
            Some((_host, path)) => format!("{scheme}://{domain}/{path}"),
            None => format!("{scheme}://{domain}"),
        },
        None => format!("https://{domain}"),
    }
}

/// 轻量探测某条 API 线路：请求一次 `/categories`，返回耗时
pub async fn probe_api_domain(client: &reqwest::Client, domain: &str) -> eyre::Result<Duration> {
    let ts = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    let token = utils::md5_hex(&format!("{ts}{APP_TOKEN_SECRET}"));
    let tokenparam = format!("{ts},{APP_VERSION}");

    let start = Instant::now();
    let http_resp = client
        .get(format!("https://{domain}{}", ApiPath::GetCategories.as_str()))
        .header("token", token)
        .header("tokenparam", tokenparam)
        .header("user-agent", USER_AGENT)
        .send()
        .await?;
    let status = http_resp.status();
    let body = http_resp.text().await?;
    let elapsed = start.elapsed();

    if status != StatusCode::OK {
        return Err(eyre!("状态码 {status}"));
    }
    let jm_resp = serde_json::from_str::<JmResp>(&body).wrap_err("返回内容不是禁漫接口的响应")?;
    if jm_resp.code != 200 {
        return Err(eyre!("接口返回 code={}", jm_resp.code));
    }

    Ok(elapsed)
}

/// 轻量探测某条图片线路：图片 CDN 对根路径一般返回 403，只要能连上就算可用
pub async fn probe_image_domain(client: &reqwest::Client, domain: &str) -> eyre::Result<Duration> {
    let start = Instant::now();
    let http_resp = client
        .get(format!("https://{domain}/"))
        .header("user-agent", USER_AGENT)
        .send()
        .await?;
    let status = http_resp.status();
    let elapsed = start.elapsed();

    if status.is_server_error() {
        return Err(eyre!("状态码 {status}"));
    }

    Ok(elapsed)
}

/// 代理配置校验（只检查地址格式，不测连通性）
/// - 保存配置前先调这个：代理非法就不要写进配置文件，
///   否则重启时建客户端会失败，应用直接起不来
pub fn validate_proxy_settings(
    proxy_mode: &ProxyMode,
    proxy_host: &str,
    proxy_port: u16,
) -> eyre::Result<()> {
    if *proxy_mode != ProxyMode::Custom {
        return Ok(());
    }
    let proxy_url = format!("http://{proxy_host}:{proxy_port}");
    reqwest::Proxy::all(&proxy_url).wrap_err(format!("代理地址`{proxy_url}`不合法"))?;
    Ok(())
}

/// 按配置给客户端挂代理
/// - 代理地址非法时返回错误，而不是静默直连：否则用户以为走了代理，其实没走
fn apply_proxy(
    builder: reqwest::ClientBuilder,
    config: &Config,
) -> eyre::Result<reqwest::ClientBuilder> {
    let builder = match config.proxy_mode {
        ProxyMode::System => builder,
        ProxyMode::NoProxy => builder.no_proxy(),
        ProxyMode::Custom => {
            let proxy_url = format!("http://{}:{}", config.proxy_host, config.proxy_port);
            let proxy =
                reqwest::Proxy::all(&proxy_url).wrap_err(format!("代理地址`{proxy_url}`不合法"))?;
            builder.proxy(proxy)
        }
    };
    Ok(builder)
}

fn create_api_client_with_config(
    config: &Config,
    cookie_store: &Arc<SessionCookieStore>,
) -> eyre::Result<ClientWithMiddleware> {
    let builder = apply_proxy(
        reqwest::ClientBuilder::new().cookie_provider(cookie_store.clone()),
        config,
    )?;

    let retry_policy = ExponentialBackoff::builder()
        .base(1) // 指数为1，保证重试间隔为1秒不变
        .jitter(Jitter::Bounded) // 重试间隔在1秒左右波动
        .build_with_total_retry_duration(Duration::from_secs(5)); // 重试总时长为5秒

    let client = builder
        .build()
        .wrap_err("创建 API 客户端失败（请检查代理设置）")?;

    Ok(
        reqwest_middleware::ClientBuilder::new(client)
            .with(RetryTransientMiddleware::new_with_policy(retry_policy))
            .build(),
    )
}

fn create_img_client_with_config(config: &Config) -> eyre::Result<ClientWithMiddleware> {
    let builder = apply_proxy(reqwest::ClientBuilder::new(), config)?;
    let retry_policy = ExponentialBackoff::builder().build_with_max_retries(2);

    let client = builder
        .build()
        .wrap_err("创建图片客户端失败（请检查代理设置）")?;

    Ok(
        reqwest_middleware::ClientBuilder::new(client)
            .with(RetryTransientMiddleware::new_with_policy(retry_policy))
            .build(),
    )
}

fn create_api_client(
    app: &AppHandle,
    cookie_store: &Arc<SessionCookieStore>,
) -> eyre::Result<ClientWithMiddleware> {
    let config = app.get_config().read().clone();
    create_api_client_with_config(&config, cookie_store)
}

pub fn create_img_client(app: &AppHandle) -> eyre::Result<ClientWithMiddleware> {
    let config = app.get_config().read().clone();
    create_img_client_with_config(&config)
}

/// 启动时建客户端：代理配置万一不合法（比如用户手改了 config.json），
/// 也绝不能让应用起不来 —— 退化成直连并记一条错误日志
fn create_api_client_or_direct(
    app: &AppHandle,
    cookie_store: &Arc<SessionCookieStore>,
) -> ClientWithMiddleware {
    let config = app.get_config().read().clone();
    match create_api_client_with_config(&config, cookie_store) {
        Ok(client) => client,
        Err(err) => {
            tracing::error!(message = %err, "创建API客户端失败，本次退化为直连（请检查`配置`里的代理设置）");
            let mut fallback_config = config;
            fallback_config.proxy_mode = ProxyMode::NoProxy;
            create_api_client_with_config(&fallback_config, cookie_store).unwrap_or_else(|err| {
                tracing::error!(message = %err, "退化为直连仍然失败，使用不带重试的裸客户端");
                reqwest_middleware::ClientBuilder::new(reqwest::Client::new()).build()
            })
        }
    }
}

fn create_img_client_or_direct(app: &AppHandle) -> ClientWithMiddleware {
    let config = app.get_config().read().clone();
    match create_img_client_with_config(&config) {
        Ok(client) => client,
        Err(err) => {
            tracing::error!(message = %err, "创建图片客户端失败，本次退化为直连（请检查`配置`里的代理设置）");
            let mut fallback_config = config;
            fallback_config.proxy_mode = ProxyMode::NoProxy;
            create_img_client_with_config(&fallback_config).unwrap_or_else(|err| {
                tracing::error!(message = %err, "退化为直连仍然失败，使用不带重试的裸客户端");
                reqwest_middleware::ClientBuilder::new(reqwest::Client::new()).build()
            })
        }
    }
}

/// 官方返回里 id / total_albums 有时是数字有时是字符串，这里都兼容
fn json_to_i64(value: &serde_json::Value) -> i64 {
    value
        .as_i64()
        .or_else(|| value.as_str().and_then(|text| text.parse().ok()))
        .unwrap_or(0)
}

fn json_to_string(value: &serde_json::Value) -> String {
    value.as_str().map_or_else(String::new, str::to_string)
}

fn parse_categories(value: &serde_json::Value) -> CategoryResp {
    let categories = value
        .get("categories")
        .and_then(serde_json::Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| CategoryNode {
                    id: json_to_i64(&item["id"]),
                    name: json_to_string(&item["name"]),
                    slug: json_to_string(&item["slug"]),
                    total_albums: json_to_i64(&item["total_albums"]),
                    sub_categories: item
                        .get("sub_categories")
                        .and_then(serde_json::Value::as_array)
                        .map(|subs| {
                            subs.iter()
                                .map(|sub| SubCategoryNode {
                                    cid: json_to_i64(&sub["CID"]),
                                    name: json_to_string(&sub["name"]),
                                    slug: json_to_string(&sub["slug"]),
                                })
                                .collect()
                        })
                        .unwrap_or_default(),
                })
                .collect()
        })
        .unwrap_or_default();

    let blocks = value
        .get("blocks")
        .and_then(serde_json::Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| TagBlock {
                    title: json_to_string(&item["title"]),
                    content: item
                        .get("content")
                        .and_then(serde_json::Value::as_array)
                        .map(|tags| {
                            tags.iter()
                                .filter_map(serde_json::Value::as_str)
                                .map(str::to_string)
                                .collect()
                        })
                        .unwrap_or_default(),
                })
                .collect()
        })
        .unwrap_or_default();

    CategoryResp { categories, blocks }
}

/// 解密接口返回的 data 字段（AES-256-ECB + PKCS#7）
/// - 接口偶尔会返回空内容或非 AES 数据（被劫持、token 过期、响应被截断），
///   这里全部按错误返回，绝不能 panic：release 是 `panic = "abort"`，一 panic 整个应用就没了
fn decrypt_data(ts: u64, data: &str) -> eyre::Result<String> {
    // 使用Base64解码传入的数据，得到AES-256-ECB加密的数据
    let aes256_ecb_encrypted_data = general_purpose::STANDARD
        .decode(data)
        .wrap_err("接口返回的 data 不是合法的 base64")?;

    if aes256_ecb_encrypted_data.is_empty() || aes256_ecb_encrypted_data.len() % 16 != 0 {
        return Err(eyre!(
            "接口返回的 data 长度不合法({}字节)，不是 AES-256-ECB 数据",
            aes256_ecb_encrypted_data.len()
        ));
    }

    // 生成密钥
    let key = utils::md5_hex(&format!("{ts}{APP_DATA_SECRET}"));
    // 使用AES-256-ECB进行解密
    let cipher = Aes256::new(GenericArray::from_slice(key.as_bytes()));
    let decrypted_data_with_padding: Vec<u8> = aes256_ecb_encrypted_data
        .chunks(16)
        .map(GenericArray::clone_from_slice)
        .flat_map(|mut block| {
            cipher.decrypt_block(&mut block);
            block.to_vec()
        })
        .collect();

    // 去除PKCS#7填充：填充长度必须是 1..=16 且不超过数据长度，否则说明数据/密钥不对
    let padding_length = *decrypted_data_with_padding
        .last()
        .ok_or_eyre("解密结果为空")? as usize;
    if padding_length == 0
        || padding_length > 16
        || padding_length > decrypted_data_with_padding.len()
    {
        return Err(eyre!("解密后 PKCS#7 填充长度不合法({padding_length})"));
    }

    let unpadded_len = decrypted_data_with_padding.len() - padding_length;
    let decrypted_data = String::from_utf8(decrypted_data_with_padding[..unpadded_len].to_vec())
        .wrap_err("解密后的数据不是 UTF-8")?;
    Ok(decrypted_data)
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn url_host_should_parse_host() {
        assert_eq!(url_host("https://cdn-msp2.jmapiproxy2.cc/media/photos/1/2.webp"), Some("cdn-msp2.jmapiproxy2.cc"));
        assert_eq!(url_host("https://cdn-msp2.jmapiproxy2.cc"), Some("cdn-msp2.jmapiproxy2.cc"));
        assert_eq!(url_host("https://cdn-msp2.jmapiproxy2.cc?x=1"), Some("cdn-msp2.jmapiproxy2.cc"));
        assert_eq!(url_host("/media/photos/1/2.webp"), None);
    }

    #[test]
    fn replace_url_host_should_keep_path_and_query() {
        let url = "https://cdn-msp2.jmapiproxy2.cc/media/photos/432873/00001.webp?ts=1";
        assert_eq!(
            replace_url_host(url, "cdn-msp3.jmapiproxy1.cc"),
            "https://cdn-msp3.jmapiproxy1.cc/media/photos/432873/00001.webp?ts=1"
        );
        assert_eq!(
            replace_url_host("https://cdn-msp3.18comic.vip/media/albums/1.jpg", "cdn-msp2.jmapiproxy2.cc"),
            "https://cdn-msp2.jmapiproxy2.cc/media/albums/1.jpg"
        );
    }
}

#[cfg(test)]
mod decrypt_tests {
    use super::*;
    use aes::cipher::BlockEncrypt;
    use base64::engine::general_purpose;
    use base64::Engine;

    /// 用同一个密钥按 AES-256-ECB 加密（调用方自己保证 16 字节对齐）
    fn encrypt(ts: u64, plain: &[u8]) -> String {
        assert_eq!(plain.len() % 16, 0);
        let key = utils::md5_hex(&format!("{ts}{APP_DATA_SECRET}"));
        let cipher = Aes256::new(GenericArray::from_slice(key.as_bytes()));
        let encrypted: Vec<u8> = plain
            .chunks(16)
            .flat_map(|chunk| {
                let mut block = GenericArray::clone_from_slice(chunk);
                cipher.encrypt_block(&mut block);
                block.to_vec()
            })
            .collect();
        general_purpose::STANDARD.encode(encrypted)
    }

    #[test]
    fn decrypt_data_should_round_trip() {
        let ts = 1_700_000_000_u64;
        let plain = "{\"hello\":\"世界\"}".as_bytes();
        let padding = 16 - plain.len() % 16;
        let mut padded = plain.to_vec();
        padded.extend(std::iter::repeat(padding as u8).take(padding));

        let data = encrypt(ts, &padded);
        assert_eq!(decrypt_data(ts, &data).unwrap(), "{\"hello\":\"世界\"}");
    }

    #[test]
    fn decrypt_data_should_return_error_instead_of_panicking() {
        let ts = 1_700_000_000_u64;

        // 空 data：以前这里 `.last().unwrap()` 会 panic
        assert!(decrypt_data(ts, "").is_err());
        // 不是 base64
        assert!(decrypt_data(ts, "not base64!!").is_err());
        // 长度不是 16 的倍数
        let short = general_purpose::STANDARD.encode([1_u8, 2, 3]);
        assert!(decrypt_data(ts, &short).is_err());
        // 长度合法但 PKCS#7 填充长度为 0（以前会算出 len-0 而不报错，属于数据不对）
        let zero_padding = encrypt(ts, &[0_u8; 16]);
        assert!(decrypt_data(ts, &zero_padding).is_err());
        // 填充长度 > 16（以前 `len - padding` 会下溢 panic）
        let mut bad = vec![0_u8; 15];
        bad.push(200);
        let bad_padding = encrypt(ts, &bad);
        assert!(decrypt_data(ts, &bad_padding).is_err());
    }
}

mod client;
