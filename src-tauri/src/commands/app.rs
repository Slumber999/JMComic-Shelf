use tauri::AppHandle;

/// 根屏「再按一次退出」由前端直接调用，不需要额外的 JS↔Kotlin 桥。
#[tauri::command]
#[specta::specta]
#[allow(clippy::needless_pass_by_value)]
pub fn exit_app(app: AppHandle) {
    app.exit(0);
}
