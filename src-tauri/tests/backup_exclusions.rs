mod common;

use std::collections::HashSet;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;

use common::git::{canonical_tmp, commit_file, repo_with_commit, write, Isolated};
use organizemymac_lib::backup::exclusions::{apply, listed_state, parse_isexcluded, preview, state_of, undo, ExclusionState, ListedState};
use organizemymac_lib::dev::artifacts::builtin_rules;
use organizemymac_lib::dev::git::Git;
use organizemymac_lib::dev::removal::ArtifactRemoval;
use organizemymac_lib::error::AppResult;
use organizemymac_lib::exec::{Cmd, CmdOutput, CommandRunner};
use organizemymac_lib::filesystem::safety::SafetyPolicy;

/// A `tmutil` that remembers what was excluded and refuses everything the app must never do.
#[derive(Default)]
struct FakeTm {
    excluded: Mutex<HashSet<String>>,
    calls: Mutex<Vec<String>>,
    refuse_add: bool,
    add_has_no_effect: bool,
}

impl FakeTm {
    fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
    fn is_excluded(&self, path: &str) -> bool {
        let set = self.excluded.lock().unwrap();
        // Like the real thing, a folder's exclusion covers what is inside it.
        set.iter().any(|e| path == e || path.starts_with(&format!("{e}/")))
    }
    fn out(status: i32, stdout: &str, stderr: &str) -> AppResult<CmdOutput> {
        Ok(CmdOutput { status: Some(status), stdout: stdout.as_bytes().to_vec(), stderr: stderr.as_bytes().to_vec(), timed_out: false })
    }
}

impl CommandRunner for FakeTm {
    fn run(&self, cmd: &Cmd) -> AppResult<CmdOutput> {
        assert_eq!(cmd.program, "/usr/bin/tmutil");
        self.calls.lock().unwrap().push(cmd.args.join(" "));
        let verb = cmd.args[0].as_str();
        assert!(!cmd.args.iter().any(|a| a == "-p"), "the fixed-path form needs root and must never be used");
        assert_eq!(cmd.args.len(), 2, "exactly one path per call: {:?}", cmd.args);
        let path = cmd.args[1].clone();
        assert!(Path::new(&path).is_absolute(), "tmutil must get absolute paths: {path}");
        match verb {
            "isexcluded" => {
                let tag = if self.is_excluded(&path) { "Excluded" } else { "Included" };
                Self::out(0, &format!("[{tag}]  {path}\n"), "") // always exit 0, like the real one
            }
            "addexclusion" => {
                if self.refuse_add {
                    return Self::out(1, "", "tmutil: Operation not permitted");
                }
                if std::fs::symlink_metadata(&path).is_err() {
                    return Self::out(22, "", "Error (100002) while attempting to change exclusion setting.");
                }
                if !self.add_has_no_effect {
                    self.excluded.lock().unwrap().insert(path);
                }
                Self::out(0, "", "")
            }
            "removeexclusion" => {
                self.excluded.lock().unwrap().remove(&path);
                Self::out(0, "", "")
            }
            other => panic!("unexpected tmutil verb {other}"),
        }
    }
}

struct World {
    _t: tempfile::TempDir,
    root: PathBuf,
    repo: PathBuf,
    policy: SafetyPolicy,
}

fn item(repo: &Path, rel: &str, kind: &str) -> ArtifactRemoval {
    ArtifactRemoval { path: repo.join(rel).display().to_string(), repo: repo.display().to_string(), kind: kind.into(), size: 1 }
}

/// A repository with ignored node_modules and a Rust target, a versioned dist, and one more folder.
fn world() -> World {
    let (t, root) = canonical_tmp();
    let repo = root.join("projects/app");
    repo_with_commit(&repo);
    commit_file(&repo, ".gitignore", "node_modules\ntarget\n");
    commit_file(&repo, "package.json", "{}\n");
    commit_file(&repo, "package-lock.json", "{}\n");
    commit_file(&repo, "Cargo.toml", "[package]\n");
    commit_file(&repo, "Cargo.lock", "\n");
    commit_file(&repo, "dist/index.js", "versioned output\n");
    write(&repo, "node_modules/dep/index.js", &"x".repeat(1000));
    write(&repo, "target/debug/app", &"y".repeat(500));
    let policy = SafetyPolicy::custom(vec![], vec![root.join("projects")], vec![root.join("data")]);
    World { _t: t, root, repo, policy }
}

fn run_preview(w: &World, tm: &FakeTm, items: &[ArtifactRemoval]) -> Vec<organizemymac_lib::backup::exclusions::PreviewItem> {
    preview(tm, &Git::new(&Isolated), &w.policy, &w.repo, items, &builtin_rules(), &AtomicBool::new(false))
}

fn run_apply(w: &World, tm: &FakeTm, items: &[ArtifactRemoval]) -> Vec<organizemymac_lib::backup::exclusions::Applied> {
    apply(tm, &Git::new(&Isolated), &w.policy, &w.repo, items, &builtin_rules(), &AtomicBool::new(false))
}

#[test]
fn parses_what_tmutil_prints() {
    assert_eq!(parse_isexcluded("[Excluded]  /Users/x/node_modules\n"), ExclusionState::Excluded);
    assert_eq!(parse_isexcluded("[Included]  /Users/x/plain\n"), ExclusionState::Included);
    assert_eq!(parse_isexcluded(""), ExclusionState::Unknown);
    assert_eq!(parse_isexcluded("tmutil: something else"), ExclusionState::Unknown);
}

#[test]
fn the_preview_shows_count_size_and_why_something_cannot_be_excluded() {
    let w = world();
    let tm = FakeTm::default();
    let items = vec![
        item(&w.repo, "node_modules", "node-modules"),
        item(&w.repo, "target", "rust-target"),
        item(&w.repo, "dist", "js-dist"),           // versioned files: never
        item(&w.repo, "src", "node-modules"),       // not a regenerable folder
    ];
    let p = run_preview(&w, &tm, &items);
    assert_eq!(p.len(), 4);
    let by = |suffix: &str| p.iter().find(|i| i.path.ends_with(suffix)).unwrap();
    assert!(by("node_modules").ok && by("target").ok);
    assert_eq!((by("node_modules").size, by("target").size), (1000, 500));
    assert_eq!(by("node_modules").state, ExclusionState::Included);
    assert!(!by("dist").ok && by("dist").reason.as_deref().unwrap().contains("versionados"));
    assert!(!by("/src").ok);
    // Order follows the request, and nothing was changed.
    assert_eq!(p.iter().map(|i| i.path.rsplit('/').next().unwrap()).collect::<Vec<_>>(), ["node_modules", "target", "dist", "src"]);
    assert!(tm.calls().iter().all(|c| c.starts_with("isexcluded")), "{:?}", tm.calls());
    let summary = organizemymac_lib::backup::exclusions::Preview::new(p);
    assert_eq!((summary.count, summary.bytes), (2, 1500));
}

#[test]
fn an_already_excluded_folder_is_not_offered_again() {
    let w = world();
    let tm = FakeTm::default();
    tm.excluded.lock().unwrap().insert(w.repo.join("node_modules").display().to_string());
    let p = run_preview(&w, &tm, &[item(&w.repo, "node_modules", "node-modules")]);
    assert!(!p[0].ok);
    assert_eq!(p[0].state, ExclusionState::Excluded);
    // So applying does nothing to it, and it would never become "ours" to undo.
    let a = run_apply(&w, &tm, &[item(&w.repo, "node_modules", "node-modules")]);
    assert!(!a[0].ok);
    assert!(!tm.calls().iter().any(|c| c.starts_with("addexclusion")));
}

#[test]
fn apply_excludes_only_what_was_approved_and_verifies_each_one() {
    let w = world();
    let tm = FakeTm::default();
    let out = run_apply(&w, &tm, &[item(&w.repo, "node_modules", "node-modules")]);
    assert!(out.len() == 1 && out[0].ok, "{out:?}");
    assert!(tm.is_excluded(&w.repo.join("node_modules").display().to_string()));
    assert!(!tm.is_excluded(&w.repo.join("target").display().to_string()), "target was not approved");
    // add, then verify with the system, one path per call.
    let calls = tm.calls();
    let add = calls.iter().position(|c| c.starts_with("addexclusion")).unwrap();
    assert!(calls[add + 1].starts_with("isexcluded"), "{calls:?}");
    assert_eq!(calls.iter().filter(|c| c.starts_with("addexclusion")).count(), 1);
    // Nothing on disk changed.
    assert!(w.repo.join("node_modules/dep/index.js").exists());
}

#[test]
fn forged_unsafe_or_ignored_folders_never_reach_tmutil() {
    let mut w = world();
    let outside = w.root.join("elsewhere");
    write(&outside, "package.json", "{}");
    write(&outside, "node_modules/x", "x");
    symlink(&outside.join("node_modules"), w.repo.join("sub-modules-link")).unwrap();
    let tm = FakeTm::default();
    let forged = vec![
        item(&w.repo, "does-not-exist", "node-modules"),
        item(&w.repo, "dist", "js-dist"),
        item(&w.repo, "sub-modules-link", "node-modules"),
        ArtifactRemoval { path: outside.join("node_modules").display().to_string(), repo: w.repo.display().to_string(), kind: "node-modules".into(), size: 1 },
        ArtifactRemoval { path: "/etc".into(), repo: w.repo.display().to_string(), kind: "node-modules".into(), size: 1 },
    ];
    let out = run_apply(&w, &tm, &forged);
    assert!(out.iter().all(|o| !o.ok), "{out:?}");
    assert!(!tm.calls().iter().any(|c| c.starts_with("addexclusion")), "{:?}", tm.calls());

    // The user's ignore list is honoured: that folder is left alone.
    w.policy.set_ignored(vec![w.repo.join("node_modules")]);
    let out = run_apply(&w, &tm, &[item(&w.repo, "node_modules", "node-modules")]);
    assert!(!out[0].ok && out[0].error.as_deref().unwrap().contains("ignore list"), "{out:?}");
    assert!(!tm.calls().iter().any(|c| c.starts_with("addexclusion")));
}

#[test]
fn a_refusal_or_a_command_that_did_nothing_is_reported_not_assumed() {
    let w = world();
    let refuse = FakeTm { refuse_add: true, ..Default::default() };
    let out = run_apply(&w, &refuse, &[item(&w.repo, "node_modules", "node-modules")]);
    assert!(!out[0].ok && out[0].error.as_deref().unwrap().contains("recusou"), "{out:?}");

    // Exit code 0 but the system still says "included": that is a failure.
    let silent = FakeTm { add_has_no_effect: true, ..Default::default() };
    let out = run_apply(&w, &silent, &[item(&w.repo, "node_modules", "node-modules")]);
    assert!(!out[0].ok && out[0].error.as_deref().unwrap().contains("não ficou fora"), "{out:?}");
}

#[test]
fn undo_removes_the_mark_verifies_it_and_handles_missing_folders() {
    let w = world();
    let tm = FakeTm::default();
    let path = w.repo.join("node_modules").display().to_string();
    run_apply(&w, &tm, &[item(&w.repo, "node_modules", "node-modules")]);
    assert!(tm.is_excluded(&path));

    let out = undo(&tm, &w.policy, &[path.clone()]);
    assert!(out[0].ok && !out[0].gone && !out[0].still_excluded, "{out:?}");
    assert!(!tm.is_excluded(&path));
    // Undoing twice is harmless.
    assert!(undo(&tm, &w.policy, &[path.clone()])[0].ok);

    // A folder that no longer exists has nothing to undo, and tmutil is not asked.
    let gone = w.repo.join("deleted-long-ago").display().to_string();
    let before = tm.calls().len();
    let out = undo(&tm, &w.policy, &[gone]);
    assert!(out[0].ok && out[0].gone);
    assert_eq!(tm.calls().len(), before);
}

#[test]
fn undo_says_when_something_else_still_keeps_the_folder_out() {
    let w = world();
    let tm = FakeTm::default();
    // The parent folder is excluded by someone else: removing our mark does not put the folder back.
    tm.excluded.lock().unwrap().insert(w.repo.display().to_string());
    tm.excluded.lock().unwrap().insert(w.repo.join("node_modules").display().to_string());
    let out = undo(&tm, &w.policy, &[w.repo.join("node_modules").display().to_string()]);
    assert!(out[0].ok && out[0].still_excluded, "{out:?}");
    assert!(tm.is_excluded(&w.repo.display().to_string()), "the other exclusion is untouched");
}

#[test]
fn the_list_tells_active_lost_and_missing_apart() {
    let w = world();
    let tm = FakeTm::default();
    let path = w.repo.join("node_modules");
    assert_eq!(listed_state(&tm, &path), ListedState::Lost, "exists, but in the backup");
    tm.excluded.lock().unwrap().insert(path.display().to_string());
    assert_eq!(listed_state(&tm, &path), ListedState::Active);
    assert_eq!(listed_state(&tm, &w.repo.join("gone")), ListedState::Missing);
    // A deleted and recreated folder loses the mark: the old path no longer counts as excluded.
    tm.excluded.lock().unwrap().clear();
    assert_eq!(state_of(&tm, &path), ExclusionState::Included);
}
