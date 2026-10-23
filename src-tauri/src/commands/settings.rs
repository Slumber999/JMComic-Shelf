use std::future::Future;
use std::time::Duration;
use std::{
    fs::File,
    io::{BufRead, BufReader},
};

use eyre::WrapErr;
use tauri::{AppHandle, Manager};
use tauri_plugin_opener::OpenerExt;
use tokio::task::JoinSet;
use tracing::instrument;

use crate::config::{api_line_domains, ApiDomainMode, Config, LocalLibrarySource};
use crate::errors::{CommandError, CommandResult};
use crate::extensions::AppHandleExt;
use crate::jm_client;
use crate::lines::{self, ApiLineProbeResult, ImageLineProbeResult};
use crate::logger;
use crate::storage::{self, StorageStats};
use crate::types::LogMetadata;
#[tauri::command]
#[specta::specta]
#[allow(clippy::needless_pass_by_value)]
#[instrument(level = "error", skip_all)]
pub fn get_config(app: AppHandle) -> Config {
    app.get_config().read().clone()
}

#[tauri::command(async)]
#[specta::specta]
#[allow(clippy::needless_pass_by_value)]
#[instrument(level = "error", skip_all)]
pub fn save_config(app: AppHandle, config: Config) -> CommandResult<()> {
    // 代理地址非法就直接拒绝，不写进配置文件：否则下次启动建客户端会失败
    jm_client::validate_proxy_settings(&config.proxy_mode, &config.proxy_host, config.proxy_port)
        .map_err(|err| CommandError::from("代理设置不合法", err))?;

    let config_state = app.get_config();
    let jm_client = app.get_jm_client();

    let proxy_changed = {
        let config_state = config_state.read();
        config_state.proxy_mode != config.proxy_mode
            || config_state.proxy_host != config.proxy_host
            || config_state.proxy_port != config.proxy_port
    };

    let api_domain_changed = {
        let config_state = config_state.read();
        config_state.api_domain_mode != config.api_domain_mode
            || config_state.custom_api_domain != config.custom_api_domain
    };

    let enable_file_logger = config.enable_file_logger;
    let file_logger_changed = config_state.read().enable_file_logger != enable_file_logger;

    {
        let mut config_state = config_state.write();
        *config_state = config;
        config_state
            .save(&app)
            .map_err(|err| CommandError::from("保存配置失败", err))?;
        tracing::debug!("保存配置成功");
    }

    // 设置里换了线路：清掉运行时换线结果，下一次请求从用户选的那条开始
    if api_domain_changed {
        lines::reset_api_line();
    }

    if proxy_changed {
        jm_client
            .reload_client()
            .map_err(|err| CommandError::from("应用代理设置失败", err))?;
    }

    if file_logger_changed {
        if enable_file_logger {
            logger::reload_file_logger()
                .map_err(|err| CommandError::from("重新加载文件日志失败", err))?;
        } else {
            logger::disable_file_logger()
                .map_err(|err| CommandError::from("禁用文件日志失败", err))?;
        }
    }

    Ok(())
}

/// 探测耗时转毫秒；极端大值按 u64::MAX 处理
fn probe_latency_ms(elapsed: &Duration) -> u64 {
    u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)
}

/// 并发探测所有线路，返回按「可用优先、延迟升序」排好的 (目标, 探测结果)
/// - probe 只负责按目标发一次探测请求，目标本身原样带回，交给调用方拼结果结构体
async fn probe_all_lines<T, F, Fut>(
    client: reqwest::Client,
    targets: Vec<T>,
    probe: F,
) -> Vec<(T, eyre::Result<Duration>)>
where
    T: Clone + Send + 'static,
    F: Fn(reqwest::Client, T) -> Fut + Copy + Send + 'static,
    Fut: Future<Output = eyre::Result<Duration>> + Send + 'static,
{
    let mut join_set = JoinSet::new();
    for target in targets {
        let client = client.clone();
        join_set.spawn(async move {
            let result = probe(client, target.clone()).await;
            (target, result)
        });
    }

    let mut results = Vec::new();
    while let Some(task) = join_set.join_next().await {
        let Ok(item) = task else {
            continue;
        };
        results.push(item);
    }

    results.sort_by_key(|(_, result)| {
        (
            result.is_err(),
            result.as_ref().ok().map_or(u64::MAX, probe_latency_ms),
        )
    });

    results
}

/// 对官方 API 线路做轻量测速，按延迟排序（失败的在最后）
#[tauri::command]
#[specta::specta]
#[instrument(level = "error", skip_all)]
pub async fn probe_api_lines(app: AppHandle) -> CommandResult<Vec<ApiLineProbeResult>> {
    let client = lines::create_probe_client(&app)
        .map_err(|err| CommandError::from("创建测速客户端失败", err))?;

    let custom_api_domain = app.get_config().read().custom_api_domain.clone();
    let mut targets: Vec<(String, ApiDomainMode, String)> = api_line_domains()
        .into_iter()
        .enumerate()
        .map(|(index, (mode, domain))| (format!("线路{}", index + 1), mode, domain.to_string()))
        .collect();
    if !custom_api_domain.is_empty() {
        targets.push(("自定义".to_string(), ApiDomainMode::Custom, custom_api_domain));
    }

    let results = probe_all_lines(client, targets, |client, (_, _, domain)| async move {
        jm_client::probe_api_domain(&client, &domain).await
    })
    .await
    .into_iter()
    .map(|((label, mode, domain), result)| ApiLineProbeResult {
        label,
        mode,
        domain,
        ok: result.is_ok(),
        latency_ms: result.as_ref().ok().map(probe_latency_ms),
        error: result.err().map(|err| err.to_string()),
    })
    .collect();

    Ok(results)
}

/// 对图片线路做轻量测速（图片 CDN 没有可用性接口，这里只测连通性和延迟）
#[tauri::command]
#[specta::specta]
#[instrument(level = "error", skip_all)]
pub async fn probe_image_lines(app: AppHandle) -> CommandResult<Vec<ImageLineProbeResult>> {
    let client = lines::create_probe_client(&app)
        .map_err(|err| CommandError::from("创建测速客户端失败", err))?;

    let targets = lines::IMAGE_LINE_DOMAINS.to_vec();
    let results = probe_all_lines(client, targets, |client, domain| async move {
        jm_client::probe_image_domain(&client, domain).await
    })
    .await
    .into_iter()
    .map(|(domain, result)| ImageLineProbeResult {
        domain: domain.to_string(),
        ok: result.is_ok(),
        latency_ms: result.as_ref().ok().map(probe_latency_ms),
        error: result.err().map(|err| err.to_string()),
    })
    .collect();

    Ok(results)
}

/// 当前正在使用的图片线路（失败会自动切换）
#[tauri::command]
#[specta::specta]
pub fn get_active_image_domain() -> String {
    lines::active_image_domain().to_string()
}

#[allow(clippy::needless_pass_by_value)]
#[tauri::command(async)]
#[specta::specta]
#[instrument(level = "error", skip_all, fields(path = path))]
pub fn show_path_in_file_manager(app: AppHandle, path: &str) -> CommandResult<()> {
    app.opener()
        .reveal_item_in_dir(path)
        .map_err(|err| CommandError::from("在文件管理器中打开失败", err))?;
    Ok(())
}

/// 打开数据目录（config.json 所在的那层）
///
/// 路径一律由后端解析：前端的 appDataDir() 在部分 Tauri 版本上返回的是 %APPDATA% 本身，
/// 少了一层 bundle identifier，打开会落到外层目录。
#[tauri::command(async)]
#[specta::specta]
#[instrument(level = "error", skip_all)]
pub fn show_config_dir_in_file_manager(app: AppHandle) -> CommandResult<()> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|err| CommandError::from("在文件管理器中打开失败", err))?;
    let dir = dir.to_string_lossy().to_string();
    show_path_in_file_manager(app, &dir)
}

/// 空间统计：下载目录 / 导出目录 / 日志目录的占用，以及可以清理的东西
#[tauri::command(async)]
#[specta::specta]
#[instrument(level = "error", skip_all)]
pub fn get_storage_stats(app: AppHandle) -> CommandResult<StorageStats> {
    storage::stats(&app).map_err(|err| CommandError::from("统计空间占用失败", err))
}

/// 清理下载残留（`.下载中-*`），返回释放的字节数
#[tauri::command(async)]
#[specta::specta]
#[instrument(level = "error", skip_all)]
pub fn clean_storage_leftovers(app: AppHandle) -> CommandResult<u64> {
    storage::clean_leftovers(&app).map_err(|err| CommandError::from("清理下载残留失败", err))
}

/// 清理快速阅读器分享包（只删带阅读器标识文件的目录），返回释放的字节数
#[tauri::command(async)]
#[specta::specta]
#[instrument(level = "error", skip_all)]
pub fn clean_storage_quick_readers(app: AppHandle, paths: Vec<String>) -> CommandResult<u64> {
    storage::clean_quick_readers(&app, paths)
        .map_err(|err| CommandError::from("清理快速阅读器分享包失败", err))
}

/// 清理旧日志（保留最近 24 小时内以及最新的一份），返回释放的字节数
#[tauri::command(async)]
#[specta::specta]
#[instrument(level = "error", skip_all)]
pub fn clean_storage_logs(app: AppHandle) -> CommandResult<u64> {
    storage::clean_logs(&app).map_err(|err| CommandError::from("清理旧日志失败", err))
}

/// 删除一本漫画在下载目录或导出目录里的文件夹，返回释放的字节数
#[tauri::command(async)]
#[specta::specta]
#[instrument(level = "error", skip_all, fields(comic_id = comic_id))]
pub fn delete_local_comic(
    app: AppHandle,
    comic_id: i64,
    source: LocalLibrarySource,
) -> CommandResult<u64> {
    storage::delete_comic_dir(&app, comic_id, source)
        .map_err(|err| CommandError::from("删除本地漫画失败", err))
}

#[allow(clippy::needless_pass_by_value)]
#[tauri::command(async)]
#[specta::specta]
#[instrument(level = "error", skip_all, fields(path = path))]
pub fn open_log_file(path: &str) -> CommandResult<Vec<LogMetadata>> {
    let log_file = File::open(path).map_err(|err| CommandError::from("打开日志文件失败", err))?;
    let reader = BufReader::new(log_file);

    let mut logs = Vec::new();

    for (index, line_result) in reader.lines().enumerate() {
        let line_num = index + 1;

        let line = line_result
            .wrap_err(format!("读取日志文件的第`{line_num}`行失败"))
            .map_err(|err| CommandError::from("打开日志文件失败", err))?;

        if line.trim().is_empty() {
            continue;
        }

        // 日志是非阻塞写入的，被强杀时可能留下半行；坏行跳过，
        // 不然一行解析失败整份日志都打不开
        match serde_json::from_str::<LogMetadata>(&line) {
            Ok(log) => logs.push(log),
            Err(err) => tracing::warn!("解析日志第`{line_num}`行失败，已跳过：{err}"),
        }
    }

    Ok(logs)
}
