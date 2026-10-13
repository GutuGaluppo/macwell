//! Finds git repositories under folders the user chose. Symbolic links are never followed, and
//! folders that are regenerated or huge (`node_modules`, `target`…) are never entered.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use serde::Serialize;

/// Folder names never entered while looking for repositories.
pub const PRUNED: &[&str] = &[
    "node_modules", ".git", "target", ".next", "dist", "build", ".turbo", "DerivedData", ".venv",
    "Pods", "vendor", ".Trash", ".build",
];

pub const DEFAULT_MAX_DEPTH: usize = 6;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoRef {
    pub path: PathBuf,
    /// The user-chosen root it was found under.
    pub root: PathBuf,
    /// `.git` is a file (worktree or submodule) instead of a folder.
    pub git_file: bool,
    /// Closest enclosing repository found in the same walk, if any.
    pub parent: Option<PathBuf>,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Discovery {
    pub repos: Vec<RepoRef>,
    pub warnings: Vec<String>,
}

pub fn discover(roots: &[PathBuf], max_depth: usize, cancel: &AtomicBool) -> Discovery {
    let mut out = Discovery::default();
    let mut canonical: Vec<PathBuf> = Vec::new();
    for root in roots {
        if !root.is_absolute() {
            out.warnings.push(format!("{}: o caminho precisa ser absoluto", root.display()));
            continue;
        }
        match fs::canonicalize(root) {
            Ok(c) if c.is_dir() => canonical.push(c),
            Ok(_) => out.warnings.push(format!("{}: não é uma pasta", root.display())),
            Err(e) => out.warnings.push(format!("{}: {e}", root.display())),
        }
    }
    canonical.sort();
    canonical.dedup();
    // A root inside another root would report the same repositories twice.
    let roots: Vec<PathBuf> = canonical
        .iter()
        .filter(|r| !canonical.iter().any(|o| o != *r && r.starts_with(o)))
        .cloned()
        .collect();

    for root in &roots {
        walk(root, root, 0, max_depth, cancel, &mut out);
    }
    out.repos.sort_by(|a, b| a.path.cmp(&b.path));
    let paths: Vec<PathBuf> = out.repos.iter().map(|r| r.path.clone()).collect();
    for repo in &mut out.repos {
        repo.parent = paths
            .iter()
            .filter(|p| **p != repo.path && repo.path.starts_with(p))
            .max_by_key(|p| p.components().count())
            .cloned();
    }
    out
}

fn walk(dir: &Path, root: &Path, depth: usize, max_depth: usize, cancel: &AtomicBool, out: &mut Discovery) {
    if cancel.load(Ordering::Relaxed) {
        return;
    }
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) => {
            out.warnings.push(format!("{}: {e}", dir.display()));
            return;
        }
    };
    let mut subdirs = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Ok(kind) = entry.file_type() else { continue }; // does not follow symlinks
        if name == ".git" {
            if kind.is_dir() || kind.is_file() {
                out.repos.push(RepoRef {
                    path: dir.to_path_buf(),
                    root: root.to_path_buf(),
                    git_file: kind.is_file(),
                    parent: None,
                });
            }
            continue; // a symlinked .git is ignored on purpose
        }
        if kind.is_dir() && !PRUNED.iter().any(|p| name == *p) {
            subdirs.push(entry.path());
        }
    }
    if depth >= max_depth {
        return;
    }
    subdirs.sort();
    for sub in subdirs {
        walk(&sub, root, depth + 1, max_depth, cancel, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    fn tree() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        (dir, root)
    }

    fn repo(path: &Path) {
        fs::create_dir_all(path.join(".git")).unwrap();
    }

    fn found(d: &Discovery, root: &Path) -> Vec<String> {
        d.repos.iter().map(|r| r.path.strip_prefix(root).unwrap().display().to_string()).collect()
    }

    fn run(roots: &[PathBuf]) -> Discovery {
        discover(roots, DEFAULT_MAX_DEPTH, &AtomicBool::new(false))
    }

    #[test]
    fn finds_repos_and_prunes_regenerable_folders() {
        let (_d, root) = tree();
        repo(&root.join("a"));
        repo(&root.join("group/b"));
        repo(&root.join("a/node_modules/dep"));
        repo(&root.join("c/target/debug/x"));
        repo(&root.join("c/vendor/lib"));
        fs::create_dir_all(root.join("plain/dir")).unwrap();
        let d = run(&[root.clone()]);
        assert_eq!(found(&d, &root), ["a", "group/b"]);
    }

    #[test]
    fn never_follows_symlinks() {
        let (_d, root) = tree();
        let (_o, outside) = tree();
        repo(&outside.join("secret-repo"));
        fs::create_dir_all(root.join("work")).unwrap();
        symlink(&outside, root.join("work/link")).unwrap();
        symlink(outside.join("secret-repo"), root.join("direct")).unwrap();
        let d = run(&[root.clone()]);
        assert!(d.repos.is_empty(), "{:?}", d.repos);
    }

    #[test]
    fn symlinked_dot_git_is_not_a_repo() {
        let (_d, root) = tree();
        let (_o, outside) = tree();
        repo(&outside.join("real"));
        fs::create_dir_all(root.join("fake")).unwrap();
        symlink(outside.join("real/.git"), root.join("fake/.git")).unwrap();
        assert!(run(&[root]).repos.is_empty());
    }

    #[test]
    fn worktrees_have_a_dot_git_file() {
        let (_d, root) = tree();
        fs::create_dir_all(root.join("wt")).unwrap();
        fs::write(root.join("wt/.git"), "gitdir: /elsewhere/.git/worktrees/wt\n").unwrap();
        let d = run(&[root.clone()]);
        assert_eq!(found(&d, &root), ["wt"]);
        assert!(d.repos[0].git_file);
    }

    #[test]
    fn nested_repos_know_their_parent() {
        let (_d, root) = tree();
        repo(&root.join("mono"));
        repo(&root.join("mono/packages/inner"));
        repo(&root.join("mono/packages/inner/deep/deeper"));
        let d = run(&[root.clone()]);
        assert_eq!(found(&d, &root), ["mono", "mono/packages/inner", "mono/packages/inner/deep/deeper"]);
        assert_eq!(d.repos[0].parent, None);
        assert_eq!(d.repos[1].parent.as_deref(), Some(root.join("mono").as_path()));
        assert_eq!(d.repos[2].parent.as_deref(), Some(root.join("mono/packages/inner").as_path()));
    }

    #[test]
    fn depth_limit_overlapping_roots_and_bad_roots() {
        let (_d, root) = tree();
        repo(&root.join("l1/l2/l3"));
        let shallow = discover(&[root.clone()], 1, &AtomicBool::new(false));
        assert!(shallow.repos.is_empty());
        let deep = discover(&[root.clone()], 3, &AtomicBool::new(false));
        assert_eq!(deep.repos.len(), 1);
        // The same tree through two overlapping roots is reported once.
        let both = run(&[root.clone(), root.join("l1")]);
        assert_eq!(both.repos.len(), 1);
        let bad = run(&[PathBuf::from("relative"), root.join("missing")]);
        assert!(bad.repos.is_empty());
        assert_eq!(bad.warnings.len(), 2);
    }

    #[test]
    fn cancellation_stops_the_walk() {
        let (_d, root) = tree();
        repo(&root.join("a"));
        let d = discover(&[root], DEFAULT_MAX_DEPTH, &AtomicBool::new(true));
        assert!(d.repos.is_empty());
    }
}
