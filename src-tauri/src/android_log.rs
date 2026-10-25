/// Android 上进程内 stdout 会被丢掉：诊断/测量行统一写 logcat（tag 用来区分模块）。
/// 桌面端此函数不做任何事（开发时看终端日志即可）。
pub fn log_line(tag: &str, line: &str) {
    #[cfg(target_os = "android")]
    {
        use std::process::{Command, Stdio};
        let _ = Command::new("/system/bin/log")
            .args(["-t", tag, "-p", "i", line])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
    }
    #[cfg(not(target_os = "android"))]
    {
        let _ = (tag, line);
    }
}