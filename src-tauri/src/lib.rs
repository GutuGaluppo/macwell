pub mod applications;
pub mod backup;
pub mod cleanup;
pub mod cloud;
pub mod commands;
pub mod db;
pub mod dev;
pub mod duplicates;
pub mod error;
pub mod exec;
pub mod filesystem;
pub mod images;
pub mod jobs;
pub mod menubar;
pub mod processes;
pub mod quarantine;
pub mod security;
pub mod smart_care;
pub mod state;
pub mod storage;
pub mod types;
pub mod updates;

use std::path::PathBuf;
use std::sync::OnceLock;

use tauri::Manager;
use tracing_appender::non_blocking::WorkerGuard;

use crate::db::Db;
use crate::filesystem::safety::SafetyPolicy;
use crate::state::{AppPaths, AppState};

static LOG_GUARD: OnceLock<WorkerGuard> = OnceLock::new();

/// Local log file, rotated daily, in ~/Library/Logs/<bundle id>.
fn init_logging(log_dir: &std::path::Path) {
    let appender = tracing_appender::rolling::Builder::new()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("organizemymac")
        .filename_suffix("log")
        .max_log_files(7)
        .build(log_dir);
    let Ok(appender) = appender else { return };
    let (writer, guard) = tracing_appender::non_blocking(appender);
    let _ = LOG_GUARD.set(guard);
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info"));
    let _ = tracing_subscriber::fmt().with_writer(writer).with_ansi(false).with_env_filter(filter).try_init();
}

/// Paths that belong to OrganizeMyMac itself and must never be removed while it runs.
fn own_paths(paths: &AppPaths) -> Vec<PathBuf> {
    let mut own = vec![paths.data_dir.clone(), paths.log_dir.clone()];
    if let Ok(exe) = std::env::current_exe() {
        if let Some(bundle) = exe.ancestors().find(|p| p.extension().is_some_and(|e| e == "app")) {
            own.push(bundle.to_path_buf());
        }
    }
    own
}

fn setup(app: &mut tauri::App) -> Result<(), Box<dyn std::error::Error>> {
    let home = dirs::home_dir().ok_or("home folder not found")?;
    let data_dir = app.path().app_data_dir()?;
    let log_dir = app.path().app_log_dir()?;
    std::fs::create_dir_all(&data_dir)?;
    std::fs::create_dir_all(&log_dir)?;
    init_logging(&log_dir);
    tracing::info!(version = %app.package_info().version, "starting");

    let paths = AppPaths { home: home.clone(), data_dir: data_dir.clone(), log_dir };
    let db = Db::open(&data_dir.join("organizemymac.sqlite"))?;
    let policy = SafetyPolicy::system(&home, own_paths(&paths));
    let state = AppState::new(paths, db, policy)?;
    let menu_bar = menubar::MenuBar::new(menubar::load_settings(&state));
    app.manage(state);
    app.manage(processes::HealthMonitor::default());
    app.manage(menu_bar);
    menubar::apply(app.handle())?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(setup)
        .invoke_handler(tauri::generate_handler![
            commands::cancel_job,
            commands::quarantine::get_removal_mode,
            commands::quarantine::set_removal_mode,
            commands::quarantine::list_quarantine,
            commands::quarantine::restore_quarantine,
            commands::quarantine::empty_quarantine,
            commands::backup::get_backup_state,
            commands::backup::get_backup_gate,
            commands::backup::preview_tm_exclusions,
            commands::backup::apply_tm_exclusions,
            commands::backup::list_tm_exclusions,
            commands::backup::undo_tm_exclusions,
            commands::backup::set_backup_stale_days,
            commands::dev::list_dev_roots,
            commands::dev::add_dev_root,
            commands::dev::remove_dev_root,
            commands::dev::start_dev_scan,
            commands::dev::scan_repo_secrets,
            commands::dev::plan_wip_snapshot,
            commands::dev::create_wip_snapshot,
            commands::dev::list_dev_snapshots,
            commands::dev::plan_push_snapshot,
            commands::dev::check_remote_visibility,
            commands::dev::push_snapshot,
            commands::dev::remove_dev_artifacts,
            commands::scan::start_scan,
            commands::scan::storage_node,
            commands::files::disk_overview,
            commands::files::start_find_files,
            commands::files::start_downloads_scan,
            commands::files::trash_summary,
            commands::files::empty_trash,
            commands::files::empty_trash_with_finder,
            commands::files::reveal_in_finder,
            commands::files::quick_look,
            commands::files::move_to_trash,
            commands::files::path_exists,
            commands::duplicates::start_duplicate_scan,
            commands::duplicates::move_duplicates_to_trash,
            commands::apps::start_app_list,
            commands::apps::app_icon,
            commands::apps::uninstall_plan,
            commands::apps::uninstall_app,
            commands::apps::start_orphan_scan,
            commands::apps::remove_orphans,
            commands::images::start_similar_images,
            commands::images::image_thumbnail,
            commands::images::photos_status,
            commands::images::start_similar_photos,
            commands::images::photo_thumbnail,
            commands::images::delete_photos,
            commands::security::start_security_audit,
            commands::cloud::cloud_folders,
            commands::cloud::start_cloud_usage,
            commands::cloud::evict_icloud,
            commands::cloud::start_update_check,
            commands::cloud::open_app_store_page,
            commands::cloud::open_application,
            commands::cleanup_rules::cleanup_rules,
            commands::cleanup_rules::add_custom_rule,
            commands::cleanup_rules::remove_custom_rule,
            commands::cleanup_rules::run_cleanup_rules,
            commands::smart_care::start_smart_care,
            commands::smart_care::run_smart_care,
            commands::health::health,
            commands::health::process_list,
            commands::health::quit_application,
            menubar::menu_bar_settings,
            menubar::set_menu_bar_settings,
            commands::system::permission_status,
            commands::system::open_system_settings,
            commands::system::suggested_locations,
            commands::system::app_info,
            commands::system::recent_scans,
            commands::system::operation_log,
            commands::system::local_metrics,
            commands::system::ignore_list,
            commands::system::add_to_ignore_list,
            commands::system::remove_from_ignore_list,
        ])
        .on_window_event(|window, event| {
            // With the menu bar item on, closing the window keeps the app running there.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.app_handle().state::<menubar::MenuBar>().settings().enabled {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("error while building OrganizeMyMac")
        .run(|app, event| match event {
            tauri::RunEvent::ExitRequested { .. } => app.state::<AppState>().jobs.cancel_all(),
            // Clicking the Dock icon brings the window back.
            tauri::RunEvent::Reopen { .. } => menubar::show_main_window(app),
            _ => {}
        });
}
