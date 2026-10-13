//! Tauri command layer: the only entry point from the UI. Every command takes typed, validated
//! arguments; nothing here runs a shell command built from UI input.

pub mod apps;
pub mod backup;
pub mod brew;
pub mod cleanup_rules;
pub mod cloud;
pub mod dev;
pub mod duplicates;
pub mod files;
pub mod health;
pub mod images;
pub mod quarantine;
pub mod report;
pub mod scan;
pub mod security;
pub mod smart_care;
pub mod system;

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager};

use crate::error::AppResult;
use crate::filesystem::scanner::{ScanEvent, ScanProgress};
use crate::jobs::spawn_background;
use crate::state::AppState;

/// Events streamed to the UI for a job (`scan:start`, `scan:progress`, `scan:warning`,
/// `scan:complete`, `scan:cancelled` in the spec, plus `failed`).
#[derive(Clone, Serialize)]
#[serde(tag = "event", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum JobEvent<T: Serialize + Clone> {
    Start { job_id: String, root: String },
    Progress(ScanProgress),
    Warning { path: String, message: String },
    Stage { stage: String, done: u64, total: u64 },
    Complete { result: T },
    Cancelled { result: T },
    Failed { message: String },
}

pub struct JobOutcome<T> {
    pub result: T,
    pub cancelled: bool,
}

/// Runs `work` as a cancellable background job and streams its events to `channel`.
/// Returns the job id right away.
pub fn start_job<T, F>(app: &AppHandle, channel: Channel<JobEvent<T>>, name: &str, work: F) -> String
where
    T: Serialize + Clone + Send + 'static,
    F: FnOnce(&AppState, &Arc<AtomicBool>, &mut dyn FnMut(ScanEvent)) -> AppResult<JobOutcome<T>> + Send + 'static,
{
    let (job_id, cancel) = app.state::<AppState>().jobs.register();
    let app = app.clone();
    let id = job_id.clone();
    spawn_background(name, move || {
        let state = app.state::<AppState>();
        let mut emit = |event: ScanEvent| {
            let mapped = match event {
                ScanEvent::Start { root } => JobEvent::Start { job_id: id.clone(), root },
                ScanEvent::Progress(p) => JobEvent::Progress(p),
                ScanEvent::Warning { path, message } => JobEvent::Warning { path, message },
                ScanEvent::Stage { stage, done, total } => JobEvent::Stage { stage, done, total },
            };
            let _ = channel.send(mapped);
        };
        let event = match work(&state, &cancel, &mut emit) {
            Ok(JobOutcome { result, cancelled: false }) => JobEvent::Complete { result },
            Ok(JobOutcome { result, cancelled: true }) => JobEvent::Cancelled { result },
            Err(err) => {
                tracing::warn!(job = %id, error = %err, "job failed");
                JobEvent::Failed { message: err.to_string() }
            }
        };
        let _ = channel.send(event);
        state.jobs.finish(&id);
    });
    job_id
}

#[tauri::command]
pub fn cancel_job(state: tauri::State<'_, AppState>, job_id: String) -> bool {
    state.jobs.cancel(&job_id)
}

/// Moves reviewed items to the Trash or into quarantine, whichever the user chose in Settings (the
/// Trash unless they changed it). Every batch removal goes through here, so the choice is made in
/// one place and the safety policy applies the same way to both.
pub fn remove_items(state: &AppState, policy: &crate::filesystem::safety::SafetyPolicy, db: &crate::db::Db, request: &crate::cleanup::RemovalRequest, action: &str) -> Vec<crate::types::OperationOutcome> {
    let mode = crate::quarantine::RemovalMode::from_setting(db.setting(crate::quarantine::MODE_SETTING).ok().flatten().as_deref());
    let env = crate::quarantine::Env { data_root: state.paths.data_dir.join("Quarantine"), data_dir: state.paths.data_dir.clone(), volumes: &crate::quarantine::RealVolumes };
    crate::quarantine::remove(mode, policy, db, request, action, &env)
}
