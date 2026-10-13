mod common;

use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;
use std::sync::atomic::AtomicBool;

use common::git::{canonical_tmp, commit_file, git, repo_with_commit, write};
use organizemymac_lib::dev::artifacts::{builtin_rules, scan_repo, Artifact, NOTE_NOT_IGNORED, NOTE_NO_LOCKFILE, NOTE_TRACKED};
use organizemymac_lib::dev::git::Git;
use organizemymac_lib::exec::RealRunner;
use organizemymac_lib::types::Confidence;

fn scan(repo: &Path) -> Vec<Artifact> {
    scan_repo(&Git::new(&RealRunner), repo, &builtin_rules(), &AtomicBool::new(false))
}

fn rel(a: &Artifact, repo: &Path) -> String {
    a.path.strip_prefix(repo).unwrap().display().to_string()
}

fn node_project(repo: &Path) {
    repo_with_commit(repo);
    commit_file(repo, ".gitignore", "node_modules\n");
    commit_file(repo, "package.json", "{}\n");
    commit_file(repo, "package-lock.json", "{}\n");
    write(repo, "node_modules/dep/index.js", &"x".repeat(1000));
    write(repo, "node_modules/dep/package.json", "{}");
}

#[test]
fn node_modules_with_lockfile_is_safe_and_explained() {
    let (_t, root) = canonical_tmp();
    let repo = root.join("app");
    node_project(&repo);
    let found = scan(&repo);
    assert_eq!(found.len(), 1);
    let a = &found[0];
    assert_eq!(rel(a, &repo), "node_modules");
    assert_eq!(a.kind, "node-modules");
    assert_eq!(a.confidence, Confidence::Safe);
    assert!(a.notes.is_empty());
    assert_eq!(a.size, 1002);
    assert!(a.reason.contains("install"));
    assert!(a.last_used_ms.is_some_and(|ms| ms > 1_700_000_000_000));
}

#[test]
fn missing_lockfile_demotes_to_review() {
    let (_t, root) = canonical_tmp();
    let repo = root.join("app");
    repo_with_commit(&repo);
    commit_file(&repo, "package.json", "{}\n");
    write(&repo, "node_modules/dep/index.js", "x");
    let found = scan(&repo);
    assert_eq!(found[0].confidence, Confidence::Review);
    assert_eq!(found[0].notes, [NOTE_NO_LOCKFILE]);
}

#[test]
fn needs_a_marker_and_never_follows_symlinks() {
    let (_t, root) = canonical_tmp();
    let repo = root.join("app");
    repo_with_commit(&repo);
    write(&repo, "node_modules/dep/index.js", "x"); // no package.json beside it
    write(&repo, "sub/package.json", "{}");
    let outside = root.join("outside-modules");
    write(&outside, "big.bin", "x");
    symlink(&outside, repo.join("sub/node_modules")).unwrap();
    assert!(scan(&repo).is_empty());
    assert!(outside.join("big.bin").exists());
}

#[test]
fn ambiguous_folders_need_to_be_ignored_by_git() {
    let (_t, root) = canonical_tmp();
    let repo = root.join("lib");
    repo_with_commit(&repo);
    commit_file(&repo, "package.json", "{}\n");
    commit_file(&repo, ".gitignore", "build\n");
    commit_file(&repo, "dist/index.js", "committed\n"); // versioned output
    write(&repo, "build/out.js", "ignored\n"); // ignored by .gitignore
    let found = scan(&repo);
    let by = |name: &str| found.iter().find(|a| rel(a, &repo) == name).unwrap();
    assert_eq!((by("build").confidence, by("build").notes.clone()), (Confidence::Safe, vec![]));
    assert!(by("dist").notes.contains(&NOTE_TRACKED.to_string()));
    assert_eq!(by("dist").confidence, Confidence::Review);

    git(&repo, &["rm", "-rq", "dist"]);
    git(&repo, &["commit", "-qm", "drop dist"]);
    write(&repo, "dist/index.js", "untracked and not ignored\n");
    let again = scan(&repo);
    let dist = again.iter().find(|a| rel(a, &repo) == "dist").unwrap();
    assert_eq!(dist.notes, [NOTE_NOT_IGNORED]);
}

#[test]
fn rust_and_maven_targets_are_told_apart() {
    let (_t, root) = canonical_tmp();
    let repo = root.join("mixed");
    repo_with_commit(&repo);
    commit_file(&repo, "rust/Cargo.toml", "[package]\n");
    commit_file(&repo, "rust/Cargo.lock", "\n");
    write(&repo, "rust/target/debug/x", "x");
    commit_file(&repo, "java/pom.xml", "<project/>\n");
    write(&repo, "java/target/classes/A.class", "x");
    write(&repo, "other/target/y", "y"); // no marker: not a build folder
    let found = scan(&repo);
    let kinds: Vec<_> = found.iter().map(|a| (rel(a, &repo), a.kind.as_str(), a.confidence)).collect();
    assert!(kinds.contains(&("rust/target".into(), "rust-target", Confidence::Safe)), "{kinds:?}");
    assert!(kinds.contains(&("java/target".into(), "maven-target", Confidence::Safe)), "{kinds:?}");
    assert_eq!(found.len(), 2);
}

#[test]
fn monorepo_finds_package_level_folders_but_not_nested_ones_or_nested_repos() {
    let (_t, root) = canonical_tmp();
    let repo = root.join("mono");
    repo_with_commit(&repo);
    commit_file(&repo, ".gitignore", "node_modules\n");
    for pkg in ["a", "b"] {
        commit_file(&repo, &format!("packages/{pkg}/package.json"), "{}\n");
        commit_file(&repo, &format!("packages/{pkg}/package-lock.json"), "{}\n");
        write(&repo, &format!("packages/{pkg}/node_modules/dep/index.js"), "x");
    }
    // Inside node_modules, a package with its own node_modules must not be listed again.
    write(&repo, "packages/a/node_modules/dep/package.json", "{}");
    write(&repo, "packages/a/node_modules/dep/node_modules/deeper/i.js", "x");
    // A nested repository has its own pass.
    node_project(&repo.join("inner"));
    let found = scan(&repo);
    let paths: Vec<_> = found.iter().map(|a| rel(a, &repo)).collect();
    assert_eq!(paths.len(), 2, "{paths:?}");
    assert!(paths.contains(&"packages/a/node_modules".to_string()));
    assert!(paths.contains(&"packages/b/node_modules".to_string()));
}

#[test]
fn workspace_packages_inherit_the_root_lockfile() {
    let (_t, root) = canonical_tmp();
    let repo = root.join("ws");
    repo_with_commit(&repo);
    commit_file(&repo, ".gitignore", "node_modules\n");
    commit_file(&repo, "package.json", "{}\n");
    commit_file(&repo, "pnpm-lock.yaml", "\n");
    commit_file(&repo, "packages/a/package.json", "{}\n");
    write(&repo, "packages/a/node_modules/x/i.js", "x");
    // A lockfile above the repository must not count.
    write(&root, "package-lock.json", "{}");
    let other = root.join("solo");
    repo_with_commit(&other);
    commit_file(&other, "package.json", "{}\n");
    write(&other, "node_modules/x/i.js", "x");
    let found = scan(&repo);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].confidence, Confidence::Safe, "{:?}", found[0].notes);
    assert_eq!(scan(&other)[0].notes, [NOTE_NO_LOCKFILE]);
}

#[test]
fn project_local_derived_data_links_to_the_cleanup_rule() {
    let (_t, root) = canonical_tmp();
    let repo = root.join("ios");
    repo_with_commit(&repo);
    fs::create_dir_all(repo.join("App.xcodeproj")).unwrap();
    write(&repo, "DerivedData/Build/x", "x");
    let found = scan(&repo);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].cleanup_rule_id.as_deref(), Some("xcode-derived-data"));
}

#[test]
fn scanning_changes_nothing() {
    let (_t, root) = canonical_tmp();
    let repo = root.join("app");
    node_project(&repo);
    let snapshot = || {
        let status = git(&repo, &["status", "--porcelain=v2", "--ignored"]);
        let index = fs::read(repo.join(".git/index")).unwrap();
        let listing = git(&repo, &["ls-files", "-z"]);
        (status, index, listing, fs::metadata(repo.join("node_modules")).unwrap().modified().unwrap())
    };
    let before = snapshot();
    let _ = scan(&repo);
    assert_eq!(before, snapshot());
}

#[test]
fn works_on_a_directory_that_is_not_a_repository_by_degrading_to_review() {
    let (_t, root) = canonical_tmp();
    let dir = root.join("plain");
    write(&dir, "package.json", "{}");
    write(&dir, "package-lock.json", "{}");
    write(&dir, "node_modules/x/i.js", "x");
    let found = scan(&dir);
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].confidence, Confidence::Review);
    assert!(found[0].notes.contains(&"gitUnknown".to_string()));
}
