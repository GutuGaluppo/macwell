//! WIP snapshot: a commit on a new local branch `wip/snapshot-<date>` holding the working tree as it
//! is right now, built with git plumbing and a temporary index kept outside the repository.
//!
//! The working tree, the real index, HEAD and the current branch are never touched. What is written
//! is new objects under `.git/objects` and one new ref. Nothing is pushed here.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use chrono::{DateTime, Local};
use serde::Serialize;

use crate::dev::git::Git;
use crate::dev::secrets::{is_excluded, scan_files, SecretFinding};
use crate::error::{AppError, AppResult};

/// A new (untracked) file above this size must be excluded first; it would bloat `.git` for good.
pub const MAX_FILE_BYTES: u64 = 50 * 1024 * 1024;
/// Same for the sum of all new files.
pub const MAX_TOTAL_BYTES: u64 = 500 * 1024 * 1024;
const LIST_LIMIT: usize = 200;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "code", rename_all = "camelCase")]
pub enum Blocker {
    /// Untracked files that look like secrets and are not excluded.
    Secrets { count: u32 },
    FileTooBig { path: String, size: u64 },
    TotalTooBig { bytes: u64 },
    /// A clean filter (for example Git LFS) would run external programs during `git add`.
    FiltersConfigured,
    NothingToSave,
    /// All three names for today are taken.
    NoFreeRefName,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileSize {
    pub path: String,
    pub size: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotPlan {
    pub repo: PathBuf,
    pub branch: Option<String>,
    pub head: Option<String>,
    /// Without the `refs/heads/` prefix.
    pub ref_name: String,
    /// Tracked files with changes (the snapshot captures them as they are on disk).
    pub changed: u32,
    pub untracked_count: u32,
    pub untracked_bytes: u64,
    /// The largest new files, at most 200, so the user can approve the exact list.
    pub untracked: Vec<FileSize>,
    pub excludes: Vec<String>,
    /// Untracked, not excluded: these block the snapshot until excluded.
    pub secrets: Vec<SecretFinding>,
    /// Secrets already in the repository; reported, not blocking.
    pub tracked_secrets: u32,
    pub warnings: Vec<String>,
    pub blockers: Vec<Blocker>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotResult {
    pub repo: PathBuf,
    pub ref_name: String,
    pub commit: String,
    pub tree: String,
    pub parent: Option<String>,
    pub warnings: Vec<String>,
}

/// Excluded paths are relative, inside the repository, and never pathspec syntax.
pub fn validate_excludes(excludes: &[String]) -> AppResult<Vec<String>> {
    excludes
        .iter()
        .map(|e| {
            let e = e.trim_end_matches('/');
            let bad = e.is_empty() || e.starts_with('/') || e.contains('\0') || e.split('/').any(|c| c == ".." || c.is_empty());
            if bad {
                Err(AppError::Invalid(format!("caminho de exclusão inválido: {e}")))
            } else {
                Ok(e.to_string())
            }
        })
        .collect()
}

fn ref_candidates(now: &DateTime<Local>) -> [String; 3] {
    let day = now.format("%Y-%m-%d");
    [format!("wip/snapshot-{day}"), format!("wip/snapshot-{day}-{}", now.format("%H%M")), format!("wip/snapshot-{day}-{}", now.format("%H%M%S"))]
}

fn free_ref_name(git: &Git, repo: &Path, now: &DateTime<Local>) -> Option<String> {
    ref_candidates(now).into_iter().find(|name| git.rev_parse(repo, &format!("refs/heads/{name}")).is_none())
}

/// Everything the user needs to approve, computed without writing anything.
pub fn plan(git: &Git, repo: &Path, excludes: &[String], now: &DateTime<Local>, cancel: &AtomicBool) -> AppResult<SnapshotPlan> {
    let excludes = validate_excludes(excludes)?;
    let state = git.state(repo)?;
    let head = git.rev_parse(repo, "HEAD");
    let (tracked, untracked) = git.snapshot_files(repo)?;

    let mut sized: Vec<FileSize> = untracked
        .iter()
        .filter(|p| !is_excluded(p, &excludes))
        .map(|p| FileSize { path: p.clone(), size: fs::symlink_metadata(repo.join(p)).ok().filter(|m| m.is_file()).map_or(0, |m| m.len()) })
        .collect();
    let untracked_count = sized.len() as u32;
    let untracked_bytes: u64 = sized.iter().map(|f| f.size).sum();
    sized.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.path.cmp(&b.path)));

    let files: Vec<(String, bool)> = tracked.into_iter().map(|p| (p, true)).chain(untracked.into_iter().map(|p| (p, false))).collect();
    let found = scan_files(repo, &files, cancel).findings;
    let live: Vec<SecretFinding> = found.into_iter().filter(|f| !is_excluded(&f.path, &excludes)).collect();
    let tracked_secrets = live.iter().filter(|f| f.tracked).count() as u32;
    let secrets: Vec<SecretFinding> = live.into_iter().filter(|f| !f.tracked).collect();

    let ref_name = free_ref_name(git, repo, now);
    let changed = state.status.changed + state.status.conflicted;

    let mut blockers = Vec::new();
    if !secrets.is_empty() {
        blockers.push(Blocker::Secrets { count: secrets.len() as u32 });
    }
    blockers.extend(sized.iter().filter(|f| f.size > MAX_FILE_BYTES).map(|f| Blocker::FileTooBig { path: f.path.clone(), size: f.size }));
    if untracked_bytes > MAX_TOTAL_BYTES {
        blockers.push(Blocker::TotalTooBig { bytes: untracked_bytes });
    }
    if git.filters_in_use(repo)? {
        blockers.push(Blocker::FiltersConfigured);
    }
    if changed == 0 && untracked_count == 0 {
        blockers.push(Blocker::NothingToSave);
    }
    if ref_name.is_none() {
        blockers.push(Blocker::NoFreeRefName);
    }

    sized.truncate(LIST_LIMIT);
    Ok(SnapshotPlan {
        repo: repo.to_path_buf(),
        branch: state.status.branch,
        head,
        ref_name: ref_name.unwrap_or_default(),
        changed,
        untracked_count,
        untracked_bytes,
        untracked: sized,
        excludes,
        secrets,
        tracked_secrets,
        warnings: Vec::new(),
        blockers,
    })
}

/// Removes the temporary index (and a stale lock) however the function ends.
struct TempIndex(PathBuf);

impl Drop for TempIndex {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
        let _ = fs::remove_file(self.0.with_extension("idx.lock"));
    }
}

/// Builds the snapshot. The plan is derived again here; the caller's earlier preview is never trusted.
/// `tmp_dir` must be outside the repository (the app data folder).
pub fn create(git: &Git, repo: &Path, excludes: &[String], tmp_dir: &Path, now: &DateTime<Local>, cancel: &AtomicBool) -> AppResult<SnapshotResult> {
    let plan = plan(git, repo, excludes, now, cancel)?;
    if !plan.blockers.is_empty() {
        return Err(AppError::Invalid(format!("o snapshot não pode ser criado: {}", serde_json::to_string(&plan.blockers).unwrap_or_default())));
    }
    if tmp_dir.starts_with(repo) {
        return Err(AppError::Invalid("o índice temporário não pode ficar dentro do repositório".into()));
    }
    fs::create_dir_all(tmp_dir)?;
    let index = TempIndex(tmp_dir.join(format!("snapshot-{}.idx", uuid::Uuid::new_v4())));
    let index_path = index.0.to_str().ok_or_else(|| AppError::Invalid("caminho temporário não é UTF-8".into()))?.to_string();
    let with_index = [("GIT_INDEX_FILE", index_path.as_str())];

    // 1. Seed the private index from HEAD, so files that are tracked but ignored stay tracked.
    if plan.head.is_some() {
        git.exec_ok(repo, &["read-tree", "HEAD"], &with_index, true)?;
    }
    // 2. Stage the working tree into that private index only. Excluded paths are left as HEAD has them.
    let exclude_specs: Vec<String> = plan.excludes.iter().map(|e| format!(":(exclude,literal){e}")).collect();
    let mut add = vec!["add", "-A", "--", "."];
    add.extend(exclude_specs.iter().map(String::as_str));
    let added = git.exec_ok(repo, &add, &with_index, false)?;
    let mut warnings = Vec::new();
    if added.stderr_str().contains("embedded git repository") {
        warnings.push("embeddedRepository".to_string());
    }
    // 3. Tree and commit.
    let tree = git.exec_ok(repo, &["write-tree"], &with_index, true)?.stdout_str().trim().to_string();
    if git.rev_parse(repo, "HEAD^{tree}").as_deref() == Some(tree.as_str()) {
        return Err(AppError::Invalid("nada para salvar: a árvore é igual à do último commit".into()));
    }
    let message = format!(
        "WIP snapshot {}\n\nCreated by OrganizeMyMac from {} at {}.\nThe working tree, the index and the branch were not changed.\n",
        now.format("%Y-%m-%d %H:%M"),
        plan.branch.as_deref().unwrap_or("a detached HEAD"),
        plan.head.as_deref().map_or("an empty repository".to_string(), |h| h.chars().take(10).collect()),
    );
    let mut commit_args = vec!["-c", "commit.gpgsign=false", "commit-tree", tree.as_str()];
    if let Some(h) = plan.head.as_deref() {
        commit_args.extend(["-p", h]);
    }
    commit_args.extend(["-m", message.as_str()]);
    let identity = git.identity_env(repo);
    let identity: Vec<(&str, &str)> = identity.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    let commit = git.exec_ok(repo, &commit_args, &identity, true)?.stdout_str().trim().to_string();
    // 4. The only ref that is written. An empty old value means "must not exist yet".
    let full_ref = format!("refs/heads/{}", plan.ref_name);
    git.exec_ok(repo, &["update-ref", "-m", "OrganizeMyMac WIP snapshot", full_ref.as_str(), commit.as_str(), ""], &[], true)?;

    Ok(SnapshotResult { repo: repo.to_path_buf(), ref_name: plan.ref_name, commit, tree, parent: plan.head, warnings })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn excludes_must_be_plain_relative_paths() {
        assert_eq!(validate_excludes(&[".env".into(), "keys/".into()]).unwrap(), [".env", "keys"]);
        for bad in ["", "/etc/passwd", "../x", "a/../b", "a//b", "/"] {
            assert!(validate_excludes(&[bad.into()]).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn ref_names_use_date_then_time() {
        let now = Local.with_ymd_and_hms(2024, 5, 6, 7, 8, 9).unwrap();
        assert_eq!(ref_candidates(&now), ["wip/snapshot-2024-05-06", "wip/snapshot-2024-05-06-0708", "wip/snapshot-2024-05-06-070809"]);
    }
}
