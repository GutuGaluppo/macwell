mod common;

use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Mutex;

use common::git::{canonical_tmp, commit_file, git, repo_with_commit, write, Isolated};
use organizemymac_lib::cleanup::RemovalRequest;
use organizemymac_lib::commands::dev::remove_artifacts_with;
use organizemymac_lib::db::Db;
use organizemymac_lib::dev::artifacts::builtin_rules;
use organizemymac_lib::dev::git::Git;
use organizemymac_lib::dev::removal::{revalidate, ArtifactRemoval};
use organizemymac_lib::exec::FakeRunner;
use organizemymac_lib::filesystem::safety::{RemovalContext, SafetyPolicy};
use organizemymac_lib::state::{AppPaths, AppState};
use organizemymac_lib::types::OperationOutcome;

struct World {
    _t: tempfile::TempDir,
    root: PathBuf,
    trash: PathBuf,
    state: AppState,
    /// Every request the mover received: (scan root, paths).
    seen: Mutex<Vec<(Option<String>, Vec<String>)>>,
}

fn world() -> World {
    let (t, root) = canonical_tmp();
    let projects = root.join("projects");
    fs::create_dir_all(&projects).unwrap();
    let db = Db::open_in_memory().unwrap();
    db.add_dev_root(projects.to_str().unwrap()).unwrap();
    let paths = AppPaths { home: root.clone(), data_dir: root.join("data"), log_dir: root.join("logs") };
    // Nothing is protected except the app's own data: the rules under test are the ones in this feature.
    let policy = SafetyPolicy::custom(vec![], vec![projects.clone()], vec![root.join("data")]);
    let trash = root.join("fake-trash");
    fs::create_dir_all(&trash).unwrap();
    World { _t: t, root, trash, state: AppState::new(paths, db, policy).unwrap(), seen: Mutex::new(Vec::new()) }
}

impl World {
    fn projects(&self) -> PathBuf {
        self.root.join("projects")
    }

    /// Stands in for `move_to_trash`: same policy check with the same scan root, then a rename.
    fn remove(&self, items: Vec<ArtifactRemoval>) -> Vec<OperationOutcome> {
        let trash = self.trash.clone();
        let mover = |policy: &SafetyPolicy, _db: &Db, req: &RemovalRequest| -> Vec<OperationOutcome> {
            self.seen.lock().unwrap().push((req.scan_root.clone(), req.items.iter().map(|i| i.path.clone()).collect()));
            let ctx = RemovalContext { scan_root: req.scan_root.clone().map(PathBuf::from), allow_system_library: false };
            req.items
                .iter()
                .map(|i| match policy.check(Path::new(&i.path), &ctx) {
                    Ok(resolved) => {
                        let dest = trash.join(format!("{}-{}", i.size, resolved.file_name().unwrap().to_string_lossy()));
                        fs::rename(&resolved, dest).unwrap();
                        OperationOutcome { path: i.path.clone(), size: i.size, ok: true, error: None }
                    }
                    Err(e) => OperationOutcome { path: i.path.clone(), size: i.size, ok: false, error: Some(e.to_string()) },
                })
                .collect()
        };
        remove_artifacts_with(&self.state, items, &mover).unwrap()
    }
}

fn item(repo: &Path, rel: &str, kind: &str) -> ArtifactRemoval {
    ArtifactRemoval { path: repo.join(rel).display().to_string(), repo: repo.display().to_string(), kind: kind.into(), size: 1000 }
}

fn node_repo(dir: &Path) {
    repo_with_commit(dir);
    commit_file(dir, ".gitignore", "node_modules\n");
    commit_file(dir, "package.json", "{}\n");
    commit_file(dir, "package-lock.json", "{}\n");
    write(dir, "node_modules/dep/index.js", "x");
}

fn reasons(out: &[OperationOutcome]) -> Vec<String> {
    out.iter().map(|o| format!("{} {}", o.ok, o.error.clone().unwrap_or_default())).collect()
}

#[test]
fn removes_a_regenerable_folder_through_the_policy_with_the_repo_as_scan_root() {
    let w = world();
    let repo = w.projects().join("app");
    node_repo(&repo);
    let out = w.remove(vec![item(&repo, "node_modules", "node-modules")]);
    assert!(out.len() == 1 && out[0].ok, "{out:?}");
    assert!(!repo.join("node_modules").exists());
    assert!(w.trash.join("1000-node_modules/dep/index.js").exists(), "moved, not deleted");
    // Everything else is untouched.
    assert!(repo.join("package.json").exists() && repo.join(".git").exists());
    let seen = w.seen.lock().unwrap();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].0.as_deref(), Some(repo.to_str().unwrap()));
}

#[test]
fn forged_or_stale_items_are_rejected_and_nothing_moves() {
    let w = world();
    let repo = w.projects().join("app");
    node_repo(&repo);
    write(&repo, "src/main.rs", "fn main() {}");
    let other = w.projects().join("other");
    node_repo(&other);
    let outside = w.root.join("elsewhere");
    write(&outside, "node_modules/x", "x");
    write(&outside, "package.json", "{}");

    let forged = vec![
        item(&repo, "src", "node-modules"),                 // a real folder, but not an artifact
        item(&repo, "src/main.rs", "node-modules"),         // a file
        item(&repo, "node_modules/dep", "node-modules"),    // inside an artifact
        item(&repo, "node_modules", "rust-target"),         // wrong kind
        item(&repo, "does-not-exist", "node-modules"),
        ArtifactRemoval { path: other.join("node_modules").display().to_string(), ..item(&repo, "x", "node-modules") }, // another repo's folder under this repo's name
        ArtifactRemoval { path: outside.join("node_modules").display().to_string(), repo: outside.display().to_string(), kind: "node-modules".into(), size: 1 }, // repo outside every root
        ArtifactRemoval { path: "/etc".into(), repo: repo.display().to_string(), kind: "node-modules".into(), size: 1 },
        ArtifactRemoval { path: "relative/node_modules".into(), repo: "relative".into(), kind: "node-modules".into(), size: 1 },
    ];
    let n = forged.len();
    let out = w.remove(forged);
    assert_eq!(out.len(), n, "{out:?}");
    assert!(out.iter().all(|o| !o.ok), "{}", reasons(&out).join("\n"));
    assert!(w.seen.lock().unwrap().is_empty(), "nothing may reach the mover");
    assert!(repo.join("node_modules/dep/index.js").exists() && repo.join("src/main.rs").exists());
    assert!(other.join("node_modules").exists() && outside.join("node_modules").exists());
}

#[test]
fn a_symlinked_folder_is_never_removed_and_its_target_is_safe() {
    let w = world();
    let repo = w.projects().join("app");
    repo_with_commit(&repo);
    commit_file(&repo, "package.json", "{}\n");
    let target = w.root.join("shared-modules");
    write(&target, "big.bin", "keep me");
    symlink(&target, repo.join("node_modules")).unwrap();
    let out = w.remove(vec![item(&repo, "node_modules", "node-modules")]);
    assert!(!out[0].ok);
    assert!(target.join("big.bin").exists());
}

#[test]
fn a_folder_swapped_for_a_symlink_after_the_scan_is_rejected() {
    let w = world();
    let repo = w.projects().join("app");
    node_repo(&repo);
    let request = item(&repo, "node_modules", "node-modules"); // as listed by the scan
    let precious = w.root.join("precious");
    write(&precious, "data.txt", "do not touch");
    fs::remove_dir_all(repo.join("node_modules")).unwrap();
    symlink(&precious, repo.join("node_modules")).unwrap();
    let out = w.remove(vec![request]);
    assert!(!out[0].ok, "{out:?}");
    assert!(precious.join("data.txt").exists());
}

#[test]
fn versioned_folders_are_refused_but_a_missing_lockfile_is_the_users_call() {
    let w = world();
    let repo = w.projects().join("lib");
    repo_with_commit(&repo);
    commit_file(&repo, "package.json", "{}\n");
    commit_file(&repo, ".gitignore", "build\n");
    commit_file(&repo, "dist/index.js", "versioned output\n");
    write(&repo, "build/out.js", "ignored\n");
    write(&repo, "node_modules/dep/i.js", "x"); // no lockfile: "review", but explicitly chosen
    let out = w.remove(vec![
        item(&repo, "dist", "js-dist"),
        item(&repo, "build", "build-output"),
        item(&repo, "node_modules", "node-modules"),
    ]);
    let by = |suffix: &str| out.iter().find(|o| o.path.ends_with(suffix)).unwrap();
    assert!(!by("dist").ok && by("dist").error.as_deref().unwrap().contains("versionados"), "{out:?}");
    assert!(by("build").ok && by("node_modules").ok, "{out:?}");
    assert!(repo.join("dist/index.js").exists(), "versioned files stay");
}

#[test]
fn the_users_ignore_list_and_duplicates_are_respected() {
    let w = world();
    let repo = w.projects().join("app");
    node_repo(&repo);
    w.state.db.lock().unwrap().add_ignore(repo.join("node_modules").to_str().unwrap(), Some("keep")).unwrap();
    w.state.reload_ignore_list().unwrap();
    let out = w.remove(vec![item(&repo, "node_modules", "node-modules"), item(&repo, "node_modules", "node-modules")]);
    assert_eq!(out.len(), 1, "the same folder twice is one request: {out:?}");
    assert!(!out[0].ok && out[0].error.as_deref().unwrap().contains("ignore list"), "{out:?}");
    assert!(repo.join("node_modules").exists());
}

#[test]
fn when_git_cannot_answer_nothing_is_removed() {
    let (_t, root) = canonical_tmp();
    node_repo(&root.join("app"));
    let repo = root.join("app");
    let fake = FakeRunner::new().reply_failure(128, "fatal: not a git repository");
    let rv = revalidate(&Git::new(&fake), &repo, &[item(&repo, "node_modules", "node-modules")], &builtin_rules(), &AtomicBool::new(false));
    assert!(rv.accepted.is_empty());
    assert_eq!(rv.rejected.len(), 1);
    assert!(rv.rejected[0].error.as_deref().unwrap().contains("git"));
    // And with a working git the same request is accepted.
    let rv = revalidate(&Git::new(&Isolated), &repo, &[item(&repo, "node_modules", "node-modules")], &builtin_rules(), &AtomicBool::new(false));
    assert_eq!((rv.accepted.len(), rv.rejected.len()), (1, 0));
    // The check itself never changes the repository.
    assert!(git(&repo, &["status", "--porcelain"]).is_empty());
}
