mod common;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use chrono::{DateTime, Local, TimeZone};
use common::git::{canonical_tmp, commit_file, git, repo_with_commit, write, Isolated};
use organizemymac_lib::dev::git::Git;
use organizemymac_lib::dev::push::{plan_push, push, PushConsent, PushWarning};
use organizemymac_lib::dev::snapshot::create;
use organizemymac_lib::dev::visibility::Visibility;

const YES: PushConsent = PushConsent { confirmed: true, accept_exposure: false };

fn now() -> DateTime<Local> {
    Local.with_ymd_and_hms(2024, 5, 6, 7, 8, 9).unwrap()
}

fn bare(root: &Path, name: &str) -> PathBuf {
    let p = root.join(name);
    fs::create_dir_all(&p).unwrap();
    git(&p, &["init", "-q", "--bare", "-b", "main"]);
    p
}

/// A repository with a snapshot branch and `origin` pointing at a local bare repository.
fn with_snapshot(root: &Path) -> (PathBuf, PathBuf, String) {
    let remote = bare(root, "origin.git");
    let r = root.join("work");
    repo_with_commit(&r);
    git(&r, &["remote", "add", "origin", remote.to_str().unwrap()]);
    write(&r, "wip.txt", "work in progress\n");
    let snap = create(&Git::new(&Isolated), &r, &[], &root.join("tmp"), &now(), &AtomicBool::new(false)).unwrap();
    (r, remote, snap.ref_name)
}

fn remote_refs(remote: &Path) -> String {
    git(remote, &["for-each-ref", "--format=%(refname) %(objectname)"])
}

#[test]
fn pushes_exactly_one_ref_and_changes_nothing_locally() {
    let (_t, root) = canonical_tmp();
    let (r, remote, name) = with_snapshot(&root);
    let local_before = (
        fs::read(r.join(".git/config")).unwrap(),
        fs::read(r.join(".git/index")).unwrap(),
        git(&r, &["rev-parse", "HEAD"]),
        git(&r, &["--no-optional-locks", "status", "--porcelain=v2", "--branch"]),
    );
    let commit = git(&r, &["rev-parse", &format!("refs/heads/{name}")]).trim().to_string();

    let result = push(&Git::new(&Isolated), &r, &name, "origin", None, YES).unwrap();

    assert_eq!(result.commit, commit);
    assert_eq!(remote_refs(&remote).trim(), format!("refs/heads/{name} {commit}"), "main must not be pushed");
    // No upstream was set up, no config changed, nothing local moved.
    let local_after = (
        fs::read(r.join(".git/config")).unwrap(),
        fs::read(r.join(".git/index")).unwrap(),
        git(&r, &["rev-parse", "HEAD"]),
        git(&r, &["--no-optional-locks", "status", "--porcelain=v2", "--branch"]),
    );
    assert_eq!(local_before, local_after);
}

#[test]
fn never_overwrites_a_different_branch_on_the_remote() {
    let (_t, root) = canonical_tmp();
    let (r, remote, name) = with_snapshot(&root);
    // Someone else already has a branch with this name and different content.
    let other = root.join("other");
    git(&root, &["clone", "-q", remote.to_str().unwrap(), "other"]);
    git(&other, &["checkout", "-q", "-b", &name]);
    commit_file(&other, "theirs.txt", "not yours\n");
    git(&other, &["push", "-q", "origin", &name]);
    let theirs = remote_refs(&remote);

    let err = push(&Git::new(&Isolated), &r, &name, "origin", None, YES).unwrap_err().to_string();
    assert!(err.contains("nada foi sobrescrito"), "{err}");
    assert_eq!(remote_refs(&remote), theirs, "the remote branch must be untouched");
}

#[test]
fn only_our_branches_and_configured_remotes_are_accepted() {
    let (_t, root) = canonical_tmp();
    let (r, remote, name) = with_snapshot(&root);
    let g = Git::new(&Isolated);
    for bad in ["main", "wip/other", &format!("+{name}"), &format!("{name}:main")] {
        assert!(push(&g, &r, bad, "origin", None, YES).is_err(), "{bad}");
    }
    assert!(push(&g, &r, "wip/snapshot-1999-01-01", "origin", None, YES).is_err(), "unknown branch");
    for bad in ["nope", remote.to_str().unwrap(), "https://example.invalid/x.git", "--mirror"] {
        assert!(push(&g, &r, &name, bad, None, YES).is_err(), "{bad}");
    }
    assert!(remote_refs(&remote).trim().is_empty());
}

#[test]
fn needs_confirmation_and_a_local_remote_needs_no_exposure_warning() {
    let (_t, root) = canonical_tmp();
    let (r, remote, name) = with_snapshot(&root);
    let g = Git::new(&Isolated);
    let plan = plan_push(&g, &r, &name, "origin", None).unwrap();
    assert_eq!(plan.warning, None);
    let no = PushConsent { confirmed: false, accept_exposure: true };
    assert!(push(&g, &r, &name, "origin", None, no).is_err());
    assert!(remote_refs(&remote).trim().is_empty());
    assert!(push(&g, &r, &name, "origin", None, YES).is_ok());
}

#[test]
fn public_and_unknown_remotes_stop_at_the_warning_without_touching_the_network() {
    let (_t, root) = canonical_tmp();
    let (r, _remote, name) = with_snapshot(&root);
    git(&r, &["remote", "add", "gh", "git@github.com:me/x.git"]);
    let g = Git::new(&Isolated);

    let unknown = plan_push(&g, &r, &name, "gh", None).unwrap();
    assert_eq!(unknown.warning, Some(PushWarning::Unknown));
    let public = plan_push(&g, &r, &name, "gh", Some((Visibility::Public, 123))).unwrap();
    assert_eq!(public.warning, Some(PushWarning::Public));
    assert_eq!(public.visibility_checked_at, Some(123));
    let private = plan_push(&g, &r, &name, "gh", Some((Visibility::PrivateOrMissing, 123))).unwrap();
    assert_eq!(private.warning, None);

    // Without accepting the warning the command fails before ssh could ever start (this would
    // otherwise try to reach github.com).
    assert!(push(&g, &r, &name, "gh", None, YES).is_err());
    assert!(push(&g, &r, &name, "gh", Some((Visibility::Public, 1)), YES).is_err());
}

#[test]
fn uses_the_real_push_url_and_hides_credentials() {
    let (_t, root) = canonical_tmp();
    let (r, remote, name) = with_snapshot(&root);
    // fetch URL looks like a hosted remote, but pushes go to the local bare repository.
    git(&r, &["remote", "set-url", "origin", "https://user:secret@example.invalid/me/x.git"]);
    git(&r, &["remote", "set-url", "--push", "origin", remote.to_str().unwrap()]);
    let g = Git::new(&Isolated);
    let plan = plan_push(&g, &r, &name, "origin", None).unwrap();
    assert_eq!(plan.url, remote.to_str().unwrap());
    assert_eq!(plan.warning, None);
    assert!(push(&g, &r, &name, "origin", None, YES).is_ok());
    assert!(!remote_refs(&remote).trim().is_empty());
}

#[test]
fn failure_messages_never_contain_credentials() {
    let (_t, root) = canonical_tmp();
    let (r, _remote, name) = with_snapshot(&root);
    // Port 1 on localhost refuses at once: no external network is involved.
    git(&r, &["remote", "set-url", "origin", "https://someuser:s3cr3tpass@localhost:1/me/x.git"]);
    let g = Git::new(&Isolated);
    let plan = plan_push(&g, &r, &name, "origin", None).unwrap();
    assert!(!plan.url.contains("s3cr3tpass") && !plan.url.contains("someuser"), "{}", plan.url);
    let consent = PushConsent { confirmed: true, accept_exposure: true };
    let err = push(&g, &r, &name, "origin", None, consent).unwrap_err().to_string();
    assert!(err.contains("o envio falhou"), "{err}");
    assert!(!err.contains("s3cr3tpass") && !err.contains("someuser"), "{err}");
}

#[test]
fn hooks_and_submodule_recursion_do_not_run_on_push() {
    let (_t, root) = canonical_tmp();
    let (r, remote, name) = with_snapshot(&root);
    let marker = root.join("hook-ran");
    let hook = r.join(".git/hooks/pre-push");
    fs::write(&hook, format!("#!/bin/sh\ntouch '{}'\nexit 1\n", marker.display())).unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    git(&r, &["config", "push.recurseSubmodules", "on-demand"]);
    assert!(push(&Git::new(&Isolated), &r, &name, "origin", None, YES).is_ok());
    assert!(!marker.exists(), "the pre-push hook ran");
    assert!(!remote_refs(&remote).trim().is_empty());
}

#[test]
fn a_mirror_remote_fails_closed() {
    let (_t, root) = canonical_tmp();
    let (r, remote, name) = with_snapshot(&root);
    git(&r, &["config", "remote.origin.mirror", "true"]);
    assert!(push(&Git::new(&Isolated), &r, &name, "origin", None, YES).is_err());
    assert!(remote_refs(&remote).trim().is_empty());
}
