//! Homebrew analysis (read only). Runs `brew outdated` and looks at the user's project folders; it
//! never upgrades, installs or removes anything.

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use tauri::{AppHandle, Manager};

use crate::brew::{analyze, find_brew, HomebrewReport};
use crate::dev::discover::{discover, DEFAULT_MAX_DEPTH};
use crate::error::{AppError, AppResult};
use crate::exec::RealRunner;
use crate::state::AppState;

#[tauri::command]
pub async fn analyze_homebrew(app: AppHandle) -> AppResult<HomebrewReport> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let roots: Vec<PathBuf> = super::dev::roots(&state)?;
        let repos: Vec<PathBuf> = if roots.is_empty() { Vec::new() } else { discover(&roots, DEFAULT_MAX_DEPTH, &AtomicBool::new(false)).repos.into_iter().map(|r| r.path).collect() };
        Ok(analyze(&RealRunner, find_brew(), &repos))
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}
