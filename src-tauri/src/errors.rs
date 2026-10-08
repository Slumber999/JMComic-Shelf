use std::panic::Location;

use eyre::EyreHandler;
use serde::Serialize;
use specta::Type;
use tracing::instrument;
use tracing_error::SpanTrace;

pub type CommandResult<T> = Result<T, CommandError>;

#[derive(Debug, Type, Serialize)]
pub struct CommandError {
    pub err_title: String,
    /// 错误码：前端只认它，用它查多语言文案。
    /// 是稳定的英文短标识，跟着接口走——改名字等于改接口。
    pub code: String,
    /// 给人看的错误说明：优先取报错里带中文的那一层原因，没有才退回英文层。
    /// 完整调用链在日志里。
    pub message: String,
}

/// 从 eyre 的完整报告里挑一句「人话」：优先取带中文的那一层原因。
/// reqwest / io 的英文层、Location、SpanTrace 都只留在日志里。
fn human_reason(message: &str) -> Option<String> {
    message
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| {
            // 去掉 Location / SpanTrace / "at src/..." 这些调试段落
            !line.starts_with("Location:")
                && !line.starts_with("SpanTrace:")
                && !line.starts_with("at ")
                && *line != "Error:"
        })
        .filter(|line| line.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)))
        .map(|line| {
            // 去掉 "0: " / "dns error: " 这类前缀，只留后面的中文
            let text = match line.split_once(": ") {
                Some((_, rest)) => rest,
                None => line,
            };
            // 去掉 " (os error 11004)" 这类尾巴
            match text.find(" (os error ") {
                Some(index) => text[..index].to_string(),
                None => text.to_string(),
            }
        })
        .find(|text| !text.is_empty())
}

/// 标题 -> 错误码。标题是给日志看的，错误码是给界面看的。
fn code_for(err_title: &str) -> &'static str {
    match err_title {
        "获取收藏夹失败" => "favorite-fetch",
        "打开日志文件失败" => "log-open-file",
        "打开阅读器失败" => "reader-open",
        "导出指定章节cbz失败" => "export-chapter-cbz",
        "导出cbz失败" => "export-cbz",
        "搜索失败" => "search",
        "获取日志目录大小失败" => "log-dir-size",
        "获取排行榜失败" => "ranking-fetch",
        "同步Comic字段失败" => "sync-comic",
        "获取每周必看失败" => "weekly-fetch",
        "获取漫画信息失败" => "comic-fetch",
        "读取漫画列表失败" => "comic-list-read",
        "创建测速客户端失败" => "probe-client",
        "导出快速阅读器失败" => "export-quick-reader",
        "导出单文件阅读器失败" => "export-single-reader",
        "一键下载漫画失败" => "download-comic",
        "统计空间占用失败" => "storage-stats",
        "删除导出任务失败" => "export-task-delete",
        "删除下载任务失败" => "download-task-delete",
        "收藏/取消收藏失败" => "favorite-toggle",
        "在文件管理器中打开失败" => "open-in-file-manager",
        "应用代理设置失败" => "proxy-apply",
        "移动收藏夹失败" => "favorite-move",
        "同步ComicInFavorite字段失败" => "sync-favorite-fields",
        "同步ComicInSearch字段失败" => "sync-search-fields",
        "删除本地漫画失败" => "local-comic-delete",
        "同步ComicInWeekly字段失败" => "sync-weekly-fields",
        "同步收藏夹失败" => "favorite-sync",
        "清理下载残留失败" => "clean-leftovers",
        "保存配置失败" => "config-save",
        "清理旧日志失败" => "clean-logs",
        "创建下载任务失败" => "download-task-create",
        "打开阅读窗口失败" => "reader-window-open",
        "代理设置不合法" => "proxy-invalid",
        "导出指定章节pdf失败" => "export-chapter-pdf",
        "导出pdf失败" => "export-pdf",
        "登录失败" => "login",
        "恢复下载任务失败" => "download-task-resume",
        "获取分类失败" => "category-fetch",
        "获取每周必看信息失败" => "weekly-info-fetch",
        "获取评论失败" => "comment-fetch",
        "暂停下载任务失败" => "download-task-pause",
        "获取用户信息失败" => "user-info-fetch",
        "继续导出任务失败" => "export-task-resume",
        "加载章节失败" => "chapter-load",
        "禁用文件日志失败" => "file-log-disable",
        "清理快速阅读器分享包失败" => "clean-quick-readers",
        "重新加载文件日志失败" => "file-log-reload",
        _ => "unknown",
    }
}

impl CommandError {
    pub fn from<E>(err_title: &str, err: E) -> Self
    where
        E: Into<eyre::Report>,
    {
        let message = format!("{:?}", err.into());
        // 日志里保留完整调用链；code 供前端查多语言文案
        let code = code_for(err_title);
        tracing::error!(err_title, code, message);
        let brief = message
            .lines()
            .find_map(|line| line.trim().strip_prefix("0: "))
            .unwrap_or(&message)
            .to_string();
        // 界面上只给人话：有中文层就用中文层，没有才退回英文的第一层原因
        let human = human_reason(&message).unwrap_or(brief);
        Self {
            err_title: err_title.to_string(),
            code: code.to_string(),
            message: human,
        }
    }
}

struct CustomEyreHandler {
    span_trace: SpanTrace,
    location: Option<&'static Location<'static>>,
}

impl EyreHandler for CustomEyreHandler {
    fn debug(
        &self,
        error: &(dyn std::error::Error + 'static),
        f: &mut std::fmt::Formatter<'_>,
    ) -> std::fmt::Result {
        use std::fmt::Write;

        let mut buf = String::new();

        writeln!(&mut buf, "Error:")?;
        writeln!(&mut buf, "   0: {error}")?;

        let mut current = error.source();
        let mut i = 1;
        while let Some(cause) = current {
            writeln!(&mut buf, "   {i}: {cause}")?;
            current = cause.source();
            i += 1;
        }

        if let Some(location) = self.location {
            writeln!(&mut buf, "Location:")?;
            writeln!(&mut buf, "   at {}:{}", location.file(), location.line())?;
        }

        let span_trace = format!("{}", self.span_trace);
        if !span_trace.is_empty() {
            writeln!(&mut buf, "SpanTrace:")?;
            writeln!(&mut buf, "{span_trace}")?;
        }

        write!(f, "{}", buf.trim_end())?;

        Ok(())
    }

    fn track_caller(&mut self, location: &'static Location<'static>) {
        self.location = Some(location);
    }
}

#[instrument(level = "error", skip_all)]
pub fn install_custom_eyre_handler() -> eyre::Result<()> {
    eyre::set_hook(Box::new(|_error| {
        Box::new(CustomEyreHandler {
            span_trace: SpanTrace::capture(),
            location: None,
        })
    }))?;
    Ok(())
}