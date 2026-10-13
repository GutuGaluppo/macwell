//! Talks to the real `tmutil`. It marks a throwaway folder in your home and puts everything back.
//! Not run by default: `cargo test --test backup_exclusions_real -- --ignored --nocapture`.
mod common;

use std::sync::atomic::AtomicBool;

use common::git::{commit_file, repo_with_commit, write, Isolated};
use organizemymac_lib::backup::exclusions::{apply, listed_state, preview, state_of, undo, ExclusionState, ListedState};
use organizemymac_lib::dev::artifacts::builtin_rules;
use organizemymac_lib::dev::git::Git;
use organizemymac_lib::dev::removal::ArtifactRemoval;
use organizemymac_lib::exec::RealRunner;
use organizemymac_lib::filesystem::safety::SafetyPolicy;

#[test]
#[ignore = "changes a Time Machine setting on a throwaway folder; run on purpose"]
fn the_real_tmutil_round_trip() {
    // /private/tmp is excluded by macOS itself, so the folder has to live in the home folder.
    let home = dirs::home_dir().unwrap();
    let dir = tempfile::Builder::new().prefix("omm-tm-probe-").tempdir_in(&home).unwrap();
    let repo = std::fs::canonicalize(dir.path()).unwrap();
    repo_with_commit(&repo);
    commit_file(&repo, ".gitignore", "node_modules\n");
    commit_file(&repo, "package.json", "{}\n");
    commit_file(&repo, "package-lock.json", "{}\n");
    write(&repo, "node_modules/dep/index.js", &"x".repeat(2048));

    let policy = SafetyPolicy::custom(vec![], vec![home.clone()], vec![]);
    let git = Git::new(&Isolated);
    let rules = builtin_rules();
    let cancel = AtomicBool::new(false);
    let nm = repo.join("node_modules");
    let items = vec![ArtifactRemoval { path: nm.display().to_string(), repo: repo.display().to_string(), kind: "node-modules".into(), size: 0 }];

    assert_eq!(state_of(&RealRunner, &nm), ExclusionState::Included, "the probe folder must start out in the backup");
    let p = preview(&RealRunner, &git, &policy, &repo, &items, &rules, &cancel);
    assert!(p[0].ok && p[0].size == 2048, "{p:?}");

    let applied = apply(&RealRunner, &git, &policy, &repo, &items, &rules, &cancel);
    assert!(applied[0].ok, "{applied:?}");
    assert_eq!(state_of(&RealRunner, &nm), ExclusionState::Excluded);
    assert_eq!(state_of(&RealRunner, &nm.join("dep/index.js")), ExclusionState::Excluded, "covers what is inside");
    assert_eq!(state_of(&RealRunner, &repo.join("package.json")), ExclusionState::Included, "nothing else is touched");
    assert_eq!(listed_state(&RealRunner, &nm), ListedState::Active);
    // Already excluded: not offered again.
    assert!(!preview(&RealRunner, &git, &policy, &repo, &items, &rules, &cancel)[0].ok);

    let undone = undo(&RealRunner, &policy, &[nm.display().to_string()]);
    assert!(undone[0].ok && !undone[0].still_excluded, "{undone:?}");
    assert_eq!(state_of(&RealRunner, &nm), ExclusionState::Included);
    assert_eq!(listed_state(&RealRunner, &nm), ListedState::Lost, "back in the backup");
    assert_eq!(std::fs::read(nm.join("dep/index.js")).unwrap().len(), 2048, "the folder itself was never touched");
}
