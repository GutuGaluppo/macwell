//! Removing regenerable folders. The UI only says which folders the user approved; here each one is
//! re-derived from the disk and git before it may reach the Trash, so a stale or forged list cannot
//! remove anything that is not a regenerable folder right now.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use serde::Deserialize;

use crate::cleanup::RemovalItem;
use crate::dev::artifacts::{assess_notes, find_candidates, ArtifactRule, NOTE_GIT_UNKNOWN, NOTE_TRACKED};
use crate::dev::git::Git;
use crate::types::OperationOutcome;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactRemoval {
    pub path: String,
    pub repo: String,
    pub kind: String,
    /// Size shown to the user; recorded in the log.
    pub size: u64,
}

#[derive(Debug, Default)]
pub struct Revalidated {
    pub accepted: Vec<RemovalItem>,
    pub rejected: Vec<OperationOutcome>,
}

/// Which of `items` (all inside `repo`) may still be removed. Rejected ones carry the reason.
///
/// A folder passes only if it is still found by the same rule that listed it (marker next to it, a
/// real directory, not a symlink, not inside another artifact or repository) and git does not know
/// versioned files in it. Reviewable observations such as a missing lockfile do not block: the
/// user already saw them and chose the item explicitly.
pub fn revalidate(git: &Git, repo: &Path, items: &[ArtifactRemoval], rules: &[ArtifactRule], cancel: &AtomicBool) -> Revalidated {
    let candidates = find_candidates(repo, rules, cancel);
    let mut out = Revalidated::default();
    let mut matched = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    for item in items {
        let reject = |why: &str| OperationOutcome { path: item.path.clone(), size: item.size, ok: false, error: Some(why.to_string()) };
        if seen.contains(&item.path.as_str()) {
            continue; // the same folder twice is one folder
        }
        seen.push(&item.path);
        let path = PathBuf::from(&item.path);
        match candidates.iter().find(|c| c.path == path && rules[c.rule].kind == item.kind) {
            Some(c) => matched.push((item, c.clone())),
            None => out.rejected.push(reject("não é mais uma pasta regenerável deste projeto")),
        }
    }
    let cands: Vec<_> = matched.iter().map(|(_, c)| c.clone()).collect();
    for ((item, _), notes) in matched.iter().zip(assess_notes(git, repo, rules, &cands)) {
        let reject = |why: &str| OperationOutcome { path: item.path.clone(), size: item.size, ok: false, error: Some(why.to_string()) };
        if notes.iter().any(|n| n == NOTE_TRACKED) {
            out.rejected.push(reject("tem arquivos versionados no git: remova pelo git, não por aqui"));
        } else if notes.iter().any(|n| n == NOTE_GIT_UNKNOWN) {
            out.rejected.push(reject("não foi possível confirmar no git que esta pasta pode ser recriada"));
        } else {
            out.accepted.push(RemovalItem { path: item.path.clone(), size: item.size });
        }
    }
    out
}
