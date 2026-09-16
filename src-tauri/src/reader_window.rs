//! 阅读器独立窗口：全局最多一个，主窗口可随时切换它显示的漫画

use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::{errors::{CommandError, CommandResult}, types::Comic};

/// 阅读窗口的标签：标签唯一，所以最多只会存在一个阅读窗口
pub const READER_WINDOW_LABEL: &str = "reader";
/// 主窗口切换阅读漫画时推给阅读窗口的事件名
pub const READER_WINDOW_TARGET_EVENT: &str = "reader-window-target";

/// 阅读窗口要显示的漫画：传 comic 表示本地库存，只传 comic_id 表示从网络读
#[derive(Default, Debug, Clone, PartialEq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ReaderWindowTarget {
    pub comic: Option<Comic>,
    pub comic_id: Option<i64>,
}

/// 主窗口最后一次指定的目标：阅读窗口刚建好时用它取初始值
#[derive(Default)]
pub struct ReaderWindowTargetState(RwLock<Option<ReaderWindowTarget>>);

#[tauri::command(async)]
#[specta::specta]
pub fn open_reader_window(app: AppHandle, target: ReaderWindowTarget) -> CommandResult<()> {
    *app.state::<ReaderWindowTargetState>().0.write() = Some(target.clone());

    // 已经开着阅读窗口就复用：聚焦 + 通知它换漫画
    if let Some(window) = app.get_webview_window(READER_WINDOW_LABEL) {
        let _ = window.set_focus();
        let _ = app.emit_to(READER_WINDOW_LABEL, READER_WINDOW_TARGET_EVENT, target);
        return Ok(());
    }

    // 阅读窗口默认和主窗口一样大
    let (width, height) = main_window_size(&app);

    WebviewWindowBuilder::new(&app, READER_WINDOW_LABEL, WebviewUrl::App("index.html".into()))
        .title("禁漫书架 · 阅读")
        .inner_size(width, height)
        .min_inner_size(420.0, 500.0)
        .build()
        .map_err(|err| CommandError::from("打开阅读窗口失败", err))?;

    Ok(())
}

/// 主窗口当前的逻辑尺寸；拿不到就用默认值
fn main_window_size(app: &AppHandle) -> (f64, f64) {
    let Some(main_window) = app.get_webview_window("main") else {
        return (600.0, 720.0);
    };
    let Ok(size) = main_window.inner_size() else {
        return (600.0, 720.0);
    };
    let scale_factor = main_window.scale_factor().unwrap_or(1.0);
    (
        f64::from(size.width) / scale_factor,
        f64::from(size.height) / scale_factor,
    )
}

/// 阅读窗口启动时取初始目标（主窗口发事件时它可能还没注册监听）
#[tauri::command(async)]
#[specta::specta]
pub fn get_reader_window_target(app: AppHandle) -> Option<ReaderWindowTarget> {
    app.state::<ReaderWindowTargetState>().0.read().clone()
}
