//! Time Machine exclusions for regenerable folders (`node_modules`, `target`…): they are rebuilt by a
//! command, so they only make the backup bigger and slower.
//!
//! This is the one place the app changes a setting of the system, so it is deliberately narrow:
//!
//! * only folders that the regenerable-folder rules still recognise right now, checked on the server
//!   (the same check as before a removal), and never ones with versioned files;
//! * only the "sticky" exclusion, `tmutil addexclusion <path>`, which marks the folder itself, needs
//!   no root and no Full Disk Access. The fixed-path form (`-p`) needs root and is never used;
//! * every change goes through the safety policy, is verified with `tmutil isexcluded`, is recorded,
//!   and can be undone. Only exclusions this app added are ever removed.
//!
//! A sticky exclusion lives on the folder. If the folder is deleted and recreated (a clean `npm ci`),
//! the exclusion is gone: the list the app keeps shows when that happened.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use serde::Serialize;

use super::run_text;
use crate::cleanup::rules::size_of;
use crate::dev::artifacts::ArtifactRule;
use crate::dev::git::Git;
use crate::dev::removal::{revalidate, ArtifactRemoval};
use crate::exec::CommandRunner;
use crate::filesystem::safety::{MutationKind, RemovalContext, SafetyPolicy};

const TMUTIL: &str = "/usr/bin/tmutil";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ExclusionState {
    Included,
    Excluded,
    /// `tmutil` could not be asked.
    Unknown,
}

/// `tmutil isexcluded` always exits 0: the answer is in the text, `[Excluded]  path` or `[Included]  path`.
pub fn parse_isexcluded(text: &str) -> ExclusionState {
    for line in text.lines() {
        let l = line.trim_start();
        if l.starts_with("[Excluded]") {
            return ExclusionState::Excluded;
        }
        if l.starts_with("[Included]") {
            return ExclusionState::Included;
        }
    }
    ExclusionState::Unknown
}

pub fn state_of(tm: &dyn CommandRunner, path: &Path) -> ExclusionState {
    let Some(p) = path.to_str() else { return ExclusionState::Unknown };
    run_text(tm, TMUTIL, &["isexcluded", p], 10).map(|t| parse_isexcluded(&t)).unwrap_or(ExclusionState::Unknown)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreviewItem {
    pub path: String,
    pub repo: String,
    pub kind: String,
    pub size: u64,
    pub state: ExclusionState,
    /// Can be excluded now. When false, `reason` says why.
    pub ok: bool,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub items: Vec<PreviewItem>,
    /// Folders that would be excluded.
    pub count: usize,
    /// How much would stop being copied.
    pub bytes: u64,
}

impl Preview {
    pub fn new(items: Vec<PreviewItem>) -> Self {
        let ok: Vec<_> = items.iter().filter(|i| i.ok).collect();
        Preview { count: ok.len(), bytes: ok.iter().map(|i| i.size).sum(), items }
    }
}

fn context(repo: &Path) -> RemovalContext {
    RemovalContext { scan_root: Some(repo.to_path_buf()), allow_system_library: false }
}

/// What excluding `items` (all inside `repo`) would do. Reads only.
pub fn preview(tm: &dyn CommandRunner, git: &Git, policy: &SafetyPolicy, repo: &Path, items: &[ArtifactRemoval], rules: &[ArtifactRule], cancel: &AtomicBool) -> Vec<PreviewItem> {
    let checked = revalidate(git, repo, items, rules, cancel);
    let meta = |path: &str| items.iter().find(|i| i.path == path);
    let mut out: Vec<PreviewItem> = checked
        .rejected
        .iter()
        .map(|r| {
            let m = meta(&r.path);
            PreviewItem {
                path: r.path.clone(),
                repo: m.map(|m| m.repo.clone()).unwrap_or_default(),
                kind: m.map(|m| m.kind.clone()).unwrap_or_default(),
                size: r.size,
                state: ExclusionState::Unknown,
                ok: false,
                reason: r.error.clone(),
            }
        })
        .collect();
    for a in &checked.accepted {
        let m = meta(&a.path);
        let path = PathBuf::from(&a.path);
        let (state, ok, reason) = match policy.check_mutation(&path, &context(repo), MutationKind::ExistingItem) {
            Err(e) => (ExclusionState::Unknown, false, Some(e.to_string())),
            Ok(_) => match state_of(tm, &path) {
                ExclusionState::Excluded => (ExclusionState::Excluded, false, Some("já está fora do backup".to_string())),
                ExclusionState::Included => (ExclusionState::Included, true, None),
                ExclusionState::Unknown => (ExclusionState::Unknown, false, Some("não foi possível perguntar ao Time Machine".to_string())),
            },
        };
        out.push(PreviewItem {
            path: a.path.clone(),
            repo: m.map(|m| m.repo.clone()).unwrap_or_default(),
            kind: m.map(|m| m.kind.clone()).unwrap_or_default(),
            size: if ok { size_of(&path) } else { a.size },
            state,
            ok,
            reason,
        });
    }
    // Same order as the request.
    out.sort_by_key(|i| items.iter().position(|r| r.path == i.path).unwrap_or(usize::MAX));
    out
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Applied {
    pub path: String,
    pub kind: String,
    pub size: u64,
    pub ok: bool,
    pub error: Option<String>,
}

/// Excludes the approved folders that still pass every check. The preview is derived again here: the
/// list the user approved is only a request.
pub fn apply(tm: &dyn CommandRunner, git: &Git, policy: &SafetyPolicy, repo: &Path, items: &[ArtifactRemoval], rules: &[ArtifactRule], cancel: &AtomicBool) -> Vec<Applied> {
    preview(tm, git, policy, repo, items, rules, cancel)
        .into_iter()
        .map(|p| {
            let done = |ok: bool, error: Option<String>| Applied { path: p.path.clone(), kind: p.kind.clone(), size: p.size, ok, error };
            if !p.ok {
                return done(false, p.reason.clone());
            }
            if let Err((reason, detail)) = run_text(tm, TMUTIL, &["addexclusion", &p.path], 15) {
                return done(false, Some(format!("o Time Machine recusou: {}", detail.unwrap_or_else(|| format!("{reason:?}")))));
            }
            // Trust the system's answer, not the exit code.
            match state_of(tm, Path::new(&p.path)) {
                ExclusionState::Excluded => done(true, None),
                _ => done(false, Some("o comando terminou, mas a pasta não ficou fora do backup".to_string())),
            }
        })
        .collect()
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Undone {
    pub path: String,
    pub ok: bool,
    /// The folder does not exist any more, so there was nothing to undo.
    pub gone: bool,
    /// Our mark is gone but something else (a parent folder, a macOS rule) still keeps it out of the backup.
    pub still_excluded: bool,
    pub error: Option<String>,
}

/// Removes exclusions this app added. The caller passes only paths from the app's own list.
pub fn undo(tm: &dyn CommandRunner, policy: &SafetyPolicy, paths: &[String]) -> Vec<Undone> {
    paths
        .iter()
        .map(|p| {
            let res = |ok: bool, gone: bool, still: bool, error: Option<String>| Undone { path: p.clone(), ok, gone, still_excluded: still, error };
            let path = PathBuf::from(p);
            if std::fs::symlink_metadata(&path).is_err() {
                return res(true, true, false, None);
            }
            if let Err(e) = policy.check_mutation(&path, &RemovalContext::default(), MutationKind::ExistingItem) {
                return res(false, false, false, Some(e.to_string()));
            }
            if let Err((reason, detail)) = run_text(tm, TMUTIL, &["removeexclusion", p], 15) {
                return res(false, false, false, Some(format!("o Time Machine recusou: {}", detail.unwrap_or_else(|| format!("{reason:?}")))));
            }
            match state_of(tm, &path) {
                ExclusionState::Included => res(true, false, false, None),
                ExclusionState::Excluded => res(true, false, true, None),
                ExclusionState::Unknown => res(false, false, false, Some("não foi possível confirmar com o Time Machine".to_string())),
            }
        })
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ListedState {
    /// Still out of the backup.
    Active,
    /// The folder exists but is in the backup again, usually because it was deleted and recreated.
    Lost,
    /// The folder is gone.
    Missing,
    Unknown,
}

pub fn listed_state(tm: &dyn CommandRunner, path: &Path) -> ListedState {
    if std::fs::symlink_metadata(path).is_err() {
        return ListedState::Missing;
    }
    match state_of(tm, path) {
        ExclusionState::Excluded => ListedState::Active,
        ExclusionState::Included => ListedState::Lost,
        ExclusionState::Unknown => ListedState::Unknown,
    }
}
