//! Cleanup rule database (spec §16): each rule names locations under the home folder, a risk
//! level, the apps that must be closed, and whether it is selected by default. Built-in rules live
//! in `rules/cleanup/builtin.json`; users can add their own. Every match goes to the Trash, so
//! every rule can be rolled back from there.

use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::applications::leftovers::vendor_of;
use crate::filesystem::scanner::{scan, ScanOptions, VisitEntry};
use crate::filesystem::worker_pool;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Risk {
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupRule {
    pub id: String,
    /// "user", "developer", "browser", "mail" or "custom".
    pub group: String,
    pub title: String,
    #[serde(default)]
    pub description: String,
    /// Relative to the home folder; `*` matches exactly one path component.
    pub paths: Vec<String>,
    pub risk: Risk,
    /// Bundle ids that must not be running.
    #[serde(default)]
    pub requires_closed: Vec<String>,
    #[serde(default)]
    pub app: Option<String>,
    /// Check each match against running apps (for folders like ~/Library/Caches/*).
    #[serde(default)]
    pub per_item_running_check: bool,
    #[serde(default)]
    pub needs_full_disk_access: bool,
    #[serde(default)]
    pub selected: bool,
}

#[derive(Deserialize)]
struct RuleFile {
    rules: Vec<CleanupRule>,
}

pub fn builtin_rules() -> Vec<CleanupRule> {
    serde_json::from_str::<RuleFile>(include_str!("../../../rules/cleanup/builtin.json")).map(|f| f.rules).unwrap_or_default()
}

/// A user rule: one folder inside the home folder whose contents can be cleaned.
pub fn custom_rule(id: &str, title: &str, folder_rel: &str, risk: Risk) -> CleanupRule {
    CleanupRule {
        id: format!("custom:{id}"),
        group: "custom".into(),
        title: title.into(),
        description: String::new(),
        paths: vec![format!("{}/*", folder_rel.trim_end_matches('/'))],
        risk,
        requires_closed: Vec::new(),
        app: None,
        per_item_running_check: false,
        needs_full_disk_access: false,
        selected: false,
    }
}

/// Validates a custom rule folder: relative to home, no `..`, no wildcards, not the home folder.
pub fn valid_custom_folder(rel: &str) -> bool {
    let p = Path::new(rel);
    !rel.is_empty()
        && !p.is_absolute()
        && !rel.contains('*')
        && p.components().all(|c| matches!(c, Component::Normal(_)))
}

/// Expands a pattern where `*` matches one path component. Symlinks are never followed.
pub fn expand(home: &Path, pattern: &str) -> Vec<PathBuf> {
    let mut current = vec![home.to_path_buf()];
    for part in pattern.split('/').filter(|p| !p.is_empty()) {
        let mut next = Vec::new();
        for base in &current {
            if part == "*" {
                if let Ok(read) = std::fs::read_dir(base) {
                    next.extend(read.flatten().map(|e| e.path()).filter(|p| p.file_name().is_some_and(|n| n != ".DS_Store")));
                }
            } else {
                let candidate = base.join(part);
                // Intermediate components must be real folders (not symlinks); the last may be a file.
                if std::fs::symlink_metadata(&candidate).is_ok() {
                    next.push(candidate);
                }
            }
        }
        // Do not descend through symlinked folders.
        current = next.into_iter().filter(|p| std::fs::symlink_metadata(p).is_ok_and(|m| !m.file_type().is_symlink() || !m.is_dir())).collect();
    }
    current
}

/// Running apps as (bundle id, name) pairs.
pub fn running_apps() -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for exe in crate::applications::running_executables() {
        if let Some(bundle) = crate::processes::app_bundle_of(&exe) {
            if seen.insert(bundle.clone()) {
                let info = crate::applications::read_bundle(&bundle);
                if let Some(id) = info.bundle_id {
                    out.push((id, info.name.unwrap_or_default()));
                }
            }
        }
    }
    out
}

/// The running app a cache folder belongs to, if any: exact bundle id, bundle id prefix, the
/// vendor folder ("Google" while Chrome runs) or the app's name.
pub fn owner_running(name: &str, running: &[(String, String)]) -> Option<String> {
    let lname = name.to_lowercase();
    running.iter().find_map(|(id, app)| {
        let lid = id.to_lowercase();
        let vendor = vendor_of(id);
        let hit = lname == lid
            || lname.starts_with(&format!("{lid}."))
            || lid.starts_with(&format!("{lname}."))
            || vendor.as_deref().is_some_and(|v| v == lname.replace(' ', ""))
            || (!app.is_empty() && lname == app.to_lowercase());
        hit.then(|| if app.is_empty() { id.clone() } else { app.clone() })
    })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleItem {
    pub path: String,
    pub size: u64,
    pub selected: bool,
    /// Why this item cannot be cleaned right now (an app is open).
    pub blocked: Option<String>,
    /// macOS's own cache: offered, never preselected.
    pub system: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleResult {
    #[serde(flatten)]
    pub rule: CleanupRule,
    pub items: Vec<RuleItem>,
    pub size: u64,
    /// Apps from `requiresClosed` that are open now.
    pub blocked_by: Vec<String>,
    pub unavailable: bool,
}

pub fn size_of(path: &Path) -> u64 {
    let meta = std::fs::symlink_metadata(path);
    if meta.as_ref().is_ok_and(|m| !m.is_dir()) {
        return meta.map(|m| m.len()).unwrap_or(0);
    }
    let never = Arc::new(AtomicBool::new(false));
    let mut size = 0;
    let _ = scan(&ScanOptions { root: path.to_path_buf(), ..Default::default() }, &never, &mut |e: &VisitEntry| {
        if e.first_link {
            size += e.meta.size_logical;
        }
    }, |_| {});
    size
}

pub fn evaluate(home: &Path, rules: &[CleanupRule], running: &[(String, String)], full_disk_access: bool) -> Vec<RuleResult> {
    worker_pool().install(|| {
        rules
            .par_iter()
            .map(|rule| {
                let blocked_by: Vec<String> = running
                    .iter()
                    .filter(|(id, _)| rule.requires_closed.iter().any(|r| r.eq_ignore_ascii_case(id)))
                    .map(|(id, name)| if name.is_empty() { id.clone() } else { name.clone() })
                    .collect();
                let unavailable = rule.needs_full_disk_access && !full_disk_access;
                let mut paths: Vec<PathBuf> = if unavailable { Vec::new() } else { rule.paths.iter().flat_map(|p| expand(home, p)).collect() };
                paths.sort();
                paths.dedup();
                let mut items: Vec<RuleItem> = paths
                    .into_par_iter()
                    .map(|p| {
                        let name = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                        let system = rule.per_item_running_check && name.starts_with("com.apple.");
                        let blocked = if !blocked_by.is_empty() {
                            Some(blocked_by.join(", "))
                        } else if rule.per_item_running_check {
                            owner_running(&name, running)
                        } else {
                            None
                        };
                        RuleItem { size: size_of(&p), path: p.display().to_string(), selected: rule.selected && blocked.is_none() && !system, blocked, system }
                    })
                    .filter(|i| i.size > 0)
                    .collect();
                items.sort_by_key(|i| std::cmp::Reverse(i.size));
                let size = items.iter().map(|i| i.size).sum();
                RuleResult { rule: rule.clone(), items, size, blocked_by, unavailable }
            })
            .collect()
    })
}

/// Re-validation before removal: `path` must still be a match of `rule`, and nothing that must be
/// closed may be running.
pub fn still_allowed(home: &Path, rule: &CleanupRule, path: &Path, running: &[(String, String)]) -> Result<(), String> {
    if !rule.paths.iter().any(|p| expand(home, p).iter().any(|m| m == path)) {
        return Err("no longer matches the rule".into());
    }
    if let Some((_, name)) = running.iter().find(|(id, _)| rule.requires_closed.iter().any(|r| r.eq_ignore_ascii_case(id))) {
        return Err(format!("quit {name} first"));
    }
    if rule.per_item_running_check {
        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        if let Some(app) = owner_running(&name, running) {
            return Err(format!("{app} is open"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn builtin_rules_parse_and_stay_in_home() {
        let rules = builtin_rules();
        assert!(rules.len() >= 10);
        for r in &rules {
            for p in &r.paths {
                assert!(!p.starts_with('/') && !p.contains(".."), "{p}");
            }
        }
        // Firefox bookmarks and history live in places.sqlite: never a target.
        assert!(rules.iter().all(|r| r.paths.iter().all(|p| !p.contains("places.sqlite"))));
    }

    #[test]
    fn expansion_and_evaluation() {
        let dir = tempfile::tempdir().unwrap();
        let home = fs::canonicalize(dir.path()).unwrap();
        for (p, n) in [
            ("Library/Caches/com.foo.App/data", 100),
            ("Library/Caches/Google/Chrome/Default/cache", 300),
            ("Library/Caches/com.apple.Safari/x", 50),
            ("Library/Logs/app.log", 20),
            ("Library/Application Support/Google/Chrome/Default/History", 70),
            ("Library/Application Support/Google/Chrome/Profile 1/History", 30),
        ] {
            let path = home.join(p);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, vec![0u8; n]).unwrap();
        }
        let hist = expand(&home, "Library/Application Support/Google/Chrome/*/History");
        assert_eq!(hist.len(), 2);

        let rules = builtin_rules();
        let running = vec![("com.google.Chrome".to_string(), "Google Chrome".to_string())];
        let results = evaluate(&home, &rules, &running, false);
        let get = |id: &str| results.iter().find(|r| r.rule.id == id).unwrap();

        let caches = get("user-caches");
        let google = caches.items.iter().find(|i| i.path.ends_with("/Google")).unwrap();
        assert_eq!(google.blocked.as_deref(), Some("Google Chrome"), "vendor folder of a running app is blocked");
        assert!(!google.selected);
        let apple = caches.items.iter().find(|i| i.path.ends_with("com.apple.Safari")).unwrap();
        assert!(apple.system && !apple.selected);
        let foo = caches.items.iter().find(|i| i.path.ends_with("com.foo.App")).unwrap();
        assert!(foo.selected && foo.blocked.is_none());

        let chrome_history = get("chrome-history");
        assert_eq!(chrome_history.items.len(), 2);
        assert_eq!(chrome_history.blocked_by, vec!["Google Chrome"]);
        assert!(chrome_history.items.iter().all(|i| !i.selected));

        assert!(get("mail-downloads").unavailable);

        // Re-validation.
        let log_rule = rules.iter().find(|r| r.id == "user-logs").unwrap();
        assert!(still_allowed(&home, log_rule, &home.join("Library/Logs/app.log"), &running).is_ok());
        assert!(still_allowed(&home, log_rule, &home.join("Library/Caches/com.foo.App"), &running).is_err());
        let chrome_rule = rules.iter().find(|r| r.id == "chrome-history").unwrap();
        assert!(still_allowed(&home, chrome_rule, &hist[0], &running).is_err());
        assert!(still_allowed(&home, chrome_rule, &hist[0], &[]).is_ok());
    }

    #[test]
    fn custom_folders() {
        assert!(valid_custom_folder("Projects/old-builds"));
        assert!(!valid_custom_folder("/Users/x/Projects"));
        assert!(!valid_custom_folder("../other"));
        assert!(!valid_custom_folder("Projects/*"));
        assert!(!valid_custom_folder(""));
        let r = custom_rule("a", "Builds", "Projects/builds/", Risk::Medium);
        assert_eq!(r.paths, vec!["Projects/builds/*"]);
        assert!(!r.selected);
    }
}
