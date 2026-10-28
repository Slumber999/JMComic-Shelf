#![allow(clippy::used_underscore_binding)]

use events::{
    DownloadAllFavoritesEvent, DownloadEvent, ExportCbzEvent, ExportPdfEvent, ExportTaskEvent,
    ExportQuickReaderEvent, LogEvent, UpdateDownloadedComicsEvent,
};
use eyre::WrapErr;
use parking_lot::RwLock;
use tauri::{Manager, Wry};

use crate::commands::*;
use crate::config::Config;
use crate::downloader::download_manager::DownloadManager;
use crate::errors::install_custom_eyre_handler;
use crate::export::manager::ExportManager;
use crate::export::ComicExportLock;
use crate::jm_client::JmClient;
use crate::reader::ReaderState;
use crate::reader_window::{
    get_reader_window_target, open_reader_window, MainWindowInitialSize, ReaderWindowTargetState,
    READER_WINDOW_LABEL,
};

mod android_log;
mod comic_tags;
mod commands;
mod config;
mod covers;
mod crash;
mod downloader;
mod errors;
mod events;
mod export;
mod extensions;
mod favorites_index;
mod jm_client;
mod lines;
mod reader_window;
mod local_index;
mod logger;
mod measure;
mod quick_reader;
mod reader;
mod responses;
mod storage;
mod types;
mod utils;

/// 自定义协议的响应预算（只有 Android 用得到）。
/// Android：wry 在 shouldInterceptRequest 里只等 10s（MAIN_PIPE_TIMEOUT），超时把
/// `Err(Timeout)` unwrap 成 panic，release 的 panic=abort 直接退进程，所以自己先掐到 8s。
/// 桌面端不回在 UI 线程里等响应，改成工作线程算完自己 respond，所以不需要这个上限。
#[cfg(mobile)]
const PROTOCOL_RESPONSE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(8);

fn generate_context() -> tauri::Context<Wry> {
    tauri::generate_context!()
}

/// 恢复主窗口上次的尺寸，比当前显示器还大就收回来
#[cfg(desktop)]
fn restore_main_window_size(window: &tauri::WebviewWindow, width: u32, height: u32) {
    let mut logical_width = f64::from(width);
    let mut logical_height = f64::from(height);

    if let Ok(Some(monitor)) = window.current_monitor() {
        let scale_factor = monitor.scale_factor();
        logical_width = logical_width.min(f64::from(monitor.size().width) / scale_factor);
        // 给任务栏留一点，不然窗口会顶到屏幕外
        logical_height = logical_height.min(f64::from(monitor.size().height) / scale_factor - 80.0);
    }

    let _ = window.set_size(tauri::LogicalSize::new(
        logical_width.max(600.0),
        logical_height.max(400.0),
    ));
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // 最先装 panic hook：启动早期崩溃 / 阅读时的 native 崩溃都能留下记录
    crash::install();

    // 安装失败（比如已经装过）不应该让整个应用起不来，记一条日志继续跑
    if let Err(err) = install_custom_eyre_handler() {
        eprintln!("安装自定义错误处理器失败，继续启动: {err:?}");
        tracing::error!(message = %err, "安装自定义错误处理器失败，继续启动");
    }

    let builder = tauri_specta::Builder::<Wry>::new()
        .commands(tauri_specta::collect_commands![
            get_config,
            save_config,
            login,
            logout,
            get_notifications,
            mark_notification_read,
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
            get_comments,
            get_novel_comments,
            get_watch_history,
            delete_watch_history,
            get_novel_list,
            get_novel_detail,
            get_novel_chapter,
            search_novels,
            get_favorite_folder,
            get_all_favorite_comics,
            get_favorites_index,
            build_favorites_index,
            update_favorites_index,
            remove_from_favorites_index,
            get_favorites_tags,
            fetch_comic_tags,
            get_comic_tags,
            get_weekly_info,
            get_weekly,
            get_serialization,
            get_user_profile,
            get_sign_status,
            sign_in,
            create_download_task,
            create_download_tasks,
            pause_download_task,
            resume_download_task,
            delete_download_task,
            sync_download_tasks,
            download_comic,
            download_all_favorites,
            update_downloaded_comics,
            update_exported_comics,
            show_path_in_file_manager,
            show_config_dir_in_file_manager,
            sync_favorite_folder,
            open_comic_reader,
            open_reader_by_id,
            prepare_reader_chapter,
            close_reader,
            get_local_comics,
            get_local_tags_all,
            get_downloaded_comics,
            export_cbz,
            export_cbz_without_download,
            export_cbz_chapters_without_download,
            export_comic_zip,
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
            open_reader_window,
            get_reader_window_target,
            exit_app,
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

    // 只在桌面开发时导出 TS 绑定：Android 上沙箱里没有 ../src 可写，会直接 panic
    #[cfg(all(debug_assertions, desktop))]
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
        // 封面（/cover、/local-cover）也走这里，见 covers.rs
        .register_asynchronous_uri_scheme_protocol("comic", |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            let uri = request.uri().clone();
            let path = uri.path().to_string();
            let query = uri.query().map(str::to_string);
            let log_path = path.clone();

            // 这个回调在 Windows 上跑在 WebView2 的 UI 线程（窗口线程）里：
            // 只要在这里等网络/磁盘，整个窗口就不刷新、点击和 invoke 也排不上号——
            // 「封面还没加载完就点阅读/详情会卡住、连点变未响应」就是这条路径。
            // 所以桌面端把活交给工作线程，算完由它自己 respond（wry 内部会 PostMessage
            // 回 UI 线程再 SetResponse，工作线程不会反过来等 UI 线程）。
            #[cfg(desktop)]
            {
                let entered = measure::enter("protocol", &format!("path={log_path}"));
                let worker_path = log_path.clone();
                // responder 放在槽里：线程起不来（极端情况）时主线程还能回一个 504，
                // 不能让这条请求永远挂着；线程起来后由工作线程取走并在算完时 respond。
                let slot = std::sync::Arc::new(parking_lot::Mutex::new(Some(responder)));
                let worker_slot = slot.clone();
                let spawned = std::thread::Builder::new()
                    .name("comic-proto".to_string())
                    .spawn(move || {
                        let response = covers::handle_cover_request(&app, &path, query.as_deref())
                            .unwrap_or_else(|| reader::handle_request(&app, &path));
                        measure::finish(
                            "protocol-handle",
                            entered,
                            &format!("path={worker_path}"),
                        );
                        if let Some(responder) = worker_slot.lock().take() {
                            responder.respond(response);
                        }
                    });
                if let Err(err) = spawned {
                    tracing::error!(message = %err, "comic:// 工作线程启动失败，先回 504");
                    if let Some(responder) = slot.lock().take() {
                        responder.respond(reader::timeout_response());
                    }
                }
                // 到这里回调就返回了：UI 线程只花了这么点时间，封面/页图都在工作线程上跑
                measure::finish("protocol-return", entered, &format!("path={log_path}"));
            }

            // Android 上 wry 只等 10s，超时把 Err(Timeout) unwrap 成 panic，release 的
            // panic=abort 再把整个进程带走（设备上 16:32 的 SIGABRT 就是这条路径）。
            // 这里先按 PROTOCOL_RESPONSE_TIMEOUT 掐时间：超时回 504，取图/取封面继续在
            // 后台跑并把结果写进缓存，前端重试一次就能命中；这块只在 Android 上生效，
            // 桌面端绝不在这条回调里等。
            #[cfg(mobile)]
            {
                let (tx, rx) = std::sync::mpsc::channel();
                std::thread::spawn(move || {
                    // 封面（/cover、/local-cover）先走 covers，其余都交给阅读器
                    let response = covers::handle_cover_request(&app, &path, query.as_deref())
                        .unwrap_or_else(|| reader::handle_request(&app, &path));
                    // 主线程可能已经超时返回 504 并把 rx 丢掉，send 失败就算了
                    let _ = tx.send(response);
                });

                match rx.recv_timeout(PROTOCOL_RESPONSE_TIMEOUT) {
                    Ok(response) => responder.respond(response),
                    Err(_) => {
                        tracing::warn!(
                            path = %log_path,
                            budget = ?PROTOCOL_RESPONSE_TIMEOUT,
                            "comic:// 响应超时，先回 504（后台继续取，稍后重试即可命中缓存）"
                        );
                        responder.respond(reader::timeout_response());
                    }
                }
            }
        })
        .on_window_event(|window, event| {
            // 阅读窗口关掉了就释放它的后端缓存
            if matches!(event, tauri::WindowEvent::Destroyed) && window.label() == READER_WINDOW_LABEL {
                if let Some(reader_state) = window.try_state::<ReaderState>() {
                    crate::reader::close_reader(&reader_state);
                }
            }

            if !matches!(event, tauri::WindowEvent::CloseRequested { .. }) {
                return;
            }

            // 关掉主窗口就跟着关掉阅读窗口，避免只剩一个阅读窗口还开着
            #[cfg(desktop)]
            {
                if window.label() == "main" {
                    if let Some(reader_window) =
                        window.app_handle().get_webview_window(READER_WINDOW_LABEL)
                    {
                        let _ = reader_window.close();
                    }

                    // 关窗时再记一次尺寸，免得刚拉完窗口就退出、前端那次防抖保存还没落盘
                    if let (Some(config), Ok(size)) =
                        (window.try_state::<RwLock<Config>>(), window.inner_size())
                    {
                        let scale_factor = window.scale_factor().unwrap_or(1.0);
                        let mut config = config.write();
                        config.window_width = (f64::from(size.width) / scale_factor).round() as u32;
                        config.window_height = (f64::from(size.height) / scale_factor).round() as u32;
                        if let Err(err) = config.save(window.app_handle()) {
                            tracing::error!(message = %err, "退出时保存窗口尺寸失败");
                        }
                    }
                }
            }

            // 只有主窗口关闭才暂停任务：关阅读窗口不该影响下载/导出
            if window.label() != "main" {
                return;
            }

            // 关闭主窗口时把所有没结束的下载任务设为暂停并落盘，下次启动由用户手动继续
            if let Some(download_manager) = window.try_state::<DownloadManager>() {
                download_manager.pause_all_tasks();
            }
            if let Some(export_manager) = window.try_state::<ExportManager>() {
                export_manager.pause_all_tasks();
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

            // panic hook 落盘用同一个日志目录（logger 的 logs_dir 也在 app_data_dir/日志）
            crash::set_log_dir(app_data_dir.join("日志"));

            let config = RwLock::new(Config::new(app.handle())?);

            // 主窗口用上次的尺寸（换网格档位、拉伸窗口时前端会把它写进配置）
            #[cfg(desktop)]
            {
                if let Some(window) = app.get_webview_window("main") {
                    let (width, height) = {
                        let saved = config.read();
                        (saved.window_width, saved.window_height)
                    };
                    restore_main_window_size(&window, width, height);

                    // 记下主窗口启动时的尺寸：阅读窗口按它开，不跟随之后的变化
                    let scale_factor = window.scale_factor().unwrap_or(1.0);
                    if let Ok(size) = window.inner_size() {
                        let initial = (
                            f64::from(size.width) / scale_factor,
                            f64::from(size.height) / scale_factor,
                        );
                        app.manage(MainWindowInitialSize::new(initial));
                    }
                }
            }

            // 冷启动先用上次请求成功的线路，别从死线路开始等超时
            lines::load_persisted_api_line(app.handle());

            app.manage(config);

            let jm_client = JmClient::new(app.handle().clone());
            app.manage(jm_client);

            app.manage(DownloadManager::new(app.handle()));

            let export_lock = ComicExportLock::new();
            app.manage(export_lock);

            app.manage(ExportManager::new(app.handle()));

            app.manage(ReaderState::default());

            app.manage(ReaderWindowTargetState::default());

            logger::init(app.handle())?;

            // 启动预热：登录态恢复后在后台打一次最轻的 /categories，把 DNS/TLS/线路选择提前做掉；
            // 失败静默重试一次，不弹错误、不阻塞界面
            {
                let warm_handle = app.handle().clone();
                tauri::async_runtime::spawn(async move {
                    let client = warm_handle.state::<JmClient>().inner().clone();
                    let started = std::time::Instant::now();
                    let mut last_error = None;
                    for attempt in 0..2 {
                        match client.get_categories().await {
                            Ok(_) => {
                                last_error = None;
                                break;
                            }
                            Err(err) => last_error = Some(err),
                        }
                        if attempt == 0 {
                            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                        }
                    }
                    let ms = started.elapsed().as_millis() as u64;
                    match last_error {
                        None => tracing::debug!(ms, "启动预热完成"),
                        Some(err) => tracing::debug!(ms, message = %err, "启动预热失败，忽略"),
                    }
                    crate::android_log::log_line("JMAPI", &format!("warmup categories ms={ms}"));
                });
            }

            // 恢复上次没做完的任务（全部恢复成暂停状态，由用户手动继续）
            app.state::<DownloadManager>().restore_tasks();
            app.state::<ExportManager>().restore_tasks();

            Ok(())
        })
        .run(generate_context())
        .expect("error while running tauri application");
}
