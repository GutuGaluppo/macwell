//! Storage MVP commands: disk overview, large & old files, Downloads, Trash and the file actions
//! (reveal, Quick Look, move to Trash).

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

use serde::Serialize;
use tauri::ipc::Channel;
use tauri::AppHandle;

use super::scan::{record, scan_options, validate_root, ScanResult};
use super::{start_job, JobEvent, JobOutcome};
use crate::cleanup::downloads::DownloadItem;
use crate::cleanup::{trash_bin, RemovalRequest};
use crate::db::now_ms;
use crate::error::{AppError, AppResult};
use crate::filesystem::scanner::{scan, ScanStats};
use crate::state::AppState;
use crate::storage::finder::{FileFilter, FileFinder, FinderResult};
use crate::storage::volumes::{self, Volume};
use crate::types::OperationOutcome;

fn existing_absolute(path: &str) -> AppResult<PathBuf> {
    let p = PathBuf::from(path);
    if !p.is_absolute() {
        return Err(AppError::Invalid("path must be absolute".into()));
    }
    if std::fs::symlink_metadata(&p).is_err() {
        return Err(AppError::NotFound(path.into()));
    }
    Ok(p)
}

fn scan_result(id: String, started_at: i64, stats: &ScanStats, reclaimable: u64) -> ScanResult {
    ScanResult {
        id,
        started_at,
        finished_at: now_ms(),
        stats: stats.clone(),
        reclaimable_bytes: reclaimable,
        largest_files: Vec::new(),
        tree: None,
    }
}

#[tauri::command]
pub fn disk_overview() -> Vec<Volume> {
    volumes::list()
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FindResult {
    #[serde(flatten)]
    pub scan: ScanResult,
    #[serde(flatten)]
    pub found: FinderResult,
}

/// Large & Old Files.
#[tauri::command]
pub fn start_find_files(app: AppHandle, root: String, filter: FileFilter, on_event: Channel<JobEvent<FindResult>>) -> AppResult<String> {
    let root = validate_root(&root)?;
    Ok(start_job(&app, on_event, "find-files", move |state, cancel, emit| {
        let started_at = now_ms();
        let mut finder = FileFinder::new(filter, &root, &state.paths.home, started_at, 2_000);
        let stats = scan(&scan_options(state, &root), cancel, &mut finder, &mut *emit)?;
        let found = finder.finish();
        let scan = scan_result(uuid::Uuid::new_v4().to_string(), started_at, &stats, found.matched_bytes);
        record(state, "largeFiles", &scan)?;
        Ok(JobOutcome { cancelled: stats.cancelled, result: FindResult { scan, found } })
    }))
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadsResult {
    #[serde(flatten)]
    pub scan: ScanResult,
    pub folder: String,
    pub items: Vec<DownloadItem>,
    pub total_bytes: u64,
}

/// Downloads cleanup: each top-level item, classified and ranked.
#[tauri::command]
pub fn start_downloads_scan(app: AppHandle, on_event: Channel<JobEvent<DownloadsResult>>) -> AppResult<String> {
    Ok(start_job(&app, on_event, "downloads", move |state, cancel, emit| {
        let started_at = now_ms();
        let root = state.paths.home.join("Downloads");
        let (items, stats) = crate::cleanup::downloads::analyze(&scan_options(state, &root), cancel, &mut *emit)?;
        let total_bytes = items.iter().map(|i| i.entry.size_logical).sum();
        let reclaimable = items.iter().filter(|i| i.selected).map(|i| i.entry.size_logical).sum();
        let scan = scan_result(uuid::Uuid::new_v4().to_string(), started_at, &stats, reclaimable);
        record(state, "downloads", &scan)?;
        Ok(JobOutcome {
            cancelled: stats.cancelled,
            result: DownloadsResult { scan, folder: root.display().to_string(), items, total_bytes },
        })
    }))
}

#[tauri::command]
pub async fn trash_summary(state: tauri::State<'_, AppState>) -> AppResult<trash_bin::TrashSummary> {
    let dir = state.paths.home.join(".Trash");
    tauri::async_runtime::spawn_blocking(move || trash_bin::summary(&dir)).await.map_err(|e| AppError::Other(e.to_string()))?
}

/// Permanently deletes the Trash contents. The UI asks for explicit confirmation first.
#[tauri::command]
pub async fn empty_trash(app: AppHandle) -> AppResult<Vec<OperationOutcome>> {
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        let state = app.state::<AppState>();
        let policy = state.policy.read().unwrap().clone();
        let db = state.db.lock().unwrap();
        trash_bin::empty(&policy, &db, &state.paths.home.join(".Trash"))
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

/// Without Full Disk Access the app cannot read the Trash; Finder can empty it (macOS asks the
/// user to allow OrganizeMyMac to control Finder the first time).
#[tauri::command]
pub async fn empty_trash_with_finder() -> AppResult<()> {
    let status = tauri::async_runtime::spawn_blocking(|| {
        Command::new("/usr/bin/osascript").args(["-e", "tell application \"Finder\" to empty trash"]).status()
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))??;
    if status.success() { Ok(()) } else { Err(AppError::Other("Finder did not empty the Trash".into())) }
}

#[tauri::command]
pub fn reveal_in_finder(path: String) -> AppResult<()> {
    let p = existing_absolute(&path)?;
    Command::new("/usr/bin/open").arg("-R").arg(&p).status()?;
    Ok(())
}

static QUICK_LOOK: Mutex<Option<Child>> = Mutex::new(None);

/// Quick Look preview in its own panel. A new preview replaces the previous one.
#[tauri::command]
pub fn quick_look(path: String) -> AppResult<()> {
    let p = existing_absolute(&path)?;
    let mut current = QUICK_LOOK.lock().unwrap();
    if let Some(mut child) = current.take() {
        let _ = child.kill();
        let _ = child.wait();
    }
    let child = Command::new("/usr/bin/qlmanage")
        .arg("-p")
        .arg(&p)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    *current = Some(child);
    Ok(())
}

/// Moves reviewed items to the Trash. Each one is validated again right before it moves.
#[tauri::command]
pub async fn move_to_trash(app: AppHandle, request: RemovalRequest) -> AppResult<Vec<OperationOutcome>> {
    if request.items.is_empty() {
        return Ok(Vec::new());
    }
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        let state = app.state::<AppState>();
        let policy = state.policy.read().unwrap().clone();
        let outcomes = {
            let db = state.db.lock().unwrap();
            super::remove_items(&state, &policy, &db, &request, "trash")
        };
        let removed: Vec<PathBuf> = outcomes.iter().filter(|o| o.ok).map(|o| PathBuf::from(&o.path)).collect();
        state.forget_paths(&removed);
        Ok(outcomes)
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

/// Whether a path still exists (rows are refreshed after the user acts in Finder).
#[tauri::command]
pub fn path_exists(path: String) -> bool {
    Path::new(&path).is_absolute() && std::fs::symlink_metadata(&path).is_ok()
}
