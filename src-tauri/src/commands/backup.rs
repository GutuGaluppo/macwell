//! Backup Guard commands (M13). Reading only: nothing here changes Time Machine, iCloud or the disk.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use serde::Serialize;

use tauri::{AppHandle, Manager};

use crate::backup::exclusions::{self, listed_state, ListedState, Preview, PreviewItem, Undone};
use crate::backup::gate::{evaluate, verify, BackupAck, BackupGate, GateInput};
use crate::backup::timemachine::DEFAULT_STALE_DAYS;
use crate::backup::{read_state, BackupState};
use crate::dev::artifacts::builtin_rules;
use crate::dev::git::Git;
use crate::dev::removal::ArtifactRemoval;
use crate::dev::resolve_repo;
use crate::dev::report::unsynced_repos;
use crate::db::now_ms;
use crate::error::{AppError, AppResult};
use crate::exec::RealRunner;
use crate::filesystem::probe_full_disk_access;
use crate::state::AppState;
use crate::types::OperationOutcome;

const STALE_KEY: &str = "backup.staleDays";
const MAX_STALE_DAYS: u32 = 90;

fn stale_days(state: &AppState) -> u32 {
    state
        .db
        .lock()
        .unwrap()
        .setting(STALE_KEY)
        .ok()
        .flatten()
        .and_then(|v| v.parse::<u32>().ok())
        .filter(|d| (1..=MAX_STALE_DAYS).contains(d))
        .unwrap_or(DEFAULT_STALE_DAYS)
}

/// Time Machine and iCloud state, and which sources could not be read and why.
#[tauri::command]
pub async fn get_backup_state(app: AppHandle) -> AppResult<BackupState> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let days = stale_days(&state);
        let fda = probe_full_disk_access(&state.paths.home);
        Ok(read_state(&RealRunner, &state.paths.home, now_ms(), days, fda))
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

/// After how many days without a completed backup the app calls it old.
#[tauri::command]
pub fn set_backup_stale_days(state: tauri::State<'_, AppState>, days: u32) -> AppResult<()> {
    if !(1..=MAX_STALE_DAYS).contains(&days) {
        return Err(AppError::Invalid(format!("o limite precisa ficar entre 1 e {MAX_STALE_DAYS} dias")));
    }
    state.db.lock().unwrap().set_setting(STALE_KEY, &days.to_string())
}

/// The gate, from fresh reads: the backup situation and the repositories with work only on this Mac.
pub fn compute_gate(state: &AppState) -> AppResult<BackupGate> {
    let backup = read_state(&RealRunner, &state.paths.home, now_ms(), stale_days(state), probe_full_disk_access(&state.paths.home));
    let roots: Vec<PathBuf> = state.db.lock().unwrap().dev_roots()?.into_iter().map(|r| PathBuf::from(r.path)).collect();
    let (unpushed, unreadable_repos) =
        if roots.is_empty() { (Vec::new(), 0) } else { unsynced_repos(&roots, &Git::new(&RealRunner), &AtomicBool::new(false)) };
    Ok(evaluate(GateInput { backup, unpushed, unreadable_repos }))
}

/// First line of every batch cleanup command. The answer from the UI is only a claim: the gate is
/// computed again here, so a command called without the dialog is refused exactly the same way.
pub fn require_backup_ack(state: &AppState, ack: Option<&BackupAck>) -> AppResult<()> {
    verify(&compute_gate(state)?, ack)
}

/// What the dialog before a batch cleanup shows. Reads only.
#[tauri::command]
pub async fn get_backup_gate(app: AppHandle) -> AppResult<BackupGate> {
    tauri::async_runtime::spawn_blocking(move || compute_gate(&app.state::<AppState>())).await.map_err(|e| AppError::Other(e.to_string()))?
}

// ---- Time Machine exclusions for regenerable folders

/// Groups the request by repository, resolving each one against the user's project folders. Items
/// whose repository is not valid, or whose path is not inside it, come back as rejected.
fn by_repo(state: &AppState, items: Vec<ArtifactRemoval>) -> AppResult<(Vec<(PathBuf, Vec<ArtifactRemoval>)>, Vec<(ArtifactRemoval, String)>)> {
    let roots = super::dev::roots(state)?;
    let mut groups: BTreeMap<String, Vec<ArtifactRemoval>> = BTreeMap::new();
    for item in items {
        groups.entry(item.repo.clone()).or_default().push(item);
    }
    let (mut ok, mut rejected) = (Vec::new(), Vec::new());
    for (repo, group) in groups {
        match resolve_repo(&roots, Path::new(&repo)) {
            Err(e) => rejected.extend(group.into_iter().map(|i| (i, e.to_string()))),
            Ok(path) => {
                let (inside, outside): (Vec<_>, Vec<_>) = group.into_iter().partition(|i| Path::new(&i.path).starts_with(&path));
                rejected.extend(outside.into_iter().map(|i| (i, "a pasta não está dentro do repositório informado".to_string())));
                ok.push((path, inside));
            }
        }
    }
    Ok((ok, rejected))
}

/// What excluding these folders would do: which are valid, their size, and whether they are already out.
#[tauri::command]
pub async fn preview_tm_exclusions(app: AppHandle, items: Vec<ArtifactRemoval>) -> AppResult<Preview> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let policy = state.policy.read().unwrap().clone();
        let (groups, rejected) = by_repo(&state, items)?;
        let (rules, runner, cancel) = (builtin_rules(), RealRunner, AtomicBool::new(false));
        let git = Git::new(&runner);
        let mut out: Vec<PreviewItem> = rejected
            .into_iter()
            .map(|(i, why)| PreviewItem { path: i.path, repo: i.repo, kind: i.kind, size: i.size, state: exclusions::ExclusionState::Unknown, ok: false, reason: Some(why) })
            .collect();
        for (repo, group) in groups {
            out.extend(exclusions::preview(&runner, &git, &policy, &repo, &group, &rules, &cancel));
        }
        Ok(Preview::new(out))
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

/// Excludes the approved folders from Time Machine. Changes a system setting, so it is only done for the
/// list the user approved and every folder is checked again first.
#[tauri::command]
pub async fn apply_tm_exclusions(app: AppHandle, items: Vec<ArtifactRemoval>) -> AppResult<Vec<OperationOutcome>> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let policy = state.policy.read().unwrap().clone();
        let (groups, rejected) = by_repo(&state, items)?;
        let (rules, runner, cancel) = (builtin_rules(), RealRunner, AtomicBool::new(false));
        let git = Git::new(&runner);
        let mut outcomes: Vec<OperationOutcome> =
            rejected.into_iter().map(|(i, why)| OperationOutcome { path: i.path, size: i.size, ok: false, error: Some(why) }).collect();
        for (repo, group) in groups {
            for a in exclusions::apply(&runner, &git, &policy, &repo, &group, &rules, &cancel) {
                let db = state.db.lock().unwrap();
                if a.ok {
                    db.record_tm_exclusion(&a.path, &a.kind)?;
                }
                db.log_action("tmExclude", &a.path, a.ok, a.error.as_deref().or(Some(a.kind.as_str())))?;
                outcomes.push(OperationOutcome { path: a.path, size: a.size, ok: a.ok, error: a.error });
            }
        }
        Ok(outcomes)
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TmExclusionView {
    pub path: String,
    pub added_at: i64,
    pub kind: Option<String>,
    pub state: ListedState,
}

/// The exclusions this app added, with whether each is still in effect.
#[tauri::command]
pub async fn list_tm_exclusions(app: AppHandle) -> AppResult<Vec<TmExclusionView>> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let records = state.db.lock().unwrap().tm_exclusions()?;
        Ok(records
            .into_iter()
            .map(|r| TmExclusionView { state: listed_state(&RealRunner, Path::new(&r.path)), path: r.path, added_at: r.added_at, kind: r.kind })
            .collect())
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

/// Puts folders back in the backup. Only exclusions this app added can be undone here.
#[tauri::command]
pub async fn undo_tm_exclusions(app: AppHandle, paths: Vec<String>) -> AppResult<Vec<Undone>> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let policy = state.policy.read().unwrap().clone();
        let ours: Vec<String> = state.db.lock().unwrap().tm_exclusions()?.into_iter().map(|r| r.path).collect();
        let (mine, foreign): (Vec<String>, Vec<String>) = paths.into_iter().partition(|p| ours.contains(p));
        let mut out: Vec<Undone> = foreign
            .into_iter()
            .map(|path| Undone { path, ok: false, gone: false, still_excluded: false, error: Some("esta exclusão não foi feita pelo OrganizeMyMac".to_string()) })
            .collect();
        for u in exclusions::undo(&RealRunner, &policy, &mine) {
            let db = state.db.lock().unwrap();
            if u.ok {
                db.mark_tm_exclusion_removed(&u.path)?;
            }
            db.log_action("tmUnexclude", &u.path, u.ok, u.error.as_deref())?;
            out.push(u);
        }
        Ok(out)
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}
