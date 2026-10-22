use tauri::AppHandle;
use tracing::instrument;

use crate::errors::{CommandError, CommandResult};
use crate::extensions::AppHandleExt;
use crate::favorites_index;
use crate::responses::GetUserProfileRespData;
use crate::types::{NotificationPage, SignInResult, SignStatus, WatchHistoryPage};
#[tauri::command]
#[specta::specta]
#[instrument(level = "error", skip_all)]
pub async fn login(
    app: AppHandle,
    username: String,
    password: String,
) -> CommandResult<GetUserProfileRespData> {
    let jm_client = app.get_jm_client();

    let user_profile = jm_client
        .login(&username, &password)
        .await
        .map_err(|err| CommandError::from("登录失败", err))?;

    Ok(user_profile)
}

/// 退出登录：清掉内存里的登录态（配置里记住的账号密码由前端清空并自动保存）
#[tauri::command]
#[specta::specta]
#[instrument(level = "error", skip_all)]
pub fn logout(app: AppHandle) -> CommandResult<()> {
    app.get_jm_client().logout();
    // 索引文件按账号分，换账号后不能再用缓存里的 uid
    favorites_index::reset_account_uid();
    Ok(())
}

#[tauri::command]
#[specta::specta]
#[instrument(level = "error", skip_all)]
pub async fn get_user_profile(app: AppHandle) -> CommandResult<GetUserProfileRespData> {
    let jm_client = app.get_jm_client();

    let user_profile = jm_client
        .get_user_profile()
        .await
        .map_err(|err| CommandError::from("获取用户信息失败", err))?;

    Ok(user_profile)
}

#[tauri::command]
#[specta::specta]
#[instrument(level = "error", skip_all)]
pub async fn get_sign_status(app: AppHandle) -> CommandResult<SignStatus> {
    let jm_client = app.get_jm_client();

    let status = jm_client
        .get_sign_status()
        .await
        .map_err(|err| CommandError::from("获取签到状态失败", err))?;

    Ok(status)
}

#[tauri::command]
#[specta::specta]
#[instrument(level = "error", skip_all)]
pub async fn sign_in(app: AppHandle) -> CommandResult<SignInResult> {
    let jm_client = app.get_jm_client();

    let result = jm_client
        .sign_in()
        .await
        .map_err(|err| CommandError::from("签到失败", err))?;

    Ok(result)
}

/// 浏览记录（只读）：page 从 1 开始，需要登录态
#[tauri::command]
#[specta::specta]
#[instrument(level = "error", skip_all, fields(page = page))]
pub async fn get_watch_history(app: AppHandle, page: i64) -> CommandResult<WatchHistoryPage> {
    let jm_client = app.get_jm_client();

    jm_client
        .get_watch_history(page)
        .await
        .map_err(|err| CommandError::from("获取浏览记录失败", err))
}

/// 删除一条浏览记录：记录 id 就是漫画 id（官方 /watch_list 的 list 元素 id）
#[tauri::command]
#[specta::specta]
#[instrument(level = "error", skip_all, fields(id = id))]
pub async fn delete_watch_history(app: AppHandle, id: String) -> CommandResult<()> {
    let jm_client = app.get_jm_client();

    jm_client
        .delete_watch_history(&id)
        .await
        .map_err(|err| CommandError::from("删除浏览记录失败", err))
}

/// 官方通知（官方 App 的信箱）：只读列表，已读状态由 mark_notification_read 改
#[tauri::command]
#[specta::specta]
#[instrument(level = "error", skip_all)]
pub async fn get_notifications(app: AppHandle) -> CommandResult<NotificationPage> {
    let jm_client = app.get_jm_client();

    jm_client
        .get_notifications()
        .await
        .map_err(|err| CommandError::from("获取通知失败", err))
}

/// 标记一条通知为已读：官方 POST /notifications，成功后界面的红点随之消失
#[tauri::command]
#[specta::specta]
#[instrument(level = "error", skip_all, fields(id = id))]
pub async fn mark_notification_read(app: AppHandle, id: i64) -> CommandResult<()> {
    let jm_client = app.get_jm_client();

    jm_client
        .mark_notification_read(id)
        .await
        .map_err(|err| CommandError::from("标记通知已读失败", err))
}
