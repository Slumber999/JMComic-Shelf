use events::{
    DownloadAllFavoritesEvent, DownloadEvent, ExportCbzEvent, ExportPdfEvent, ExportTaskEvent,
    ExportQuickReaderEvent, LogEvent, UpdateDownloadedComicsEvent,
};
use eyre::WrapErr;
use parking_lot::RwLock;
use tauri::{Manager, Wry};

// TODO: 用prelude来消除警告
use crate::commands::*;
use crate::config::Config;
use crate::downloader::download_manager::DownloadManager;
use crate::errors::install_custom_eyre_handler;
use crate::export::manager::ExportManager;
use crate::export::ComicExportLock;
use crate::jm_client::JmClient;
use crate::reader::ReaderState;

mod commands;
mod config;
mod covers;
mod downloader;
mod errors;
mod events;
mod export;
mod extensions;
mod jm_client;
mod lines;
mod local_index;
mod logger;
mod quick_reader;
mod reader;
mod responses;
mod storage;
mod types;
mod utils;

fn generate_context() -> tauri::Context<Wry> {
    tauri::generate_context!()
}

// TODO: 添加Panic Doc
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 安装失败（比如已经装过）不应该让整个应用起不来，记一条日志继续跑
    if let Err(err) = install_custom_eyre_handler() {
        eprintln!("安装自定义错误处理器失败，继续启动: {err:?}");
        tracing::error!(message = %err, "安装自定义错误处理器失败，继续启动");
    }

    let builder = tauri_specta::Builder::<Wry>::new()
        .commands(tauri_specta::collect_commands![
            greet,
            get_config,
            save_config,
            login,
            search,
            get_ranking,
            toggle_favorite,
            get_favorite_folders,
            move_favorite_to_folder,
            get_categories,
            probe_api_lines,
            probe_image_lines,
            get_active_image_domain,
            get_comic,
            get_favorite_folder,
            get_all_favorite_comics,
            get_weekly_info,
            get_weekly,
            get_user_profile,
            create_download_task,
            create_download_tasks,
            pause_download_task,
            resume_download_task,
            delete_download_task,
            sync_download_tasks,
            download_comic,
            download_all_favorites,
            update_downloaded_comics,
            show_path_in_file_manager,
            sync_favorite_folder,
            open_comic_reader,
            open_reader_by_id,
            prepare_reader_chapter,
            close_reader,
            get_local_comics,
            get_local_tags,
            get_downloaded_comics,
            export_cbz,
            export_cbz_without_download,
            pause_export_task,
            resume_export_task,
            delete_export_task,
            sync_export_tasks,
            list_quick_reader_candidates,
            export_quick_reader,
            export_quick_reader_single_file,
            export_pdf,
            export_cbz_chapters,
            export_pdf_chapters,
            get_logs_dir_size,
            get_storage_stats,
            clean_storage_leftovers,
            clean_storage_quick_readers,
            clean_storage_logs,
            delete_local_comic,
            get_synced_comic,
            get_synced_comic_in_favorite,
            get_synced_comic_in_search,
            get_synced_comic_in_weekly,
            open_log_file,
        ])
        .events(tauri_specta::collect_events![
            DownloadEvent,
            DownloadAllFavoritesEvent,
            UpdateDownloadedComicsEvent,
            ExportCbzEvent,
            ExportPdfEvent,
            ExportQuickReaderEvent,
            ExportTaskEvent,
            LogEvent,
        ]);

    #[cfg(debug_assertions)]
    builder
        .export(
            specta_typescript::Typescript::default()
                .bigint(specta_typescript::BigIntExportBehavior::Number)
                .formatter(specta_typescript::formatter::prettier)
                .header("// @ts-nocheck"), // 跳过检查
            "../src/bindings.ts",
        )
        .expect("Failed to export typescript bindings");

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        // 阅读器用的自定义协议：comic://page/{token}/{index}
        // 异步 + 独立线程处理，绝不阻塞主线程
        .register_asynchronous_uri_scheme_protocol("comic", |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            std::thread::spawn(move || {
                let uri = request.uri().clone();
                let path = uri.path().to_string();
                let query = uri.query().map(str::to_string);
                // 封面（/cover、/local-cover）先走 covers，其余都交给阅读器
                let response = covers::handle_cover_request(&app, &path, query.as_deref())
                    .unwrap_or_else(|| reader::handle_request(&app, &path));
                responder.respond(response);
            });
        })
        .on_window_event(|window, event| {
            // 关闭窗口时把所有没结束的下载任务设为暂停并落盘，下次启动由用户手动继续
            if matches!(event, tauri::WindowEvent::CloseRequested { .. }) {
                if let Some(download_manager) = window.try_state::<DownloadManager>() {
                    download_manager.pause_all_tasks();
                }
                if let Some(export_manager) = window.try_state::<ExportManager>() {
                    export_manager.pause_all_tasks();
                }
            }
        })
        .invoke_handler(builder.invoke_handler())
        .setup(move |app| {
            builder.mount_events(app);

            let app_data_dir = app
                .path()
                .app_data_dir()
                .wrap_err("failed to get app data dir")?;

            std::fs::create_dir_all(&app_data_dir).wrap_err(format!(
                "failed to create app data dir: {}",
                app_data_dir.display()
            ))?;

            let config = RwLock::new(Config::new(app.handle())?);
            app.manage(config);

            let jm_client = JmClient::new(app.handle().clone());
            app.manage(jm_client);

            app.manage(DownloadManager::new(app.handle()));

            let export_lock = ComicExportLock::new();
            app.manage(export_lock);

            app.manage(ExportManager::new(app.handle()));

            app.manage(ReaderState::default());

            logger::init(app.handle())?;

            // 恢复上次没做完的任务（全部恢复成暂停状态，由用户手动继续）
            app.state::<DownloadManager>().restore_tasks();
            app.state::<ExportManager>().restore_tasks();

            Ok(())
        })
        .run(generate_context())
        .expect("error while running tauri application");
}
