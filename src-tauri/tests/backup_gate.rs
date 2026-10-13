mod common;

use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use common::git::{canonical_tmp, commit_file, git, repo_with_commit, write, Isolated};
use organizemymac_lib::backup::gate::{evaluate, verify, BackupAck, BackupGate, GateInput, GateLevel};
use organizemymac_lib::backup::icloud::ICloudState;
use organizemymac_lib::backup::timemachine::{Status, TimeMachineState};
use organizemymac_lib::backup::BackupState;
use organizemymac_lib::dev::git::Git;
use organizemymac_lib::dev::report::{unsynced_repos, UnsyncedRepo};
use organizemymac_lib::error::AppError;
use organizemymac_lib::exec::FakeRunner;
use organizemymac_lib::filesystem::AccessStatus;

fn backup(status: Status, auto_backup: Option<bool>) -> BackupState {
    BackupState {
        time_machine: TimeMachineState {
            status,
            destinations: vec![],
            auto_backup,
            last_backup_ms: None,
            last_backup_source: None,
            last_attempt_ms: None,
            age_days: None,
            running: false,
            percent: None,
            local_snapshot_count: 0,
            newest_local_snapshot_ms: None,
            unreadable: vec![],
        },
        icloud: ICloudState { drive_folder_present: true, signed_in: Some(true), drive_enabled: Some(true), desktop_synced: Some(true), documents_synced: Some(true), inferred: true, unreadable: vec![] },
        full_disk_access: AccessStatus::Denied,
        stale_days: 7,
        read_at: 0,
    }
}

fn repo() -> UnsyncedRepo {
    UnsyncedRepo { path: PathBuf::from("/p/app"), reasons: vec!["uncommitted"] }
}

fn gate(status: Status, auto: Option<bool>, unpushed: usize, unreadable_repos: u32) -> BackupGate {
    evaluate(GateInput { backup: backup(status, auto), unpushed: (0..unpushed).map(|_| repo()).collect(), unreadable_repos })
}

fn ack(acknowledged: bool, typed: Option<&str>) -> BackupAck {
    BackupAck { acknowledged, typed: typed.map(String::from) }
}

#[test]
fn level_table() {
    use GateLevel::*;
    use Status::*;
    for (status, auto, unpushed, unreadable, expected) in [
        (Recent, Some(true), 0, 0, Ok),
        (Recent, None, 0, 0, Ok),
        (Recent, Some(true), 2, 0, Warn),       // work only here, but a backup exists
        (Recent, Some(false), 0, 0, Warn),      // automatic backups are off
        (Recent, Some(true), 0, 3, Warn),       // some repositories could not be read
        (NoDestination, Some(true), 0, 0, Strong),
        (Stale, Some(true), 0, 0, Strong),
        (NeverBackedUp, Some(true), 0, 0, Strong),
        (Stale, Some(true), 5, 0, Strong),
        (Unknown, Some(true), 0, 0, Warn),      // cannot verify, nothing at risk
        (Unknown, None, 1, 0, Strong),          // cannot verify, and work only here
    ] {
        let g = gate(status, auto, unpushed, unreadable);
        assert_eq!(g.level, expected, "{status:?} auto={auto:?} unpushed={unpushed} unreadable={unreadable}");
    }
}

#[test]
fn reasons_name_what_is_wrong_most_important_first() {
    let g = gate(Status::Stale, Some(false), 1, 2);
    assert_eq!(g.reasons, ["stale", "unpushedRepos", "autoBackupOff", "reposUnreadable"]);
    assert!(gate(Status::Recent, Some(true), 0, 0).reasons.is_empty());
    assert_eq!(gate(Status::Unknown, None, 0, 0).reasons, ["unknown"]);
    assert_eq!(gate(Status::NoDestination, None, 0, 0).reasons, ["noDestination"]);
}

#[test]
fn an_ok_gate_asks_nothing() {
    let g = gate(Status::Recent, Some(true), 0, 0);
    assert!(verify(&g, None).is_ok());
    assert!(verify(&g, Some(&ack(false, None))).is_ok());
}

#[test]
fn a_warning_needs_the_user_to_say_they_saw_it() {
    let g = gate(Status::Recent, Some(true), 1, 0);
    assert!(matches!(verify(&g, None), Err(AppError::BackupGate(_))));
    assert!(verify(&g, Some(&ack(false, None))).is_err());
    assert!(verify(&g, Some(&ack(true, None))).is_ok());
    // Typing something extra does not matter for a warning.
    assert!(verify(&g, Some(&ack(true, Some("whatever")))).is_ok());
}

#[test]
fn a_strong_gate_needs_the_word_typed_and_the_acknowledgement() {
    for status in [Status::NoDestination, Status::Stale, Status::NeverBackedUp] {
        let g = gate(status, Some(true), 0, 0);
        for bad in [None, Some(ack(false, None)), Some(ack(true, None)), Some(ack(true, Some(""))), Some(ack(true, Some("sim"))), Some(ack(true, Some("CONFIRMOS"))), Some(ack(false, Some("CONFIRMO")))] {
            let err = verify(&g, bad.as_ref()).unwrap_err();
            assert!(matches!(err, AppError::BackupGate(ref m) if m.contains("CONFIRMO")), "{status:?} {bad:?}");
        }
        for good in ["CONFIRMO", "confirmo", "  Confirmo  "] {
            assert!(verify(&g, Some(&ack(true, Some(good)))).is_ok(), "{good:?}");
        }
    }
}

#[test]
fn the_refusal_has_its_own_error_kind_so_the_ui_can_reopen_the_dialog() {
    let err = verify(&gate(Status::Stale, None, 0, 0), None).unwrap_err();
    let json = serde_json::to_value(&err).unwrap();
    assert_eq!(json["kind"], "backupGate");
    assert!(json["message"].as_str().unwrap().contains("CONFIRMO"));
}

#[test]
fn the_gate_serializes_for_the_dialog() {
    let g = gate(Status::Recent, Some(false), 1, 0);
    let v = serde_json::to_value(&g).unwrap();
    assert_eq!(v["level"], "warn");
    assert_eq!(v["unpushed"][0]["reasons"][0], "uncommitted");
    assert_eq!(v["backup"]["timeMachine"]["status"], "recent");
}

// ---- repositories with work only on this Mac

#[test]
fn unsynced_repos_use_real_git_and_name_the_reasons() {
    let (_t, root) = canonical_tmp();
    let bare = root.join("origin.git");
    std::fs::create_dir_all(&bare).unwrap();
    git(&bare, &["init", "-q", "--bare", "-b", "main"]);

    let clean = root.join("projects/clean");
    repo_with_commit(&clean);
    git(&clean, &["remote", "add", "origin", bare.to_str().unwrap()]);
    git(&clean, &["push", "-q", "-u", "origin", "main"]);

    let dirty = root.join("projects/dirty");
    repo_with_commit(&dirty);
    git(&dirty, &["remote", "add", "origin", bare.to_str().unwrap()]);
    git(&dirty, &["push", "-q", "-u", "origin", "main"]);
    write(&dirty, "wip.txt", "x\n");
    commit_file(&dirty, "ahead.txt", "x\n");
    write(&dirty, "more.txt", "x\n");

    let local_only = root.join("projects/local-only");
    repo_with_commit(&local_only);

    let empty = root.join("projects/empty");
    std::fs::create_dir_all(&empty).unwrap();
    git(&empty, &["init", "-q", "-b", "main"]);

    let (found, unreadable) = unsynced_repos(&[root.join("projects")], &Git::new(&Isolated), &AtomicBool::new(false));
    let by = |name: &str| found.iter().find(|r| r.path.ends_with(name)).map(|r| r.reasons.clone());
    assert_eq!(by("clean"), None, "everything pushed");
    assert_eq!(by("dirty"), Some(vec!["uncommitted", "unpushedCommits"]));
    assert_eq!(by("local-only"), Some(vec!["noRemote"]));
    assert_eq!(by("empty"), None, "a repository with no commits has nothing to lose");
    assert_eq!(unreadable, 0);
}

#[test]
fn an_unreadable_repository_is_counted_not_ignored() {
    let (_t, root) = canonical_tmp();
    repo_with_commit(&root.join("projects/good"));
    std::fs::create_dir_all(root.join("projects/bad/.git")).unwrap(); // looks like a repo, git cannot read it
    let (_, unreadable) = unsynced_repos(&[root.join("projects")], &Git::new(&Isolated), &AtomicBool::new(false));
    assert_eq!(unreadable, 1);
}

#[test]
fn without_git_every_repository_is_unreadable() {
    let (_t, root) = canonical_tmp();
    repo_with_commit(&root.join("projects/a"));
    repo_with_commit(&root.join("projects/b"));
    let fake = FakeRunner::new().reply_failure(2, "no tools");
    let (found, unreadable) = unsynced_repos(&[root.join("projects")], &Git::new(&fake), &AtomicBool::new(false));
    assert!(found.is_empty());
    assert_eq!(unreadable, 2);
    assert_eq!(fake.calls().len(), 1, "only the availability check ran");
}

// ---- no batch cleanup command may skip the gate

/// The body of `fn name` up to the next command in the same file.
fn command_source(name: &str) -> Option<String> {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/commands");
    for entry in std::fs::read_dir(dir).unwrap().flatten() {
        let text = std::fs::read_to_string(entry.path()).unwrap();
        if let Some(start) = text.find(&format!("fn {name}(")) {
            let rest = &text[start..];
            let end = rest[1..].find("#[tauri::command]").map_or(rest.len(), |i| i + 1);
            return Some(rest[..end].to_string());
        }
    }
    None
}

#[test]
fn every_batch_cleanup_command_requires_the_gate() {
    for name in ["run_smart_care", "run_cleanup_rules", "remove_orphans", "move_duplicates_to_trash", "remove_dev_artifacts"] {
        let src = command_source(name).unwrap_or_else(|| panic!("command {name} not found"));
        assert!(src.contains("backup_ack: Option<BackupAck>"), "{name} does not take the acknowledgement");
        let check = src.find("require_backup_ack(").unwrap_or_else(|| panic!("{name} never checks the gate"));
        // The check comes before anything that changes the disk.
        for effect in ["move_to_trash(", "remove_items(", "empty_only(", "remove_artifacts(", "delete_items("] {
            if let Some(at) = src.find(effect) {
                assert!(check < at, "{name} calls {effect} before the gate");
            }
        }
    }
}
