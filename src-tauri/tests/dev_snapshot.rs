mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use chrono::{DateTime, Local, TimeZone};
use common::git::{canonical_tmp, commit_file, git, init, repo_with_commit, write, Isolated};
use organizemymac_lib::dev::git::Git;
use organizemymac_lib::dev::snapshot::{create, plan, Blocker, SnapshotResult, MAX_FILE_BYTES};
use organizemymac_lib::error::AppResult;

fn now() -> DateTime<Local> {
    Local.with_ymd_and_hms(2024, 5, 6, 7, 8, 9).unwrap()
}

fn snap(repo: &Path, tmp: &Path, excludes: &[&str]) -> AppResult<SnapshotResult> {
    let ex: Vec<String> = excludes.iter().map(|s| s.to_string()).collect();
    create(&Git::new(&Isolated), repo, &ex, tmp, &now(), &AtomicBool::new(false))
}

fn blockers(repo: &Path, excludes: &[&str]) -> Vec<Blocker> {
    let ex: Vec<String> = excludes.iter().map(|s| s.to_string()).collect();
    plan(&Git::new(&Isolated), repo, &ex, &now(), &AtomicBool::new(false)).unwrap().blockers
}

/// Everything the snapshot promises not to touch.
#[derive(Debug, PartialEq)]
struct Untouched {
    worktree: BTreeMap<String, (Vec<u8>, std::time::SystemTime)>,
    index: Vec<u8>,
    index_mtime: std::time::SystemTime,
    head: String,
    head_ref: String,
    status: String,
    branches: BTreeSet<String>,
    stash: String,
}

fn capture(repo: &Path) -> Untouched {
    fn walk(dir: &Path, base: &Path, out: &mut BTreeMap<String, (Vec<u8>, std::time::SystemTime)>) {
        for e in fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.file_name().unwrap() == ".git" && dir == base {
                continue;
            }
            let meta = fs::symlink_metadata(&p).unwrap();
            let key = p.strip_prefix(base).unwrap().display().to_string();
            if meta.is_dir() {
                walk(&p, base, out);
            } else if meta.file_type().is_symlink() {
                out.insert(key, (fs::read_link(&p).unwrap().to_string_lossy().into_owned().into_bytes(), meta.modified().unwrap()));
            } else {
                out.insert(key, (fs::read(&p).unwrap(), meta.modified().unwrap()));
            }
        }
    }
    let mut worktree = BTreeMap::new();
    walk(repo, repo, &mut worktree);
    let idx = repo.join(".git/index");
    Untouched {
        worktree,
        index: fs::read(&idx).unwrap(),
        index_mtime: fs::metadata(&idx).unwrap().modified().unwrap(),
        head: git(repo, &["rev-parse", "HEAD"]),
        head_ref: git(repo, &["symbolic-ref", "HEAD"]),
        status: git(repo, &["--no-optional-locks", "status", "--porcelain=v2", "--branch", "--ignored"]),
        branches: git(repo, &["for-each-ref", "--format=%(refname)", "refs/heads"]).lines().map(String::from).collect(),
        stash: git(repo, &["stash", "list"]),
    }
}

/// A repository with every kind of local state a snapshot has to cope with.
fn messy_repo(root: &Path) -> PathBuf {
    let r = root.join("messy");
    repo_with_commit(&r);
    commit_file(&r, ".gitignore", "ignored/\n*.log\n");
    commit_file(&r, "src/a.txt", "original\n");
    commit_file(&r, "src/gone.txt", "will be deleted\n");
    write(&r, "src/a.txt", "modified, not staged\n");
    write(&r, "staged.txt", "first version\n");
    git(&r, &["add", "staged.txt"]);
    write(&r, "staged.txt", "second version, after staging\n");
    fs::remove_file(r.join("src/gone.txt")).unwrap();
    write(&r, "new/notes.txt", "never added\n");
    write(&r, "ignored/x.txt", "ignored\n");
    write(&r, "debug.log", "ignored\n");
    let outside = root.join("outside-secret.txt");
    fs::write(&outside, "content that must not be copied\n").unwrap();
    symlink(&outside, r.join("link")).unwrap();
    r
}

fn tree_files(repo: &Path, r#ref: &str) -> BTreeSet<String> {
    git(repo, &["ls-tree", "-r", "--name-only", r#ref]).lines().map(String::from).collect()
}

#[test]
fn leaves_everything_untouched_and_captures_the_working_tree() {
    let (_t, root) = canonical_tmp();
    let r = messy_repo(&root);
    let tmp = root.join("app-data/tmp");
    let before = capture(&r);
    let head_before = before.head.trim().to_string();

    let result = snap(&r, &tmp, &[]).unwrap();

    let mut after = capture(&r);
    // The only visible difference is the new branch.
    let new_ref = format!("refs/heads/{}", result.ref_name);
    assert!(after.branches.remove(&new_ref), "new ref missing: {:?}", after.branches);
    assert_eq!(before, after);

    assert_eq!(result.ref_name, "wip/snapshot-2024-05-06");
    assert_eq!(result.parent.as_deref(), Some(head_before.as_str()));
    assert_eq!(git(&r, &["rev-parse", &new_ref]).trim(), result.commit);
    assert_eq!(git(&r, &["rev-parse", &format!("{}^", result.commit)]).trim(), head_before);

    let files = tree_files(&r, &result.commit);
    let expected: BTreeSet<String> =
        ["README.md", ".gitignore", "src/a.txt", "staged.txt", "new/notes.txt", "link"].iter().map(|s| s.to_string()).collect();
    assert_eq!(files, expected);
    assert_eq!(git(&r, &["show", &format!("{}:src/a.txt", result.commit)]), "modified, not staged\n");
    assert_eq!(git(&r, &["show", &format!("{}:staged.txt", result.commit)]), "second version, after staging\n");
    // A symlink is stored as a link, never as the content it points to.
    assert!(git(&r, &["ls-tree", &result.commit, "link"]).starts_with("120000 "));
    assert!(git(&r, &["cat-file", "-p", &format!("{}:link", result.commit)]).ends_with("outside-secret.txt"));

    git(&r, &["fsck", "--strict", "--no-dangling"]);
    assert!(git(&r, &["log", "-1", "--format=%B", &result.commit]).contains("The working tree, the index and the branch were not changed."));
    // The private index and its lock are gone.
    assert_eq!(fs::read_dir(&tmp).unwrap().count(), 0);
}

#[test]
fn names_do_not_collide_and_run_out_cleanly() {
    let (_t, root) = canonical_tmp();
    let r = messy_repo(&root);
    let tmp = root.join("tmp");
    let names: Vec<String> = (0..3).map(|_| snap(&r, &tmp, &[]).unwrap().ref_name).collect();
    assert_eq!(names, ["wip/snapshot-2024-05-06", "wip/snapshot-2024-05-06-0708", "wip/snapshot-2024-05-06-070809"]);
    assert!(blockers(&r, &[]).contains(&Blocker::NoFreeRefName));
    assert!(snap(&r, &tmp, &[]).is_err());
}

#[test]
fn works_in_a_repository_without_commits() {
    let (_t, root) = canonical_tmp();
    let r = root.join("empty");
    init(&r);
    write(&r, "first.txt", "hello\n");
    let result = snap(&r, &root.join("tmp"), &[]).unwrap();
    assert_eq!(result.parent, None);
    assert_eq!(tree_files(&r, &result.commit), BTreeSet::from(["first.txt".to_string()]));
    // Still no commit on the current branch.
    assert!(git(&r, &["for-each-ref", "refs/heads/main"]).is_empty());
}

#[test]
fn untracked_secrets_block_until_excluded_and_excluded_files_stay_out() {
    let (_t, root) = canonical_tmp();
    let r = root.join("r");
    repo_with_commit(&r);
    commit_file(&r, "config.txt", "tracked, v1\n");
    write(&r, "config.txt", "tracked, v2\n");
    write(&r, ".env", "SECRET=1\n");
    assert_eq!(blockers(&r, &[]), [Blocker::Secrets { count: 1 }]);
    let err = snap(&r, &root.join("tmp"), &[]).unwrap_err().to_string();
    assert!(err.contains("secrets"), "{err}");
    assert!(git(&r, &["for-each-ref", "refs/heads/wip"]).is_empty());

    let result = snap(&r, &root.join("tmp"), &[".env"]).unwrap();
    let files = tree_files(&r, &result.commit);
    assert!(!files.contains(".env"));
    assert!(files.contains("config.txt"));
}

#[test]
fn excluding_a_tracked_file_keeps_the_last_committed_version() {
    let (_t, root) = canonical_tmp();
    let r = root.join("r");
    repo_with_commit(&r);
    commit_file(&r, "keep.txt", "committed\n");
    commit_file(&r, "private.txt", "committed private\n");
    write(&r, "keep.txt", "changed\n");
    write(&r, "private.txt", "changed private\n");
    let result = snap(&r, &root.join("tmp"), &["private.txt"]).unwrap();
    assert_eq!(git(&r, &["show", &format!("{}:keep.txt", result.commit)]), "changed\n");
    assert_eq!(git(&r, &["show", &format!("{}:private.txt", result.commit)]), "committed private\n");
}

#[test]
fn secrets_already_in_the_repository_warn_but_do_not_block() {
    let (_t, root) = canonical_tmp();
    let r = root.join("r");
    repo_with_commit(&r);
    commit_file(&r, ".env", "ALREADY=committed\n");
    write(&r, "work.txt", "wip\n");
    let p = plan(&Git::new(&Isolated), &r, &[], &now(), &AtomicBool::new(false)).unwrap();
    assert!(p.blockers.is_empty(), "{:?}", p.blockers);
    assert_eq!(p.tracked_secrets, 1);
    assert!(p.secrets.is_empty());
}

#[test]
fn large_new_files_must_be_excluded() {
    let (_t, root) = canonical_tmp();
    let r = root.join("r");
    repo_with_commit(&r);
    let big = fs::File::create(r.join("capture.bin")).unwrap();
    big.set_len(MAX_FILE_BYTES + 1).unwrap(); // sparse: costs no disk
    write(&r, "small.txt", "x\n");
    assert!(matches!(blockers(&r, &[]).as_slice(), [Blocker::FileTooBig { path, .. }] if path == "capture.bin"));
    let result = snap(&r, &root.join("tmp"), &["capture.bin"]).unwrap();
    assert_eq!(tree_files(&r, &result.commit), BTreeSet::from(["README.md".to_string(), "small.txt".to_string()]));
}

#[test]
fn clean_filters_are_refused_only_when_configured_and_used() {
    let (_t, root) = canonical_tmp();
    // Attribute without a configured filter: nothing can run.
    let a = root.join("a");
    repo_with_commit(&a);
    commit_file(&a, ".gitattributes", "*.bin filter=lfs\n");
    write(&a, "x.txt", "x\n");
    assert!(blockers(&a, &[]).is_empty());
    // Filter configured but no attribute uses it.
    let b = root.join("b");
    repo_with_commit(&b);
    git(&b, &["config", "filter.lfs.clean", "cat"]);
    write(&b, "x.txt", "x\n");
    assert!(blockers(&b, &[]).is_empty());
    // Both: refuse, and nothing is written.
    let c = root.join("c");
    repo_with_commit(&c);
    commit_file(&c, ".gitattributes", "*.bin filter=lfs\n");
    git(&c, &["config", "filter.lfs.clean", "touch /tmp/should-never-exist-omm; cat"]);
    write(&c, "x.bin", "x\n");
    assert_eq!(blockers(&c, &[]), [Blocker::FiltersConfigured]);
    assert!(snap(&c, &root.join("tmp"), &[]).is_err());
    assert!(!Path::new("/tmp/should-never-exist-omm").exists());
}

#[test]
fn hooks_and_signing_never_run() {
    let (_t, root) = canonical_tmp();
    let r = root.join("r");
    repo_with_commit(&r);
    let marker = root.join("hook-ran");
    for hook in ["pre-commit", "post-commit", "commit-msg", "reference-transaction", "post-index-change"] {
        let path = r.join(".git/hooks").join(hook);
        fs::write(&path, format!("#!/bin/sh\ntouch '{}'\n", marker.display())).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    }
    git(&r, &["config", "commit.gpgsign", "true"]);
    git(&r, &["config", "gpg.program", "/nonexistent/gpg"]);
    write(&r, "w.txt", "wip\n");
    let result = snap(&r, &root.join("tmp"), &[]).unwrap();
    assert!(!marker.exists(), "a hook ran");
    assert!(git(&r, &["cat-file", "-p", &result.commit]).lines().all(|l| !l.starts_with("gpgsig")));
}

#[test]
fn uses_the_configured_identity() {
    let (_t, root) = canonical_tmp();
    let r = root.join("r");
    repo_with_commit(&r);
    git(&r, &["config", "user.name", "Ada Dev"]);
    git(&r, &["config", "user.email", "ada@example.invalid"]);
    write(&r, "w.txt", "wip\n");
    let result = snap(&r, &root.join("tmp"), &[]).unwrap();
    assert_eq!(git(&r, &["log", "-1", "--format=%an <%ae>", &result.commit]).trim(), "Ada Dev <ada@example.invalid>");
}

#[test]
fn nothing_to_save_is_not_an_empty_commit() {
    let (_t, root) = canonical_tmp();
    let r = root.join("r");
    repo_with_commit(&r);
    assert_eq!(blockers(&r, &[]), [Blocker::NothingToSave]);
    assert!(snap(&r, &root.join("tmp"), &[]).is_err());
    // Only an excluded change: the resulting tree equals HEAD, so no commit is made.
    commit_file(&r, "a.txt", "1\n");
    write(&r, "a.txt", "2\n");
    assert!(snap(&r, &root.join("tmp"), &["a.txt"]).is_err());
    assert!(git(&r, &["for-each-ref", "refs/heads/wip"]).is_empty());
}

#[test]
fn works_from_a_linked_worktree_without_moving_it() {
    let (_t, root) = canonical_tmp();
    let main = root.join("main");
    repo_with_commit(&main);
    let wt = root.join("wt");
    git(&main, &["worktree", "add", "-q", "-b", "feature", wt.to_str().unwrap()]);
    write(&wt, "only-here.txt", "x\n");
    let before = (git(&wt, &["rev-parse", "HEAD"]), git(&wt, &["symbolic-ref", "HEAD"]), git(&wt, &["status", "--porcelain=v2", "--branch"]));
    let result = snap(&wt, &root.join("tmp"), &[]).unwrap();
    let after = (git(&wt, &["rev-parse", "HEAD"]), git(&wt, &["symbolic-ref", "HEAD"]), git(&wt, &["status", "--porcelain=v2", "--branch"]));
    assert_eq!(before, after);
    assert!(tree_files(&wt, &result.commit).contains("only-here.txt"));
    assert_eq!(before.1.trim(), "refs/heads/feature");
}

#[test]
fn the_private_index_may_not_live_inside_the_repository() {
    let (_t, root) = canonical_tmp();
    let r = root.join("r");
    repo_with_commit(&r);
    write(&r, "w.txt", "wip\n");
    assert!(snap(&r, &r.join("tmp"), &[]).is_err());
    assert!(!r.join("tmp").exists());
}

#[test]
fn a_directory_that_is_not_a_repository_fails_without_side_effects() {
    let (_t, root) = canonical_tmp();
    let plain = root.join("plain");
    write(&plain, "a.txt", "x\n");
    assert!(snap(&plain, &root.join("tmp"), &[]).is_err());
    assert!(!plain.join(".git").exists());
}
