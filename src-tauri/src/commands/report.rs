//! Report commands. The report is built here, from the app's own records and a fresh read of the backup
//! state, every time: the UI never sends content to be written, only where and in which format.

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{AppHandle, Manager};

use super::backup::compute_gate;
use crate::backup::exclusions::{listed_state, ListedState};
use crate::db::now_ms;
use crate::error::{AppError, AppResult};
use crate::exec::RealRunner;
use crate::report::export::write_report;
use crate::report::{build, render, Report, ReportFormat, ReportInput};
use crate::state::AppState;

const MAX_OPERATIONS: u32 = 20_000;

fn period(days: u32) -> AppResult<u32> {
    if (1..=365).contains(&days) { Ok(days) } else { Err(AppError::Invalid("o período precisa ficar entre 1 e 365 dias".into())) }
}

fn make_report(state: &AppState, period_days: u32) -> AppResult<Report> {
    let (operations, snapshots, quarantine, exclusions) = {
        let db = state.db.lock().unwrap();
        (db.operations(MAX_OPERATIONS)?, db.snapshots(None)?, db.quarantine_items()?, db.tm_exclusions()?)
    };
    let lost_exclusions = exclusions.into_iter().filter(|e| listed_state(&RealRunner, Path::new(&e.path)) == ListedState::Lost).map(|e| e.path).collect();
    let quarantine = quarantine.into_iter().map(|q| (q.origin, q.size, std::fs::symlink_metadata(&q.dest).is_ok())).collect();
    Ok(build(ReportInput { now_ms: now_ms(), period_days, operations, snapshots, lost_exclusions, quarantine, gate: compute_gate(state)? }))
}

/// The report as data, for the screen. Reads only.
#[tauri::command]
pub async fn generate_report(app: AppHandle, period_days: u32) -> AppResult<Report> {
    let days = period(period_days)?;
    tauri::async_runtime::spawn_blocking(move || make_report(&app.state::<AppState>(), days)).await.map_err(|e| AppError::Other(e.to_string()))?
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Exported {
    pub path: String,
    pub bytes: u64,
}

/// Writes the report where the user chose. Fails with the `exists` error kind if the file is there and
/// `overwrite` is false, so the screen can ask first.
#[tauri::command]
pub async fn export_report(app: AppHandle, format: ReportFormat, path: String, overwrite: bool, period_days: u32) -> AppResult<Exported> {
    let days = period(period_days)?;
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let target = PathBuf::from(&path);
        let policy = state.policy.read().unwrap().clone();
        // Check the destination before spending time on a fresh read of everything.
        let report = make_report(&state, days)?;
        let bytes = write_report(&policy, &target, format.extension(), &render(&report, format), overwrite)?;
        state.db.lock().unwrap().log_action("report", &path, true, Some(format.extension()))?;
        Ok(Exported { path, bytes })
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}
