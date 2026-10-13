use std::path::PathBuf;

use serde::Deserialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager};

use super::scan::scan_options;
use super::{start_job, JobEvent, JobOutcome};
use crate::applications::leftovers::{installed_bundle_ids, still_orphan};
use crate::cleanup::{trash_bin, RemovalItem, RemovalRequest};
use crate::dev::removal::ArtifactRemoval;
use crate::duplicates::{unchanged, validate_group, GroupRemoval};
use crate::backup::gate::BackupAck;
use crate::commands::backup::require_backup_ack;
use crate::error::{AppError, AppResult};
use crate::smart_care::{self, SmartCareReport};
use crate::state::AppState;
use crate::types::OperationOutcome;

#[tauri::command]
pub fn start_smart_care(app: AppHandle, on_event: Channel<JobEvent<Option<SmartCareReport>>>) -> AppResult<String> {
    Ok(start_job(&app, on_event, "smart-care", move |state, cancel, emit| {
        let home = state.paths.home.clone();
        emit(crate::filesystem::scanner::ScanEvent::Start { root: home.display().to_string() });
        let base = scan_options(state, &home);
        let dev_roots: Vec<PathBuf> = state.db.lock().unwrap().dev_roots()?.into_iter().map(|r| PathBuf::from(r.path)).collect();
        match smart_care::run(&home, &base, cancel, &dev_roots, emit) {
            Ok(report) => Ok(JobOutcome { cancelled: false, result: Some(report) }),
            Err(AppError::Cancelled) => Ok(JobOutcome { cancelled: true, result: None }),
            Err(e) => Err(e),
        }
    }))
}

/// What the user approved on the review screen.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CarePlan {
    /// Top-level Trash items to delete permanently (exactly the ones reviewed).
    #[serde(default)]
    pub empty_trash: Vec<String>,
    /// Downloads items and large files, moved to the Trash (must be inside the home folder).
    #[serde(default)]
    pub files: Vec<RemovalItem>,
    #[serde(default)]
    pub duplicates: Vec<GroupRemoval>,
    #[serde(default)]
    pub leftovers: Vec<RemovalItem>,
    /// Regenerable folders inside projects; each one is re-derived on the server before it moves.
    #[serde(default)]
    pub artifacts: Vec<ArtifactRemoval>,
}

/// Runs a reviewed Smart Care plan. The Trash is emptied first and only the reviewed items are
/// deleted, so nothing moved to the Trash by this same plan is ever deleted permanently.
#[tauri::command]
pub async fn run_smart_care(app: AppHandle, plan: CarePlan, backup_ack: Option<BackupAck>) -> AppResult<Vec<OperationOutcome>> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        require_backup_ack(&state, backup_ack.as_ref())?;
        let home = state.paths.home.clone();
        let library = home.join("Library");
        let policy = state.policy.read().unwrap().clone();
        let mut outcomes = Vec::new();

        if !plan.empty_trash.is_empty() {
            let db = state.db.lock().unwrap();
            outcomes.extend(trash_bin::empty_only(&policy, &db, &home.join(".Trash"), &plan.empty_trash)?);
        }

        let mut files = plan.files;
        for g in &plan.duplicates {
            match validate_group(g) {
                Ok(()) => {
                    for r in &g.remove {
                        if unchanged(r) {
                            files.push(RemovalItem { path: r.path.clone(), size: r.size });
                        } else {
                            outcomes.push(OperationOutcome { path: r.path.clone(), size: r.size, ok: false, error: Some("the file changed since the scan".into()) });
                        }
                    }
                }
                Err(rule) => outcomes.extend(g.remove.iter().map(|r| OperationOutcome { path: r.path.clone(), size: r.size, ok: false, error: Some(rule.to_string()) })),
            }
        }
        if !plan.leftovers.is_empty() {
            let installed = installed_bundle_ids(&home);
            for item in plan.leftovers {
                if still_orphan(&PathBuf::from(&item.path), &library, &installed) {
                    files.push(item);
                } else {
                    outcomes.push(OperationOutcome { path: item.path, size: item.size, ok: false, error: Some("no longer looks like a leftover".into()) });
                }
            }
        }
        let db = state.db.lock().unwrap();
        let moved = super::remove_items(&state, &policy, &db, &RemovalRequest { items: files, scan_root: Some(home.display().to_string()) }, "trash");
        drop(db);
        let removed: Vec<PathBuf> = moved.iter().filter(|o| o.ok).map(|o| PathBuf::from(&o.path)).collect();
        state.forget_paths(&removed);
        outcomes.extend(moved);
        if !plan.artifacts.is_empty() {
            outcomes.extend(super::dev::remove_artifacts(&state, plan.artifacts)?);
        }
        Ok(outcomes)
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}
