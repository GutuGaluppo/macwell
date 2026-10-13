mod common;

use std::sync::atomic::AtomicBool;

use common::git::{canonical_tmp, commit_file, git, init, repo_with_commit, write};
use organizemymac_lib::dev::discover::{discover, DEFAULT_MAX_DEPTH};
use organizemymac_lib::dev::git::{Git, RepoState};
use organizemymac_lib::exec::RealRunner;

fn state(path: &std::path::Path) -> RepoState {
    Git::new(&RealRunner).state(path).unwrap()
}

#[test]
fn git_is_available_on_this_machine() {
    Git::new(&RealRunner).available().unwrap();
}

#[test]
fn clean_repo_without_remote() {
    let (_t, root) = canonical_tmp();
    repo_with_commit(&root.join("r"));
    let s = state(&root.join("r"));
    assert_eq!(s.status.branch.as_deref(), Some("main"));
    assert_eq!(s.status.uncommitted(), 0);
    assert!(s.remotes.is_empty());
    assert_eq!((s.status.upstream, s.status.ahead, s.status.behind), (None, None, None));
    assert!(s.submodules.is_empty() && s.embedded.is_empty());
}

#[test]
fn dirty_repo_counts_staged_modified_and_untracked() {
    let (_t, root) = canonical_tmp();
    let r = root.join("r");
    repo_with_commit(&r);
    commit_file(&r, "b.txt", "b\n");
    commit_file(&r, ".gitignore", "*.log\n");
    write(&r, "README.md", "changed\n"); // modified, unstaged
    write(&r, "c.txt", "c\n");
    git(&r, &["add", "c.txt"]); // staged new file
    write(&r, "new/d.txt", "d\n"); // untracked
    write(&r, "debug.log", "ignored\n");
    let s = state(&r).status;
    assert_eq!((s.changed, s.untracked, s.conflicted), (2, 1, 0));
}

#[test]
fn ahead_and_behind_use_local_data_only() {
    let (_t, root) = canonical_tmp();
    let bare = root.join("origin.git");
    std::fs::create_dir_all(&bare).unwrap();
    git(&bare, &["init", "-q", "--bare", "-b", "main"]);
    let a = root.join("a");
    repo_with_commit(&a);
    git(&a, &["remote", "add", "origin", bare.to_str().unwrap()]);
    git(&a, &["push", "-q", "-u", "origin", "main"]);
    git(&root, &["clone", "-q", bare.to_str().unwrap(), "b"]);
    let b = root.join("b");

    commit_file(&b, "local.txt", "x\n");
    let s = state(&b);
    assert_eq!((s.status.ahead, s.status.behind), (Some(1), Some(0)));
    assert_eq!(s.status.upstream.as_deref(), Some("origin/main"));
    assert_eq!(s.remotes[0].name, "origin");

    // The remote moves on, but without a fetch the app must not know about it.
    commit_file(&a, "remote.txt", "y\n");
    git(&a, &["push", "-q"]);
    assert_eq!(state(&b).status.behind, Some(0));
    git(&b, &["fetch", "-q"]);
    assert_eq!((state(&b).status.ahead, state(&b).status.behind), (Some(1), Some(1)));
}

#[test]
fn no_commits_and_detached_head() {
    let (_t, root) = canonical_tmp();
    init(&root.join("empty"));
    let e = state(&root.join("empty")).status;
    assert!(e.no_commits);
    assert_eq!(e.branch.as_deref(), Some("main"));

    let r = root.join("r");
    repo_with_commit(&r);
    commit_file(&r, "two.txt", "2\n");
    git(&r, &["checkout", "-q", "--detach", "HEAD~1"]);
    let d = state(&r).status;
    assert!(d.detached);
    assert_eq!(d.branch, None);
}

#[test]
fn declared_submodule_versus_embedded_repository() {
    let (_t, root) = canonical_tmp();
    let lib = root.join("lib");
    repo_with_commit(&lib);
    let main = root.join("main");
    repo_with_commit(&main);
    git(&main, &["-c", "protocol.file.allow=always", "submodule", "add", "-q", lib.to_str().unwrap(), "vendor/lib"]);
    git(&main, &["commit", "-q", "-m", "add submodule"]);

    // A repository dropped inside and added by mistake: a gitlink with no .gitmodules entry.
    let stray = main.join("stray");
    repo_with_commit(&stray);
    git(&main, &["add", "stray"]);

    let s = state(&main);
    assert_eq!(s.submodules, ["vendor/lib"]);
    assert_eq!(s.embedded, ["stray"]);
}

#[test]
fn repo_without_gitmodules_reports_embedded_gitlink() {
    let (_t, root) = canonical_tmp();
    let main = root.join("main");
    repo_with_commit(&main);
    repo_with_commit(&main.join("inner"));
    git(&main, &["add", "inner"]);
    git(&main, &["commit", "-q", "-m", "oops"]);
    let s = state(&main);
    assert_eq!(s.embedded, ["inner"]);
    assert!(s.submodules.is_empty());
}

#[test]
fn worktrees_are_found_and_readable() {
    let (_t, root) = canonical_tmp();
    let main = root.join("proj/main");
    repo_with_commit(&main);
    git(&main, &["worktree", "add", "-q", "-b", "feature", root.join("proj/wt").to_str().unwrap()]);
    let found = discover(&[root.join("proj")], DEFAULT_MAX_DEPTH, &AtomicBool::new(false));
    assert_eq!(found.repos.len(), 2);
    let wt = found.repos.iter().find(|r| r.git_file).expect("worktree has a .git file");
    assert_eq!(state(&wt.path).status.branch.as_deref(), Some("feature"));
}

#[test]
fn reading_state_never_changes_the_repository() {
    let (_t, root) = canonical_tmp();
    let r = root.join("r");
    repo_with_commit(&r);
    write(&r, "dirty.txt", "x\n");
    let before = (git(&r, &["status", "--porcelain=v2", "--branch"]), std::fs::read(r.join(".git/index")).unwrap());
    let _ = state(&r);
    let after = (git(&r, &["status", "--porcelain=v2", "--branch"]), std::fs::read(r.join(".git/index")).unwrap());
    assert_eq!(before, after);
}
