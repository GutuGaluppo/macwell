use std::path::PathBuf;

use serde::Serialize;
use tauri::ipc::Channel;
use tauri::AppHandle;

use super::scan::{record, scan_options, validate_root, ScanResult};
use super::{start_job, JobEvent, JobOutcome};
use crate::cleanup::{self, RemovalItem, RemovalRequest};
use crate::db::now_ms;
use crate::duplicates::{unchanged, validate_group, DuplicateCandidates, DuplicateGroup, DuplicateSearch, GroupRemoval, HashStats};
use crate::backup::gate::BackupAck;
use crate::commands::backup::require_backup_ack;
use crate::error::{AppError, AppResult};
use crate::filesystem::scanner::scan;
use crate::state::AppState;
use crate::types::OperationOutcome;

/// Groups sent to the UI; the rest only count in the totals.
const MAX_GROUPS: usize = 3_000;

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DuplicatesResult {
    #[serde(flatten)]
    pub scan: ScanResult,
    pub groups: Vec<DuplicateGroup>,
    pub total_groups: u64,
    pub wasted_bytes: u64,
    pub hash_stats: HashStats,
}

#[tauri::command]
pub fn start_duplicate_scan(app: AppHandle, root: String, min_size: u64, on_event: Channel<JobEvent<DuplicatesResult>>) -> AppResult<String> {
    let root = validate_root(&root)?;
    Ok(start_job(&app, on_event, "duplicates", move |state, cancel, emit| {
        let started_at = now_ms();
        let mut candidates = DuplicateCandidates::new(min_size);
        let stats = scan(&scan_options(state, &root), cancel, &mut candidates, &mut *emit)?;
        let empty = |stats| ScanResult { id: String::new(), started_at, finished_at: now_ms(), stats, reclaimable_bytes: 0, largest_files: Vec::new(), tree: None };
        if stats.cancelled {
            let result = DuplicatesResult { scan: empty(stats), groups: Vec::new(), total_groups: 0, wasted_bytes: 0, hash_stats: HashStats::default() };
            return Ok(JobOutcome { cancelled: true, result });
        }
        let search = DuplicateSearch { home: &state.paths.home, cancel };
        let (mut groups, hash_stats) = match search.run(candidates, emit) {
            Ok(r) => r,
            Err(AppError::Cancelled) => {
                let mut stats = stats;
                stats.cancelled = true;
                let result = DuplicatesResult { scan: empty(stats), groups: Vec::new(), total_groups: 0, wasted_bytes: 0, hash_stats: HashStats::default() };
                return Ok(JobOutcome { cancelled: true, result });
            }
            Err(e) => return Err(e),
        };
        let wasted_bytes = groups.iter().map(|g| g.wasted).sum();
        let total_groups = groups.len() as u64;
        groups.truncate(MAX_GROUPS);
        let mut scan = empty(stats);
        scan.id = uuid::Uuid::new_v4().to_string();
        scan.reclaimable_bytes = wasted_bytes;
        record(state, "duplicates", &scan)?;
        Ok(JobOutcome { cancelled: false, result: DuplicatesResult { scan, groups, total_groups, wasted_bytes, hash_stats } })
    }))
}

/// Moves selected copies to the Trash. A group that would lose every copy is refused as a whole,
/// and a copy that changed since the scan is skipped.
#[tauri::command]
pub async fn move_duplicates_to_trash(app: AppHandle, groups: Vec<GroupRemoval>, scan_root: Option<String>, backup_ack: Option<BackupAck>) -> AppResult<Vec<OperationOutcome>> {
    tauri::async_runtime::spawn_blocking(move || {
        use tauri::Manager;
        let state = app.state::<AppState>();
        require_backup_ack(&state, backup_ack.as_ref())?;
        let mut outcomes = Vec::new();
        let mut items = Vec::new();
        for g in &groups {
            if let Err(rule) = validate_group(g) {
                outcomes.extend(g.remove.iter().map(|r| OperationOutcome { path: r.path.clone(), size: r.size, ok: false, error: Some(rule.to_string()) }));
                continue;
            }
            for r in &g.remove {
                if unchanged(r) {
                    items.push(RemovalItem { path: r.path.clone(), size: r.size });
                } else {
                    outcomes.push(OperationOutcome { path: r.path.clone(), size: r.size, ok: false, error: Some("the file changed since the scan".into()) });
                }
            }
        }
        let policy = state.policy.read().unwrap().clone();
        let moved = {
            let db = state.db.lock().unwrap();
            cleanup::move_to_trash(&policy, &db, &RemovalRequest { items, scan_root }, "trash")
        };
        let removed: Vec<PathBuf> = moved.iter().filter(|o| o.ok).map(|o| PathBuf::from(&o.path)).collect();
        state.forget_paths(&removed);
        outcomes.extend(moved);
        Ok(outcomes)
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}
