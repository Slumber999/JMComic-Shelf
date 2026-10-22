use std::path::PathBuf;
use std::time::Duration;

use eyre::{eyre, WrapErr};
use tauri::AppHandle;
use tokio::time::sleep;
use tracing::instrument;

use crate::config::{ExportSkipMode, LocalLibrarySource};
use crate::errors::{CommandError, CommandResult};
use crate::export::manager::{ExportTaskKind, ExportTaskState};
use crate::extensions::{AppHandleExt, EyreReportToMessage};
use crate::local_index;
use crate::quick_reader::QuickReaderCandidate;
use crate::types::Comic;
use crate::{export, utils};
/// 导出目录的「更新库存」：去接口拉每本导出漫画的最新章节，只补导还没导出过的那些
/// - 返回补导了几本漫画
#[tauri::command(async)]
#[specta::specta]
#[instrument(level = "error", skip_all)]
pub async fn update_exported_comics(app: AppHandle) -> CommandResult<u32> {
    let interval_sec = app.get_config().read().update_downloaded_comics_interval_sec;
    let exported_comics = local_index::export_comics(&app);
    if exported_comics.is_empty() {
        return Ok(0);
    }

    let mut updated = 0;
    for (comic_id, comic_title, _) in exported_comics {
        let comic = match utils::get_comic(app.clone(), comic_id).await {
            Ok(comic) => comic,
            Err(err) => {
                let err_title = format!("更新导出目录过程中，获取漫画`{comic_title}`失败，已跳过");
                let err = err.wrap_err("可能是频率太高，请手动去`配置`里调整`更新库存时，每处理完一个已下载的漫画后休息`");
                tracing::error!(err_title, message = err.to_message());
                sleep(Duration::from_secs(interval_sec)).await;
                continue;
            }
        };

        match export::export_missing_chapters(&app, &comic).await {
            Ok(true) => updated += 1,
            Ok(false) => {}
            Err(err) => {
                let err_title = format!("漫画`{comic_title}`补导章节失败");
                tracing::error!(err_title, message = err.to_message());
            }
        }

        sleep(Duration::from_secs(interval_sec)).await;
    }

    Ok(updated)
}

/// 导出「单文件 HTML」快速阅读器：图片以 base64 内嵌，一个文件就能读
/// - 手机浏览器禁止 file:// 页面读取子目录图片，只有内嵌才能在手机上直接打开
/// - split_by_chapter = true 时每章一个文件
#[allow(clippy::needless_pass_by_value)]
#[tauri::command(async)]
#[specta::specta]
#[instrument(level = "error", skip_all, fields(source = ?source, target_dir = ?target_dir, split_by_chapter = split_by_chapter))]
pub async fn export_quick_reader_single_file(
    app: AppHandle,
    source: LocalLibrarySource,
    keys: Vec<String>,
    target_dir: Option<String>,
    split_by_chapter: bool,
) -> CommandResult<Vec<PathBuf>> {
    let app_for_task = app.clone();
    let files = tauri::async_runtime::spawn_blocking(move || {
        crate::quick_reader::export_quick_reader_single_file(
            &app_for_task,
            source,
            keys,
            target_dir,
            split_by_chapter,
        )
    })
    .await
    .map_err(|err| CommandError::from("导出单文件阅读器失败", eyre::Report::from(err)))?
    .map_err(|err| CommandError::from("导出单文件阅读器失败", err))?;

    tracing::debug!("单文件阅读器已导出 {} 个文件", files.len());
    Ok(files)
}

/// 列出某个目录里可以导出进快速阅读器的漫画（给前端勾选）
#[allow(clippy::needless_pass_by_value)]
#[tauri::command(async)]
#[specta::specta]
#[instrument(level = "error", skip_all, fields(source = ?source))]
pub async fn list_quick_reader_candidates(
    app: AppHandle,
    source: LocalLibrarySource,
) -> CommandResult<Vec<QuickReaderCandidate>> {
    let app_for_task = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::quick_reader::list_candidates(&app_for_task, source)
    })
    .await
    .map_err(|err| CommandError::from("读取漫画列表失败", eyre::Report::from(err)))?
    .map_err(|err| CommandError::from("读取漫画列表失败", err))
}

/// 导出一份可以在浏览器里直接打开的快速阅读器（分享包）
/// - source = 从哪个目录收集漫画（导出目录 / 下载目录）
/// - keys = 选中的漫画（相对来源目录的路径）；为空表示全部
/// - dir_name = 阅读器文件夹名，重名会自动加 -2、-3 后缀
/// - target_dir = 写到哪个目录（绝对路径）；为空则写到来源目录
#[allow(clippy::needless_pass_by_value)]
#[tauri::command(async)]
#[specta::specta]
#[instrument(level = "error", skip_all, fields(source = ?source, dir_name = ?dir_name, target_dir = ?target_dir))]
pub async fn export_quick_reader(
    app: AppHandle,
    source: LocalLibrarySource,
    keys: Vec<String>,
    dir_name: Option<String>,
    target_dir: Option<String>,
) -> CommandResult<PathBuf> {
    let app_for_task = app.clone();
    let dir = tauri::async_runtime::spawn_blocking(move || {
        crate::quick_reader::export_quick_reader(&app_for_task, source, keys, dir_name, target_dir)
    })
    .await
    .map_err(|err| CommandError::from("导出快速阅读器失败", eyre::Report::from(err)))?
    .map_err(|err| CommandError::from("导出快速阅读器失败", err))?;

    tracing::debug!("快速阅读器已导出到 {}", dir.display());
    Ok(dir)
}

#[tauri::command(async)]
#[specta::specta]
#[allow(clippy::needless_pass_by_value)]
#[instrument(level = "error", skip_all, fields(comic_id = comic.id, comic_title = comic.name))]
pub fn export_cbz(app: AppHandle, comic: Comic) -> CommandResult<()> {
    let skip_mode = app.get_config().read().export_skip_mode;
    let total = u32::try_from(
        comic
            .chapter_infos
            .iter()
            .filter(|chapter| chapter.is_downloaded.unwrap_or(false))
            .count(),
    )
    .unwrap_or(u32::MAX);
    let comic_export_dir = comic
        .get_comic_export_dir(&app)
        .map_err(|err| CommandError::from("导出cbz失败", err))?;

    let task = app.get_export_manager().create_task(
        uuid::Uuid::new_v4().to_string(),
        comic.id,
        comic.name.clone(),
        ExportTaskKind::Cbz,
        total,
        comic_export_dir,
    );

    export::cbz(&app, &comic, &task, skip_mode).map_err(|err| {
        task.set_state(ExportTaskState::Failed);
        CommandError::from("导出cbz失败", err)
    })?;

    Ok(())
}

#[tauri::command(async)]
#[specta::specta]
#[allow(clippy::needless_pass_by_value)]
#[instrument(level = "error", skip_all, fields(comic_id = comic.id, comic_title = comic.name))]
pub fn export_pdf(app: AppHandle, comic: Comic) -> CommandResult<()> {
    export::pdf(&app, &comic).map_err(|err| CommandError::from("导出pdf失败", err))?;
    Ok(())
}

#[tauri::command(async)]
#[specta::specta]
#[allow(clippy::needless_pass_by_value)]
pub fn export_cbz_chapters(
    app: AppHandle,
    comic: Comic,
    chapter_ids: Vec<i64>,
) -> CommandResult<()> {
    let comic_title = comic.name.clone();
    let total = u32::try_from(
        comic
            .chapter_infos
            .iter()
            .filter(|chapter| {
                chapter.is_downloaded.unwrap_or(false) && chapter_ids.contains(&chapter.chapter_id)
            })
            .count(),
    )
    .unwrap_or(u32::MAX);
    let comic_export_dir = comic
        .get_comic_export_dir(&app)
        .map_err(|err| CommandError::from("导出指定章节cbz失败", err))?;

    let task = app.get_export_manager().create_task(
        uuid::Uuid::new_v4().to_string(),
        comic.id,
        comic_title.clone(),
        ExportTaskKind::Cbz,
        total,
        comic_export_dir,
    );

    export::cbz_chapters(&app, &comic, chapter_ids, &task, ExportSkipMode::None)
        .wrap_err(format!("漫画`{comic_title}`导出指定章节cbz失败"))
        .map_err(|err| {
            task.set_state(ExportTaskState::Failed);
            CommandError::from("导出指定章节cbz失败", err)
        })?;
    Ok(())
}

#[tauri::command(async)]
#[specta::specta]
#[allow(clippy::needless_pass_by_value)]
pub fn export_pdf_chapters(
    app: AppHandle,
    comic: Comic,
    chapter_ids: Vec<i64>,
) -> CommandResult<()> {
    let comic_title = comic.name.clone();
    export::pdf_chapters(&app, &comic, chapter_ids)
        .wrap_err(format!("漫画`{comic_title}`导出指定章节pdf失败"))
        .map_err(|err| CommandError::from("导出指定章节pdf失败", err))?;
    Ok(())
}


/// 暂停导出任务：当前章节做完后停下，任务留在暂停状态
#[tauri::command(async)]
#[specta::specta]
#[instrument(level = "error", skip_all, fields(uuid = uuid))]
pub fn pause_export_task(app: AppHandle, uuid: String) -> CommandResult<()> {
    let Some(task) = app.get_export_manager().get(&uuid) else {
        return Err(CommandError::from(
            "暂停导出任务失败",
            eyre!("未找到导出任务 {uuid}"),
        ));
    };

    task.set_state(ExportTaskState::Paused);
    Ok(())
}

/// 继续导出任务：用「跳过已存在」重新跑一遍，已经导完的章节会跳过
#[tauri::command(async)]
#[specta::specta]
#[instrument(level = "error", skip_all, fields(uuid = uuid))]
pub async fn resume_export_task(app: AppHandle, uuid: String) -> CommandResult<()> {
    let Some(task) = app.get_export_manager().get(&uuid) else {
        return Err(CommandError::from(
            "继续导出任务失败",
            eyre!("未找到导出任务 {uuid}"),
        ));
    };

    task.set_state(ExportTaskState::Exporting);

    match task.kind {
        ExportTaskKind::Cbz => {
            let Some(comic) = load_local_comic(&app, task.comic_id) else {
                task.set_state(ExportTaskState::Failed);
                return Err(CommandError::from(
                    "继续导出任务失败",
                    eyre!("本地找不到这本漫画的元数据，无法继续导出"),
                ));
            };

            tauri::async_runtime::spawn_blocking(move || {
                if let Err(err) = export::cbz(&app, &comic, &task, ExportSkipMode::SkipExisting) {
                    let err_title = "继续导出cbz失败";
                    let message = err.to_message();
                    tracing::error!(err_title, message);
                    task.set_state(ExportTaskState::Failed);
                }
            });
        }
        ExportTaskKind::CbzDirect => {
            let comic = utils::get_comic(app.clone(), task.comic_id)
                .await
                .map_err(|err| {
                    task.set_state(ExportTaskState::Failed);
                    CommandError::from("继续导出任务失败", err)
                })?;

            tauri::async_runtime::spawn(async move {
                if let Err(err) = export::resume_comic_cbz(&app, &comic, &task).await {
                    let err_title = "继续导出cbz失败";
                    let message = err.to_message();
                    tracing::error!(err_title, message);
                    task.set_state(ExportTaskState::Failed);
                }
            });
        }
    }

    Ok(())
}

/// 删除导出任务；delete_files 为 true 时连这本漫画的导出目录一起删掉
#[tauri::command(async)]
#[specta::specta]
#[instrument(level = "error", skip_all, fields(uuid = uuid))]
pub fn delete_export_task(app: AppHandle, uuid: String, delete_files: bool) -> CommandResult<()> {
    app.get_export_manager()
        .delete_task(&uuid, delete_files)
        .map_err(|err| CommandError::from("删除导出任务失败", err))?;
    Ok(())
}

/// 前端挂载后同步一次导出任务，把恢复出来的任务补进列表
#[tauri::command(async)]
#[specta::specta]
pub fn sync_export_tasks(app: AppHandle) {
    app.get_export_manager().sync_tasks();
}

/// 从本地下载目录的元数据里还原 Comic（继续导出时用）
fn load_local_comic(app: &AppHandle, comic_id: i64) -> Option<Comic> {
    let (comic_download_dir, _comic_title) = local_index::comic_dir_and_name(app, comic_id)?;
    Comic::from_metadata(&comic_download_dir.join("元数据.json")).ok()
}

#[allow(clippy::needless_pass_by_value)]
#[tauri::command(async)]
#[specta::specta]
#[instrument(level = "error", skip_all)]
pub async fn export_cbz_without_download(
    app: AppHandle,
    comic_ids: Vec<i64>,
) -> CommandResult<()> {
    export::export_cbz_without_download(app, comic_ids)
        .await
        .map_err(|err| CommandError::from("导出cbz失败", err))?;
    Ok(())
}

/// 章节详情页的「免下载直出」：只导出选中的章节，不先下载到下载目录
#[allow(clippy::needless_pass_by_value)]
#[tauri::command(async)]
#[specta::specta]
#[instrument(level = "error", skip_all, fields(comic_id = comic.id, chapters = chapter_ids.len()))]
pub async fn export_cbz_chapters_without_download(
    app: AppHandle,
    comic: Comic,
    chapter_ids: Vec<i64>,
) -> CommandResult<()> {
    export::export_cbz_chapters_without_download(app, comic, chapter_ids)
        .await
        .map_err(|err| CommandError::from("导出指定章节cbz失败", err))?;
    Ok(())
}
