use std::collections::HashSet;
use std::path::{Path, PathBuf};

use base64::Engine;
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager};

use super::{start_job, JobEvent, JobOutcome};
use crate::applications::leftovers::{find_for_app, find_orphans, installed_bundle_ids, still_orphan, AppIdentity, Leftover, OrphanGroup};
use crate::applications::{icon_png, is_running, list_apps, read_bundle, running_executables, AppInfo};
use crate::cleanup::{self, RemovalItem, RemovalRequest};
use crate::backup::gate::BackupAck;
use crate::commands::backup::require_backup_ack;
use crate::error::{AppError, AppResult};
use crate::filesystem::scanner::ScanEvent;
use crate::filesystem::{probe_full_disk_access, AccessStatus};
use crate::state::AppState;
use crate::types::OperationOutcome;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppsResult {
    pub apps: Vec<AppInfo>,
    pub total_size: u64,
}

#[tauri::command]
pub fn start_app_list(app: AppHandle, on_event: Channel<JobEvent<AppsResult>>) -> AppResult<String> {
    Ok(start_job(&app, on_event, "apps", move |state, cancel, emit| {
        emit(ScanEvent::Start { root: "/Applications".into() });
        let running = running_executables();
        let (tx, rx) = std::sync::mpsc::channel::<(u64, u64)>();
        let progress = move |done, total| {
            let _ = tx.send((done, total));
        };
        let home = state.paths.home.clone();
        let cancel_ref = cancel.clone();
        let worker = std::thread::spawn(move || list_apps(&home, &running, &cancel_ref, &progress));
        for (done, total) in rx {
            emit(ScanEvent::Stage { stage: "apps".into(), done, total });
        }
        let apps = worker.join().map_err(|_| AppError::Other("listing apps failed".into()))?;
        let total_size = apps.iter().map(|a| a.size).sum();
        Ok(JobOutcome { cancelled: cancel.load(std::sync::atomic::Ordering::Relaxed), result: AppsResult { apps, total_size } })
    }))
}

/// The app's icon as a PNG data URL (cached).
#[tauri::command]
pub async fn app_icon(state: tauri::State<'_, AppState>, path: String) -> AppResult<Option<String>> {
    let app = validate_app_path(&path, &state.paths.home)?;
    let cache = state.paths.data_dir.join("icons");
    Ok(tauri::async_runtime::spawn_blocking(move || icon_png(&app, &cache))
        .await
        .map_err(|e| AppError::Other(e.to_string()))?
        .map(|png| format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png))))
}

/// Only bundles directly in the application folders (or one level down) can be uninstalled.
fn validate_app_path(path: &str, home: &Path) -> AppResult<PathBuf> {
    let p = PathBuf::from(path);
    let in_apps = [PathBuf::from("/Applications"), home.join("Applications")]
        .iter()
        .any(|root| p.parent().is_some_and(|parent| parent == root || parent.parent() == Some(root.as_path())));
    if !p.is_absolute() || p.extension().is_none_or(|e| e != "app") || !in_apps || !p.is_dir() {
        return Err(AppError::Invalid("not an application in /Applications or ~/Applications".into()));
    }
    Ok(p)
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UninstallPlan {
    pub app_path: String,
    pub name: String,
    pub bundle_id: Option<String>,
    pub app_size: u64,
    pub leftovers: Vec<Leftover>,
    pub running: bool,
    /// Apple apps that come with macOS cannot be removed.
    pub protected: bool,
    /// Without Full Disk Access, containers are not checked.
    pub full_disk_access: bool,
}

fn build_plan(state: &AppState, app: &Path) -> AppResult<UninstallPlan> {
    let info = read_bundle(app);
    let from_store = app.join("Contents/_MASReceipt").exists();
    let protected = info.bundle_id.as_deref().is_some_and(|id| id.starts_with("com.apple.")) && !from_store;
    let fda = probe_full_disk_access(&state.paths.home) == AccessStatus::Granted;
    let name = info.name.clone().unwrap_or_else(|| app.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default());
    let leftovers = match &info.bundle_id {
        Some(id) if !protected => {
            let mut installed = installed_bundle_ids(&state.paths.home);
            installed.insert(id.clone());
            find_for_app(&state.paths.home.join("Library"), &AppIdentity { bundle_id: id.clone(), name: name.clone() }, &installed, fda)
        }
        _ => Vec::new(),
    };
    let never = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    Ok(UninstallPlan {
        app_path: app.display().to_string(),
        app_size: crate::applications::bundle_size(app, &never),
        name,
        bundle_id: info.bundle_id,
        leftovers,
        running: is_running(app),
        protected,
        full_disk_access: fda,
    })
}

#[tauri::command]
pub async fn uninstall_plan(app: AppHandle, path: String) -> AppResult<UninstallPlan> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let bundle = validate_app_path(&path, &state.paths.home)?;
        build_plan(&state, &bundle)
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

/// Moves the app and the chosen leftovers to the Trash. The plan is rebuilt here and only paths
/// that are part of it are accepted, so the UI cannot ask to remove anything else.
#[tauri::command]
pub async fn uninstall_app(app: AppHandle, path: String, leftovers: Vec<String>) -> AppResult<Vec<OperationOutcome>> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let bundle = validate_app_path(&path, &state.paths.home)?;
        let plan = build_plan(&state, &bundle)?;
        if plan.protected {
            return Err(AppError::Invalid("apps that come with macOS cannot be removed".into()));
        }
        if plan.running {
            return Err(AppError::Invalid(format!("quit {} before uninstalling it", plan.name)));
        }
        let allowed: HashSet<&str> = plan.leftovers.iter().map(|l| l.path.as_str()).collect();
        if let Some(bad) = leftovers.iter().find(|p| !allowed.contains(p.as_str())) {
            return Err(AppError::Invalid(format!("{bad} is not part of this app's files")));
        }
        let mut items = vec![RemovalItem { path: plan.app_path.clone(), size: plan.app_size }];
        items.extend(plan.leftovers.iter().filter(|l| leftovers.contains(&l.path)).map(|l| RemovalItem { path: l.path.clone(), size: l.size }));
        let policy = state.policy.read().unwrap().clone();
        let db = state.db.lock().unwrap();
        Ok(cleanup::move_to_trash(&policy, &db, &RemovalRequest { items, scan_root: None }, "uninstall"))
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrphansResult {
    pub groups: Vec<OrphanGroup>,
    pub total_size: u64,
    pub full_disk_access: bool,
}

#[tauri::command]
pub fn start_orphan_scan(app: AppHandle, on_event: Channel<JobEvent<OrphansResult>>) -> AppResult<String> {
    Ok(start_job(&app, on_event, "leftovers", move |state, _cancel, emit| {
        let library = state.paths.home.join("Library");
        emit(ScanEvent::Start { root: library.display().to_string() });
        emit(ScanEvent::Stage { stage: "index".into(), done: 0, total: 1 });
        let installed = installed_bundle_ids(&state.paths.home);
        emit(ScanEvent::Stage { stage: "search".into(), done: 1, total: 2 });
        let fda = probe_full_disk_access(&state.paths.home) == AccessStatus::Granted;
        let groups = find_orphans(&library, &installed, fda);
        let total_size = groups.iter().map(|g| g.size).sum();
        Ok(JobOutcome { cancelled: false, result: OrphansResult { groups, total_size, full_disk_access: fda } })
    }))
}

/// Moves leftovers of uninstalled apps to the Trash. Each path must still be an entry of one of
/// the known ~/Library folders, named after a bundle id that no installed app has.
#[tauri::command]
pub async fn remove_orphans(app: AppHandle, items: Vec<RemovalItem>, backup_ack: Option<BackupAck>) -> AppResult<Vec<OperationOutcome>> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        require_backup_ack(&state, backup_ack.as_ref())?;
        let library = state.paths.home.join("Library");
        let installed = installed_bundle_ids(&state.paths.home);
        let mut outcomes = Vec::new();
        let mut accepted = Vec::new();
        for item in items {
            let p = PathBuf::from(&item.path);
            if still_orphan(&p, &library, &installed) {
                accepted.push(item);
            } else {
                outcomes.push(OperationOutcome { path: item.path, size: item.size, ok: false, error: Some("no longer looks like a leftover".into()) });
            }
        }
        let policy = state.policy.read().unwrap().clone();
        let db = state.db.lock().unwrap();
        outcomes.extend(super::remove_items(&state, &policy, &db, &RemovalRequest { items: accepted, scan_root: Some(library.display().to_string()) }, "trash"));
        Ok(outcomes)
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}
