//! The read-only picture the Projects screen shows: every repository under the chosen roots, its
//! state, and the regenerable folders inside it.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;

use crate::db::{now_ms, SnapshotRecord};
use crate::dev::artifacts::{builtin_rules, scan_repo, Artifact};
use crate::dev::discover::{discover, DEFAULT_MAX_DEPTH};
use crate::dev::git::{Git, StatusInfo};
use crate::dev::visibility::{classify, redact, RemoteKind};
use crate::filesystem::scanner::ScanEvent;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteView {
    pub name: String,
    /// Credentials removed: this goes to the UI.
    pub url: String,
    pub kind: RemoteKind,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoReport {
    pub path: PathBuf,
    pub root: PathBuf,
    pub git_file: bool,
    pub parent: Option<PathBuf>,
    pub status: Option<StatusInfo>,
    pub remotes: Vec<RemoteView>,
    pub submodules: Vec<String>,
    pub embedded: Vec<String>,
    pub error: Option<String>,
    pub artifact_bytes: u64,
    pub last_snapshot: Option<SnapshotRecord>,
}

impl RepoReport {
    /// Why this repository holds work that exists only on this Mac, as neutral codes. Empty means
    /// nothing is at risk (or git could not read it). Keep in step with `unsyncedReasons` in
    /// src/features/dev/labels.ts.
    pub fn unsynced_reasons(&self) -> Vec<&'static str> {
        let Some(s) = &self.status else { return Vec::new() };
        let mut reasons = Vec::new();
        if s.uncommitted() > 0 {
            reasons.push("uncommitted");
        }
        if s.ahead.unwrap_or(0) > 0 {
            reasons.push("unpushedCommits");
        }
        if !s.no_commits {
            if self.remotes.is_empty() {
                reasons.push("noRemote");
            } else if s.upstream.is_none() && !s.detached {
                reasons.push("noUpstream");
            }
        }
        reasons
    }

    pub fn is_unsynced(&self) -> bool {
        !self.unsynced_reasons().is_empty()
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevReport {
    pub roots: Vec<String>,
    pub repos: Vec<RepoReport>,
    pub artifacts: Vec<Artifact>,
    pub git_error: Option<String>,
    pub scanned_at: i64,
}

/// Newest snapshot per repository path, from a list ordered newest first.
pub fn latest_by_repo(snapshots: Vec<SnapshotRecord>) -> HashMap<String, SnapshotRecord> {
    let mut map = HashMap::new();
    for s in snapshots {
        map.entry(s.repo.clone()).or_insert(s);
    }
    map
}

/// Builds the report. A repository that git cannot read stays in the list with its error; if git
/// itself is unavailable every repository carries that message and nothing else is attempted.
pub fn build(roots: &[PathBuf], git: &Git, last: &HashMap<String, SnapshotRecord>, cancel: &AtomicBool, emit: &mut dyn FnMut(ScanEvent)) -> DevReport {
    emit(ScanEvent::Stage { stage: "discover".into(), done: 0, total: 1 });
    let found = discover(roots, DEFAULT_MAX_DEPTH, cancel);
    for w in &found.warnings {
        emit(ScanEvent::Warning { path: String::new(), message: w.clone() });
    }
    let git_error = git.available().err().map(|e| e.to_string());
    let rules = builtin_rules();

    let total = found.repos.len() as u64;
    let mut repos = Vec::new();
    let mut artifacts = Vec::new();
    for (i, repo) in found.repos.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        emit(ScanEvent::Stage { stage: "repos".into(), done: i as u64, total });
        let mut report = RepoReport {
            path: repo.path.clone(),
            root: repo.root.clone(),
            git_file: repo.git_file,
            parent: repo.parent.clone(),
            status: None,
            remotes: Vec::new(),
            submodules: Vec::new(),
            embedded: Vec::new(),
            error: git_error.clone(),
            artifact_bytes: 0,
            last_snapshot: last.get(&repo.path.to_string_lossy().into_owned()).cloned(),
        };
        if git_error.is_none() {
            match git.state(&repo.path) {
                Ok(s) => {
                    report.status = Some(s.status);
                    report.remotes = s.remotes.into_iter().map(|r| RemoteView { kind: classify(&r.url), url: redact(&r.url), name: r.name }).collect();
                    report.submodules = s.submodules;
                    report.embedded = s.embedded;
                }
                Err(e) => report.error = Some(redact(&e.to_string())),
            }
            let found_artifacts = scan_repo(git, &repo.path, &rules, cancel);
            report.artifact_bytes = found_artifacts.iter().map(|a| a.size).sum();
            artifacts.extend(found_artifacts);
        }
        repos.push(report);
    }
    emit(ScanEvent::Stage { stage: "repos".into(), done: total, total });
    DevReport { roots: roots.iter().map(|r| r.display().to_string()).collect(), repos, artifacts, git_error, scanned_at: now_ms() }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UnsyncedRepo {
    pub path: PathBuf,
    pub reasons: Vec<&'static str>,
}

/// A light pass for the backup gate: only discovery and `git status`, no sizing of build folders.
/// Returns the repositories with work only on this Mac and how many git could not read.
pub fn unsynced_repos(roots: &[PathBuf], git: &Git, cancel: &AtomicBool) -> (Vec<UnsyncedRepo>, u32) {
    let found = discover(roots, DEFAULT_MAX_DEPTH, cancel);
    if git.available().is_err() {
        return (Vec::new(), found.repos.len() as u32);
    }
    let mut unsynced = Vec::new();
    let mut unreadable = 0;
    for repo in &found.repos {
        if cancel.load(Ordering::Relaxed) {
            break;
        }
        match git.state(&repo.path) {
            Ok(s) => {
                let report = RepoReport {
                    path: repo.path.clone(),
                    root: repo.root.clone(),
                    git_file: repo.git_file,
                    parent: repo.parent.clone(),
                    status: Some(s.status),
                    remotes: s.remotes.into_iter().map(|r| RemoteView { kind: classify(&r.url), url: String::new(), name: r.name }).collect(),
                    submodules: s.submodules,
                    embedded: s.embedded,
                    error: None,
                    artifact_bytes: 0,
                    last_snapshot: None,
                };
                let reasons = report.unsynced_reasons();
                if !reasons.is_empty() {
                    unsynced.push(UnsyncedRepo { path: repo.path.clone(), reasons });
                }
            }
            Err(_) => unreadable += 1,
        }
    }
    (unsynced, unreadable)
}
