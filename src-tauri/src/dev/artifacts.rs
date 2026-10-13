//! Regenerable folders inside projects (`node_modules`, `target`, `.next`…). A folder only counts
//! when a marker file next to it proves the project type, and anything that looks versioned or
//! cannot be rebuilt identically is demoted to "review" instead of being offered as safe.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::UNIX_EPOCH;

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::cleanup::rules::size_of;
use crate::dev::discover::PRUNED;
use crate::dev::git::Git;
use crate::filesystem::worker_pool;
use crate::types::Confidence;

const DEFAULT_RULES: &str = include_str!("../../../rules/dev/artifacts.json");
pub const MAX_DEPTH: usize = 6;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactRule {
    pub name: String,
    pub kind: String,
    pub markers: Vec<String>,
    #[serde(default)]
    pub lockfiles: Vec<String>,
    #[serde(default)]
    pub ambiguous: bool,
    #[serde(default)]
    pub cleanup_rule_id: Option<String>,
    pub reason: String,
}

#[derive(Deserialize)]
struct RuleFile {
    rules: Vec<ArtifactRule>,
}

pub fn builtin_rules() -> Vec<ArtifactRule> {
    serde_json::from_str::<RuleFile>(DEFAULT_RULES).map(|f| f.rules).unwrap_or_default()
}

/// Neutral observations the UI translates; any of them keeps the item out of the default selection.
pub const NOTE_NO_LOCKFILE: &str = "noLockfile";
pub const NOTE_TRACKED: &str = "tracked";
pub const NOTE_NOT_IGNORED: &str = "notIgnored";
pub const NOTE_GIT_UNKNOWN: &str = "gitUnknown";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Artifact {
    pub path: PathBuf,
    /// The folder that holds the marker files.
    pub project: PathBuf,
    pub repo: PathBuf,
    pub kind: String,
    pub size: u64,
    /// Latest of the project's manifest and lockfile times and the last commit (milliseconds).
    pub last_used_ms: Option<i64>,
    pub reason: String,
    pub confidence: Confidence,
    pub notes: Vec<String>,
    pub cleanup_rule_id: Option<String>,
}

/// A folder that matches a rule, before sizing and git checks.
#[derive(Debug, Clone)]
pub struct Candidate {
    pub path: PathBuf,
    pub project: PathBuf,
    pub rule: usize,
    pub has_lockfile: bool,
    pub project_files: Vec<PathBuf>,
}

fn name_matches(pattern: &str, name: &str) -> bool {
    match pattern.strip_prefix('*') {
        Some(suffix) => name.ends_with(suffix) && name.len() > suffix.len(),
        None => pattern == name,
    }
}

fn any_match(patterns: &[String], names: &HashSet<String>) -> bool {
    patterns.iter().any(|p| names.iter().any(|n| name_matches(p, n)))
}

/// Filesystem pass only: finds candidates under `repo` without following symlinks, without entering
/// the candidates themselves and without crossing into nested repositories.
pub fn find_candidates(repo: &Path, rules: &[ArtifactRule], cancel: &AtomicBool) -> Vec<Candidate> {
    let mut out = Vec::new();
    walk(repo, repo, 0, rules, cancel, &mut out);
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

fn walk(dir: &Path, repo: &Path, depth: usize, rules: &[ArtifactRule], cancel: &AtomicBool, out: &mut Vec<Candidate>) {
    if cancel.load(Ordering::Relaxed) {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else { return };
    let mut names = HashSet::new();
    let mut subdirs = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if entry.file_type().is_ok_and(|t| t.is_dir()) {
            subdirs.push((name.clone(), entry.path()));
        }
        names.insert(name);
    }
    subdirs.sort();
    for (name, path) in subdirs {
        let rule = rules.iter().position(|r| r.name == name && any_match(&r.markers, &names));
        if let Some(i) = rule {
            let r = &rules[i];
            let mut project_files: Vec<PathBuf> = names
                .iter()
                .filter(|n| r.markers.iter().chain(&r.lockfiles).any(|p| name_matches(p, n)))
                .map(|n| dir.join(n))
                .collect();
            // Workspaces keep one lockfile at the root for every package below it.
            let inherited = if any_match(&r.lockfiles, &names) { None } else { ancestor_lockfile(dir, repo, &r.lockfiles) };
            let has_lockfile = inherited.is_some() || any_match(&r.lockfiles, &names);
            project_files.extend(inherited);
            out.push(Candidate { path, project: dir.to_path_buf(), rule: i, has_lockfile, project_files });
            continue;
        }
        if PRUNED.contains(&name.as_str()) || depth >= MAX_DEPTH {
            continue;
        }
        if fs::symlink_metadata(path.join(".git")).is_ok() {
            continue; // a nested repository is reported on its own
        }
        walk(&path, repo, depth + 1, rules, cancel, out);
    }
}

/// A lockfile in a folder above `dir`, up to and including `repo`.
fn ancestor_lockfile(dir: &Path, repo: &Path, lockfiles: &[String]) -> Option<PathBuf> {
    dir.ancestors()
        .skip(1)
        .take_while(|a| a.starts_with(repo))
        .find_map(|a| lockfiles.iter().map(|l| a.join(l)).find(|p| fs::symlink_metadata(p).is_ok_and(|m| m.is_file())))
}

fn mtime_ms(path: &Path) -> Option<i64> {
    let t = fs::symlink_metadata(path).ok()?.modified().ok()?;
    Some(t.duration_since(UNIX_EPOCH).ok()?.as_millis() as i64)
}

fn repo_relative(repo: &Path, c: &Candidate) -> String {
    c.path.strip_prefix(repo).map(|p| p.to_string_lossy().into_owned()).unwrap_or_default()
}

/// The neutral observations for `candidates`, in the same order, from the folder layout and git.
/// Empty means nothing speaks against the folder being plain, regenerable output. This is the one
/// place the rule lives: both the scan and the server-side check before a removal use it.
pub fn assess_notes(git: &Git, repo: &Path, rules: &[ArtifactRule], candidates: &[Candidate]) -> Vec<Vec<String>> {
    if candidates.is_empty() {
        return Vec::new();
    }
    let rels: Vec<String> = candidates.iter().map(|c| repo_relative(repo, c)).collect();
    let tracked = git.tracked_under(repo, &rels);
    let ambiguous: Vec<String> =
        candidates.iter().zip(&rels).filter(|(c, _)| rules[c.rule].ambiguous).map(|(_, r)| r.clone()).collect();
    let ignored = if ambiguous.is_empty() { Ok(Vec::new()) } else { git.ignored_among(repo, &ambiguous) };

    candidates
        .iter()
        .zip(&rels)
        .map(|(c, rel)| {
            let rule = &rules[c.rule];
            let mut notes = Vec::new();
            if !rule.lockfiles.is_empty() && !c.has_lockfile {
                notes.push(NOTE_NO_LOCKFILE.to_string());
            }
            match &tracked {
                Ok(files) if files.iter().any(|f| f.starts_with(&format!("{rel}/"))) => notes.push(NOTE_TRACKED.to_string()),
                Ok(_) => {}
                Err(_) => notes.push(NOTE_GIT_UNKNOWN.to_string()),
            }
            if rule.ambiguous {
                match &ignored {
                    Ok(list) if list.iter().any(|i| i.trim_end_matches('/') == rel) => {}
                    Ok(_) => notes.push(NOTE_NOT_IGNORED.to_string()),
                    Err(_) if !notes.contains(&NOTE_GIT_UNKNOWN.to_string()) => notes.push(NOTE_GIT_UNKNOWN.to_string()),
                    Err(_) => {}
                }
            }
            notes
        })
        .collect()
}

/// Full assessment of one repository: finds candidates, sizes them in parallel and applies the git
/// checks. Never modifies anything.
pub fn scan_repo(git: &Git, repo: &Path, rules: &[ArtifactRule], cancel: &AtomicBool) -> Vec<Artifact> {
    let candidates = find_candidates(repo, rules, cancel);
    if candidates.is_empty() {
        return Vec::new();
    }
    let notes = assess_notes(git, repo, rules, &candidates);
    let commit_ms = git.last_commit_ms(repo);
    let sizes: Vec<u64> = worker_pool().install(|| candidates.par_iter().map(|c| size_of(&c.path)).collect());

    let mut artifacts: Vec<Artifact> = candidates
        .iter()
        .zip(notes)
        .zip(sizes)
        .map(|((c, notes), size)| {
            let rule = &rules[c.rule];
            let last_used_ms = c.project_files.iter().filter_map(|f| mtime_ms(f)).chain(commit_ms).max();
            Artifact {
                path: c.path.clone(),
                project: c.project.clone(),
                repo: repo.to_path_buf(),
                kind: rule.kind.clone(),
                size,
                last_used_ms,
                reason: rule.reason.clone(),
                confidence: if notes.is_empty() { Confidence::Safe } else { Confidence::Review },
                notes,
                cleanup_rule_id: rule.cleanup_rule_id.clone(),
            }
        })
        .collect();
    artifacts.sort_by(|a, b| b.size.cmp(&a.size).then_with(|| a.path.cmp(&b.path)));
    artifacts
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn builtin_rules_parse_and_are_consistent() {
        let rules = builtin_rules();
        assert!(rules.len() >= 10, "rules file did not parse");
        for r in &rules {
            assert!(!r.name.contains('/') && !r.name.is_empty(), "{}", r.name);
            assert!(!r.markers.is_empty(), "{} needs a marker", r.kind);
            assert!(!r.reason.is_empty());
        }
        let kinds: HashSet<_> = rules.iter().map(|r| &r.kind).collect();
        assert_eq!(kinds.len(), rules.len(), "kinds must be unique");
    }

    #[test]
    fn linked_cleanup_rules_exist_in_the_builtin_cleanup_file() {
        let cleanup: HashSet<String> = crate::cleanup::rules::builtin_rules().into_iter().map(|r| r.id).collect();
        for r in builtin_rules() {
            if let Some(id) = r.cleanup_rule_id {
                assert!(cleanup.contains(&id), "{id} is not in rules/cleanup/builtin.json");
            }
        }
    }

    #[test]
    fn marker_patterns() {
        assert!(name_matches("*.xcodeproj", "App.xcodeproj"));
        assert!(!name_matches("*.xcodeproj", ".xcodeproj"));
        assert!(name_matches("package.json", "package.json"));
        assert!(!name_matches("package.json", "package.json.bak"));
    }
}
