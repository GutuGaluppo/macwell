//! Developer Care commands (M12). Every path from the UI is checked against the folders the user
//! chose before git is pointed at it, and everything that writes goes through the safety policy.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use tauri::ipc::Channel;
use tauri::{AppHandle, Manager};

use super::{start_job, JobEvent, JobOutcome};
use crate::db::{Db, DevRoot, SnapshotRecord};
use crate::cleanup::{self, RemovalRequest};
use crate::dev::artifacts::builtin_rules;
use crate::dev::git::Git;
use crate::dev::removal::{revalidate, ArtifactRemoval};
use crate::dev::push::{self, PushConsent, PushPlan, PushResult};
use crate::dev::report::{build, latest_by_repo, DevReport};
use crate::dev::secrets::{self, SecretScan};
use crate::dev::snapshot::{self, SnapshotPlan, SnapshotResult};
use crate::dev::visibility::{redact, RemoteKind, UreqProbe, Visibility};
use crate::dev::{resolve_repo, validate_root};
use crate::error::{AppError, AppResult};
use crate::exec::RealRunner;
use crate::filesystem::safety::{MutationKind, RemovalContext, SafetyPolicy};
use crate::filesystem::scanner::ScanEvent;
use crate::state::AppState;
use crate::types::OperationOutcome;

pub(crate) fn roots(state: &AppState) -> AppResult<Vec<PathBuf>> {
    Ok(state.db.lock().unwrap().dev_roots()?.into_iter().map(|r| PathBuf::from(r.path)).collect())
}

/// A repository inside one of the user's roots (read-only use).
fn repo_in_roots(state: &AppState, repo: &str) -> AppResult<PathBuf> {
    resolve_repo(&roots(state)?, Path::new(repo))
}

/// Same, for actions that write into the repository: the safety policy has to agree as well.
fn repo_for_write(state: &AppState, repo: &str) -> AppResult<PathBuf> {
    let path = repo_in_roots(state, repo)?;
    state.policy.read().unwrap().check_mutation(&path, &RemovalContext::default(), MutationKind::ExistingItem)?;
    Ok(path)
}

fn blocking<T: Send + 'static>(f: impl FnOnce() -> AppResult<T> + Send + 'static) -> impl std::future::Future<Output = AppResult<T>> {
    async move { tauri::async_runtime::spawn_blocking(f).await.map_err(|e| AppError::Other(e.to_string()))? }
}

#[tauri::command]
pub fn list_dev_roots(state: tauri::State<'_, AppState>) -> AppResult<Vec<DevRoot>> {
    state.db.lock().unwrap().dev_roots()
}

#[tauri::command]
pub fn add_dev_root(state: tauri::State<'_, AppState>, path: String) -> AppResult<()> {
    let own = [state.paths.data_dir.clone(), state.paths.log_dir.clone()];
    let canonical = validate_root(Path::new(&path), &own)?;
    state.db.lock().unwrap().add_dev_root(&canonical.to_string_lossy())
}

#[tauri::command]
pub fn remove_dev_root(state: tauri::State<'_, AppState>, path: String) -> AppResult<()> {
    state.db.lock().unwrap().remove_dev_root(&path)
}

/// Read-only scan of the chosen folders: repositories, their state and regenerable folders.
#[tauri::command]
pub fn start_dev_scan(app: AppHandle, on_event: Channel<JobEvent<DevReport>>) -> AppResult<String> {
    let roots = roots(&app.state::<AppState>())?;
    if roots.is_empty() {
        return Err(AppError::Invalid("adicione uma pasta de projetos primeiro".into()));
    }
    Ok(start_job(&app, on_event, "dev", move |state, cancel, emit| {
        emit(ScanEvent::Start { root: roots[0].display().to_string() });
        let last = latest_by_repo(state.db.lock().unwrap().snapshots(None)?);
        let runner = RealRunner;
        let report = build(&roots, &Git::new(&runner), &last, cancel, emit);
        Ok(JobOutcome { cancelled: cancel.load(std::sync::atomic::Ordering::Relaxed), result: report })
    }))
}

/// Paths and kinds of anything that looks like a secret in what a snapshot would contain.
#[tauri::command]
pub async fn scan_repo_secrets(app: AppHandle, repo: String) -> AppResult<SecretScan> {
    blocking(move || {
        let state = app.state::<AppState>();
        let repo = repo_in_roots(&state, &repo)?;
        let runner = RealRunner;
        secrets::scan_repo(&Git::new(&runner), &repo, &AtomicBool::new(false))
    })
    .await
}

#[tauri::command]
pub async fn plan_wip_snapshot(app: AppHandle, repo: String, excludes: Vec<String>) -> AppResult<SnapshotPlan> {
    blocking(move || {
        let state = app.state::<AppState>();
        let repo = repo_in_roots(&state, &repo)?;
        let runner = RealRunner;
        snapshot::plan(&Git::new(&runner), &repo, &excludes, &chrono::Local::now(), &AtomicBool::new(false))
    })
    .await
}

/// Creates the snapshot branch. The plan is derived again here, so only what passes now is made.
#[tauri::command]
pub async fn create_wip_snapshot(app: AppHandle, repo: String, excludes: Vec<String>) -> AppResult<SnapshotResult> {
    blocking(move || {
        let state = app.state::<AppState>();
        let repo = repo_for_write(&state, &repo)?;
        let runner = RealRunner;
        let tmp = state.paths.data_dir.join("tmp");
        let result = snapshot::create(&Git::new(&runner), &repo, &excludes, &tmp, &chrono::Local::now(), &AtomicBool::new(false));
        let db = state.db.lock().unwrap();
        let repo_str = repo.to_string_lossy();
        match &result {
            Ok(r) => {
                db.record_snapshot(&repo_str, &r.ref_name, &r.commit)?;
                db.log_action("snapshot", &repo_str, true, Some(&r.ref_name))?;
            }
            Err(e) => db.log_action("snapshot", &repo_str, false, Some(&e.to_string()))?,
        }
        result
    })
    .await
}

#[tauri::command]
pub fn list_dev_snapshots(state: tauri::State<'_, AppState>, repo: Option<String>) -> AppResult<Vec<SnapshotRecord>> {
    state.db.lock().unwrap().snapshots(repo.as_deref())
}

/// The answer the user got earlier for this remote's push URL, if any.
fn known_visibility(state: &AppState, git: &Git, repo: &Path, remote: &str) -> AppResult<Option<(Visibility, i64)>> {
    let url = redact(&git.push_url(repo, remote)?);
    let cached = state.db.lock().unwrap().visibility(&repo.to_string_lossy(), &url)?;
    Ok(cached.map(|c| (Visibility::parse(&c.visibility), c.checked_at)))
}

#[tauri::command]
pub async fn plan_push_snapshot(app: AppHandle, repo: String, ref_name: String, remote: String) -> AppResult<PushPlan> {
    blocking(move || {
        let state = app.state::<AppState>();
        let repo = repo_in_roots(&state, &repo)?;
        let runner = RealRunner;
        let git = Git::new(&runner);
        let known = known_visibility(&state, &git, &repo, &remote)?;
        push::plan_push(&git, &repo, &ref_name, &remote, known)
    })
    .await
}

/// The only command that talks to the network on its own initiative, and only when the user presses
/// the button: one anonymous request to api.github.com for a github.com remote.
#[tauri::command]
pub async fn check_remote_visibility(app: AppHandle, repo: String, remote: String, ref_name: String) -> AppResult<PushPlan> {
    blocking(move || {
        let state = app.state::<AppState>();
        let repo = repo_in_roots(&state, &repo)?;
        let runner = RealRunner;
        let git = Git::new(&runner);
        // Validates the branch and the remote name before anything leaves the machine.
        let before = push::plan_push(&git, &repo, &ref_name, &remote, None)?;
        let RemoteKind::Github { owner, repo: name } = &before.kind else {
            return Err(AppError::Invalid("só é possível verificar remotes do github.com".into()));
        };
        let visibility = crate::dev::visibility::probe_github(owner, name, &UreqProbe);
        state.db.lock().unwrap().set_visibility(&repo.to_string_lossy(), &before.url, visibility.as_str())?;
        push::plan_push(&git, &repo, &ref_name, &remote, known_visibility(&state, &git, &repo, &remote)?)
    })
    .await
}

/// Sends one snapshot branch to a configured remote. Needs both consents; see `dev::push`.
#[tauri::command]
pub async fn push_snapshot(app: AppHandle, repo: String, ref_name: String, remote: String, confirmed: bool, accept_exposure: bool) -> AppResult<PushResult> {
    blocking(move || {
        let state = app.state::<AppState>();
        let repo = repo_for_write(&state, &repo)?;
        let runner = RealRunner;
        let git = Git::new(&runner);
        let known = known_visibility(&state, &git, &repo, &remote)?;
        let result = push::push(&git, &repo, &ref_name, &remote, known, PushConsent { confirmed, accept_exposure });
        let db = state.db.lock().unwrap();
        let repo_str = repo.to_string_lossy();
        match &result {
            Ok(r) => {
                db.mark_snapshot_pushed_by_ref(&repo_str, &r.ref_name)?;
                db.log_action("push", &repo_str, true, Some(&format!("{} -> {}", r.ref_name, r.remote)))?;
            }
            Err(e) => db.log_action("push", &repo_str, false, Some(&e.to_string()))?,
        }
        result
    })
    .await
}

/// Moves approved regenerable folders to the Trash. Used by the Projects screen and by Smart Care.
/// Every item is re-derived first (see `dev::removal`) and then goes through `move_to_trash`, so the
/// safety policy, the user's ignore list and the operation log all apply.
pub fn remove_artifacts(state: &AppState, items: Vec<ArtifactRemoval>) -> AppResult<Vec<OperationOutcome>> {
    remove_artifacts_with(state, items, &|policy, db, request| cleanup::move_to_trash(policy, db, request, "trash"))
}

/// The same, with the mover injected so tests can run without touching the real Trash.
pub fn remove_artifacts_with(
    state: &AppState,
    items: Vec<ArtifactRemoval>,
    mover: &dyn Fn(&SafetyPolicy, &Db, &RemovalRequest) -> Vec<OperationOutcome>,
) -> AppResult<Vec<OperationOutcome>> {
    let roots = roots(state)?;
    let rules = builtin_rules();
    let policy = state.policy.read().unwrap().clone();
    let runner = RealRunner;
    let git = Git::new(&runner);
    let cancel = AtomicBool::new(false);
    let fail = |items: &[ArtifactRemoval], why: &str| -> Vec<OperationOutcome> {
        items.iter().map(|i| OperationOutcome { path: i.path.clone(), size: i.size, ok: false, error: Some(why.to_string()) }).collect()
    };

    let mut by_repo: BTreeMap<String, Vec<ArtifactRemoval>> = BTreeMap::new();
    for item in items {
        by_repo.entry(item.repo.clone()).or_default().push(item);
    }
    let mut outcomes = Vec::new();
    for (repo, group) in by_repo {
        let repo_path = match resolve_repo(&roots, Path::new(&repo)) {
            Ok(p) => p,
            Err(e) => {
                outcomes.extend(fail(&group, &e.to_string()));
                continue;
            }
        };
        let (inside, outside): (Vec<_>, Vec<_>) = group.into_iter().partition(|i| Path::new(&i.path).starts_with(&repo_path));
        outcomes.extend(fail(&outside, "a pasta não está dentro do repositório informado"));
        let checked = revalidate(&git, &repo_path, &inside, &rules, &cancel);
        outcomes.extend(checked.rejected);
        if checked.accepted.is_empty() {
            continue;
        }
        let moved = {
            let db = state.db.lock().unwrap();
            mover(&policy, &db, &RemovalRequest { items: checked.accepted, scan_root: Some(repo_path.display().to_string()) })
        };
        let removed: Vec<PathBuf> = moved.iter().filter(|o| o.ok).map(|o| PathBuf::from(&o.path)).collect();
        state.forget_paths(&removed);
        outcomes.extend(moved);
    }
    Ok(outcomes)
}

/// Moves the regenerable folders the user approved to the Trash.
#[tauri::command]
pub async fn remove_dev_artifacts(app: AppHandle, items: Vec<ArtifactRemoval>) -> AppResult<Vec<OperationOutcome>> {
    blocking(move || remove_artifacts(&app.state::<AppState>(), items)).await
}
