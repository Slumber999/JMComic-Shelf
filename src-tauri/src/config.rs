use std::{
    io::Write,
    path::{Path, PathBuf},
};

use crate::types::{DownloadFormat, ProxyMode};
use eyre::WrapErr;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Manager};

/// 官方 API 线路：顺序与设置页的「线路1..线路5」一致
pub fn api_line_domains() -> [(ApiDomainMode, &'static str); 5] {
    [
        (ApiDomainMode::Domain1, API_DOMAIN_1),
        (ApiDomainMode::Domain2, API_DOMAIN_2),
        (ApiDomainMode::Domain3, API_DOMAIN_3),
        (ApiDomainMode::Domain4, API_DOMAIN_4),
        (ApiDomainMode::Domain5, API_DOMAIN_5),
    ]
}

const API_DOMAIN_1: &str = "www.cdnzack.cc";
const API_DOMAIN_2: &str = "www.cdnhth.cc";
const API_DOMAIN_3: &str = "www.cdnhth.net";
const API_DOMAIN_4: &str = "www.cdnbea.net";
const API_DOMAIN_5: &str = "www.cdn-mspjmapiproxy.xyz";

#[derive(Debug, Clone, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    pub username: String,
    pub password: String,
    /// 启动时是否自动用记住的账号登录
    pub auto_login: bool,
    pub download_dir: PathBuf,
    pub export_dir: PathBuf,
    pub download_format: DownloadFormat,
    pub dir_fmt: String,
    pub proxy_mode: ProxyMode,
    pub proxy_host: String,
    pub proxy_port: u16,
    pub enable_file_logger: bool,
    pub chapter_concurrency: usize,
    pub chapter_download_interval_sec: u64,
    pub img_concurrency: usize,
    pub img_download_interval_sec: u64,
    pub download_all_favorites_interval_sec: u64,
    pub update_downloaded_comics_interval_sec: u64,
    pub api_domain_mode: ApiDomainMode,
    pub custom_api_domain: String,
    pub should_download_cover: bool,
    /// 阅读器是否拆分成独立窗口
    pub split_reader: bool,
    /// 主窗口尺寸：换网格档位、用户拉伸窗口时由前端更新
    pub window_width: u32,
    pub window_height: u32,
    pub create_pdf_concurrency: usize,
    pub enable_merge_pdf: bool,
    /// 导出跳过模式
    pub export_skip_mode: ExportSkipMode,
    /// 界面语言
    pub language: Language,
}

impl Config {
    pub fn new(app: &AppHandle) -> eyre::Result<Self> {
        let app_data_dir = app.path().app_data_dir()?;
        let config_path = app_data_dir.join("config.json");

        let config = if config_path.exists() {
            match std::fs::read_to_string(&config_path) {
                Ok(config_string) => match serde_json::from_str(&config_string) {
                    // 如果能够直接解析为Config，则直接返回
                    Ok(config) => config,
                    // 字段缺失还能当版本升级处理；只有 JSON 本身解析不出来才算坏文件
                    Err(_) => match Config::try_merge_config(&config_string, &app_data_dir) {
                        Some(config) => config,
                        None => Config::recover_from_broken_file(&config_path, &app_data_dir),
                    },
                },
                // 读不出来（权限、编码、半截文件）同样按坏文件处理
                Err(err) => {
                    tracing::error!(message = %err, path = %config_path.display(), "读取配置文件失败");
                    Config::recover_from_broken_file(&config_path, &app_data_dir)
                }
            }
        } else {
            Config::default(&app_data_dir)
        };
        config.save(app)?;
        Ok(config)
    }

    pub fn save(&self, app: &AppHandle) -> eyre::Result<()> {
        let resource_dir = app.path().app_data_dir()?;
        let config_path = resource_dir.join("config.json");
        let config_string = serde_json::to_string_pretty(self)?;
        write_file_atomic(&config_path, &config_string)?;
        Ok(())
    }

    /// JSON 彻底坏掉时先留一份 `config.json.bak` 再回落默认值，免得下次保存把原文件直接覆盖掉
    fn recover_from_broken_file(config_path: &Path, app_data_dir: &Path) -> Config {
        let backup_path = app_data_dir.join("config.json.bak");
        // Windows 上 rename 不覆盖已存在的目标
        if backup_path.exists() {
            let _ = std::fs::remove_file(&backup_path);
        }
        // 这一步发生在 logger::init 之前，tracing 还没有 subscriber，所以同时打到 stderr
        match std::fs::rename(config_path, &backup_path) {
            Ok(()) => {
                eprintln!(
                    "[config] 配置文件解析失败，已备份为 {}，本次使用默认配置",
                    backup_path.display()
                );
                tracing::warn!(
                    path = %backup_path.display(),
                    "配置文件解析失败，已备份坏文件并回落到默认配置"
                )
            }
            Err(err) => {
                eprintln!(
                    "[config] 配置文件解析失败且备份失败（{}），本次使用默认配置",
                    err
                );
                tracing::warn!(
                    message = %err,
                    path = %config_path.display(),
                    "配置文件解析失败且备份失败，已回落到默认配置"
                )
            }
        }
        Config::default(app_data_dir)
    }

    pub fn get_api_domain(&self) -> String {
        match self.api_domain_mode {
            ApiDomainMode::Domain1 => API_DOMAIN_1.to_string(),
            ApiDomainMode::Domain2 => API_DOMAIN_2.to_string(),
            ApiDomainMode::Domain3 => API_DOMAIN_3.to_string(),
            ApiDomainMode::Domain4 => API_DOMAIN_4.to_string(),
            ApiDomainMode::Domain5 => API_DOMAIN_5.to_string(),
            ApiDomainMode::Custom => self.custom_api_domain.clone(),
        }
    }

    /// 缺失字段补默认值；返回 None 表示这份 JSON 本身已经读不出来了（调用方要备份坏文件）
    fn try_merge_config(config_string: &str, app_data_dir: &Path) -> Option<Config> {
        let mut json_value = serde_json::from_str::<serde_json::Value>(config_string).ok()?;
        let serde_json::Value::Object(ref mut map) = json_value else {
            return None;
        };
        let default_config_value = serde_json::to_value(Config::default(app_data_dir)).ok()?;
        let serde_json::Value::Object(default_map) = default_config_value else {
            return None;
        };
        for (key, value) in default_map {
            map.entry(key).or_insert(value);
        }
        serde_json::from_value(json_value).ok()
    }

    fn default(app_data_dir: &Path) -> Config {
        let cpu_core_num = std::thread::available_parallelism()
            .map(std::num::NonZero::get)
            .unwrap_or(1);

        Config {
            username: String::new(),
            password: String::new(),
            auto_login: true,
            download_dir: app_data_dir.join("漫画下载"),
            export_dir: app_data_dir.join("漫画导出"),
            download_format: DownloadFormat::default(),
            dir_fmt: "{comic_title}/{chapter_title}".to_string(),
            proxy_mode: ProxyMode::default(),
            proxy_host: "127.0.0.1".to_string(),
            proxy_port: 7890,
            enable_file_logger: true,
            chapter_concurrency: 3,
            chapter_download_interval_sec: 0,
            img_concurrency: 20,
            img_download_interval_sec: 0,
            download_all_favorites_interval_sec: 0,
            update_downloaded_comics_interval_sec: 0,
            api_domain_mode: ApiDomainMode::Domain2,
            custom_api_domain: API_DOMAIN_2.to_string(),
            should_download_cover: true,
            split_reader: false,
            window_width: 600,
            window_height: 720,
            create_pdf_concurrency: cpu_core_num,
            enable_merge_pdf: true,
            export_skip_mode: ExportSkipMode::default(),
            language: Language::default(),
        }
    }
}

/// 先写 `*.tmp` 再改名：写到一半崩溃不会把原文件截成半个 JSON
fn write_file_atomic(path: &Path, contents: &str) -> eyre::Result<()> {
    let tmp_path = path.with_extension("json.tmp");
    {
        let mut file = std::fs::File::create(&tmp_path)
            .wrap_err(format!("创建文件`{}`失败", tmp_path.display()))?;
        file.write_all(contents.as_bytes())
            .wrap_err(format!("写入`{}`失败", tmp_path.display()))?;
        // 先刷盘再改名，否则断电后可能留下一个空文件
        file.sync_all()
            .wrap_err(format!("同步`{}`失败", tmp_path.display()))?;
    }
    // Windows 上 rename 不覆盖已存在的目标
    if path.exists() {
        std::fs::remove_file(path).wrap_err(format!("删除旧文件`{}`失败", path.display()))?;
    }
    std::fs::rename(&tmp_path, path).wrap_err(format!(
        "将`{}`重命名为`{}`失败",
        tmp_path.display(),
        path.display()
    ))?;
    Ok(())
}

/// 界面语言：值与前端 i18n 的 Locale 一一对应
#[derive(Default, Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Type)]
pub enum Language {
    #[default]
    #[serde(rename = "zh-CN")]
    ZhCn,
    #[serde(rename = "zh-TW")]
    ZhTw,
}

#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
pub enum ApiDomainMode {
    Domain1,
    #[default]
    Domain2,
    Domain3,
    Domain4,
    Domain5,
    Custom,
}

/// 本地库存读取的目录
/// - 注意：这只是"看哪个目录"的视图偏好，由前端存在 localStorage 里，
///   不再是 Config 的一部分（切一下目录不该触发保存配置）
#[derive(Default, Debug, Copy, Clone, PartialEq, Serialize, Deserialize, Type)]
pub enum LocalLibrarySource {
    /// 下载目录（正常下载的漫画）
    #[default]
    DownloadDir,
    /// 导出目录（只导出过 cbz、没有下载过的漫画）
    ExportDir,
}

/// 导出跳过模式
#[derive(Default, Debug, Copy, Clone, PartialEq, Serialize, Deserialize, Type)]
pub enum ExportSkipMode {
    /// 每次重新导出所有章节
    #[default]
    None,
    /// 跳过本地已存在的导出文件
    SkipExisting,
    /// 跳过曾导出过的章节（即使本地文件已删除）
    SkipExported,
}
#[cfg(test)]
mod tests {
    use super::*;

    /// 用例独立的临时目录：用完删掉，避免污染别的测试
    fn temp_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "jmcomic-shelf-config-test-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn write_file_atomic_should_write_content_and_leave_no_tmp() {
        let dir = temp_dir();
        let path = dir.join("config.json");

        write_file_atomic(&path, "{\"a\":1}").unwrap();

        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{\"a\":1}");
        assert!(!path.with_extension("json.tmp").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_file_atomic_should_replace_existing_file() {
        let dir = temp_dir();
        let path = dir.join("config.json");
        std::fs::write(&path, "old").unwrap();

        write_file_atomic(&path, "new").unwrap();

        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
        assert!(!path.with_extension("json.tmp").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn try_merge_config_should_fill_defaults_but_reject_broken_json() {
        let app_data_dir = temp_dir();

        // 只缺字段：补默认值，用户的已有配置保留
        let merged = Config::try_merge_config("{\"username\":\"alice\"}", &app_data_dir)
            .expect("缺字段应该能合并");
        assert_eq!(merged.username, "alice");
        assert_eq!(merged.img_concurrency, Config::default(&app_data_dir).img_concurrency);

        // 连 JSON 都不是：交给调用方备份坏文件
        assert!(Config::try_merge_config("not json", &app_data_dir).is_none());
        assert!(Config::try_merge_config("[1,2,3]", &app_data_dir).is_none());

        let _ = std::fs::remove_dir_all(&app_data_dir);
    }
}
