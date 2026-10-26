//! panic 兜底：把 panic 信息与 backtrace 直接追加写到日志目录的 crash-rust.log。
//! release 是 panic=abort，tracing 的异步文件日志可能来不及落盘，所以这里同步写文件。

use std::{
    backtrace::Backtrace,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::OnceLock,
    time::{SystemTime, UNIX_EPOCH},
};

/// 日志目录（`<app_data>/日志`，与 logger 同一处）：logger 初始化前为空，此时只打 logcat。
static LOG_DIR: OnceLock<PathBuf> = OnceLock::new();

pub fn set_log_dir(dir: PathBuf) {
    let _ = LOG_DIR.set(dir);
}

/// 安装全局 panic hook：先落盘 + 打 logcat，再交回原 hook（桌面仍会打印到 stderr）。
pub fn install() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let thread = std::thread::current();
        let thread_name = thread.name().unwrap_or("<unnamed>").to_string();
        let location = info.location().map_or_else(
            || "<unknown>".to_string(),
            |location| {
                format!(
                    "{}:{}:{}",
                    location.file(),
                    location.line(),
                    location.column()
                )
            },
        );
        let payload = if let Some(text) = info.payload().downcast_ref::<&str>() {
            (*text).to_string()
        } else if let Some(text) = info.payload().downcast_ref::<String>() {
            text.clone()
        } else {
            "<non-string panic payload>".to_string()
        };
        // 没有 chrono 依赖：用 unix 毫秒，精确到毫秒足够定位两次崩溃的先后
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_millis());
        let record = format!(
            "===== unix_ms={millis} thread={thread_name} at {location} =====\n{payload}\n{}\n",
            Backtrace::force_capture()
        );

        if let Some(dir) = LOG_DIR.get() {
            if let Err(err) = write_record(&dir.join("crash-rust.log"), &record) {
                eprintln!("写 panic 日志失败: {err}");
            }
        }
        // logcat 单条长度有限：只打位置和首行，完整栈在 crash-rust.log
        crate::android_log::log_line(
            "JMCRASH",
            &format!("Rust panic thread={thread_name} at {location}: {payload}"),
        );
        // 顺便进 tracing：桌面看终端，移动端进日常日志文件（异步，abort 前可能丢）
        tracing::error!(message = %payload, %location, thread = %thread_name, "Rust panic（进程即将退出）");

        default_hook(info);
    }));
}

fn write_record(path: &Path, record: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = OpenOptions::new().create(true).append(true).open(path)?;
    file.write_all(record.as_bytes())?;
    file.flush()
}
