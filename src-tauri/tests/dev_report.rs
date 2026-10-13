mod common;

use std::collections::HashMap;
use std::sync::atomic::AtomicBool;

use common::git::{canonical_tmp, commit_file, git, repo_with_commit, write, Isolated};
use organizemymac_lib::dev::git::Git;
use organizemymac_lib::dev::report::{build, latest_by_repo, DevReport};
use organizemymac_lib::dev::visibility::RemoteKind;
use organizemymac_lib::exec::FakeRunner;
use organizemymac_lib::filesystem::scanner::ScanEvent;

fn run(roots: &[std::path::PathBuf], cancel: &AtomicBool) -> (DevReport, Vec<String>) {
    let mut stages = Vec::new();
    let report = build(roots, &Git::new(&Isolated), &HashMap::new(), cancel, &mut |e| {
        if let ScanEvent::Stage { stage, done, total } = e {
            stages.push(format!("{stage} {done}/{total}"));
        }
    });
    (report, stages)
}

#[test]
fn reports_state_remotes_artifacts_and_nesting() {
    let (_t, root) = canonical_tmp();
    let a = root.join("app");
    repo_with_commit(&a);
    commit_file(&a, ".gitignore", "node_modules\n");
    commit_file(&a, "package.json", "{}\n");
    commit_file(&a, "package-lock.json", "{}\n");
    write(&a, "node_modules/x/i.js", "x");
    write(&a, "dirty.txt", "x\n");
    // A remote URL that carries a token: the report goes to the UI, so it must not.
    git(&a, &["remote", "add", "origin", "https://someuser:s3cr3tpass@github.com/me/app.git"]);
    let local = root.join("backup.git");
    std::fs::create_dir_all(&local).unwrap();
    git(&local, &["init", "-q", "--bare"]);
    git(&a, &["remote", "add", "backup", local.to_str().unwrap()]);
    let inner = a.join("packages/inner");
    repo_with_commit(&inner);

    let (r, stages) = run(&[root.clone()], &AtomicBool::new(false));

    assert_eq!(r.repos.len(), 2);
    assert!(r.git_error.is_none());
    let app = r.repos.iter().find(|x| x.path == a).unwrap();
    let st = app.status.as_ref().unwrap();
    assert_eq!((st.branch.as_deref(), st.untracked), (Some("main"), 2)); // dirty.txt and packages/inner/
    assert_eq!(app.remotes.len(), 2);
    let kind_of = |name: &str| app.remotes.iter().find(|r| r.name == name).unwrap().kind.clone();
    assert_eq!(kind_of("origin"), RemoteKind::Github { owner: "me".into(), repo: "app".into() });
    assert_eq!(kind_of("backup"), RemoteKind::Local);
    assert_eq!(app.artifact_bytes, 1); // node_modules/x/i.js holds one byte
    assert_eq!(r.artifacts.len(), 1);
    assert_eq!(r.artifacts[0].repo, a);
    let inner_report = r.repos.iter().find(|x| x.path == inner).unwrap();
    assert_eq!(inner_report.parent.as_deref(), Some(a.as_path()));

    let json = serde_json::to_string(&r).unwrap();
    assert!(!json.contains("s3cr3tpass") && !json.contains("someuser"), "credentials leaked into the report");
    assert!(json.contains("https://***@github.com/me/app.git"));

    assert_eq!(stages.first().map(String::as_str), Some("discover 0/1"));
    assert_eq!(stages.last().map(String::as_str), Some("repos 2/2"));
}

#[test]
fn one_unreadable_repository_does_not_hide_the_others() {
    let (_t, root) = canonical_tmp();
    repo_with_commit(&root.join("good"));
    // Looks like a repository to discovery but git cannot read it.
    std::fs::create_dir_all(root.join("bad/.git")).unwrap();
    let (r, _) = run(&[root], &AtomicBool::new(false));
    assert_eq!(r.repos.len(), 2);
    let bad = r.repos.iter().find(|x| x.path.ends_with("bad")).unwrap();
    assert!(bad.status.is_none() && bad.error.is_some());
    assert!(r.repos.iter().find(|x| x.path.ends_with("good")).unwrap().status.is_some());
}

#[test]
fn without_git_every_repository_carries_the_reason_and_nothing_is_run() {
    let (_t, root) = canonical_tmp();
    repo_with_commit(&root.join("a"));
    let fake = FakeRunner::new().reply_failure(2, "no developer tools");
    let r = build(&[root], &Git::new(&fake), &HashMap::new(), &AtomicBool::new(false), &mut |_| {});
    assert!(r.git_error.as_deref().is_some_and(|e| e.contains("Command Line Tools")));
    assert_eq!(r.repos.len(), 1);
    assert!(r.repos[0].status.is_none() && r.repos[0].error.is_some());
    assert_eq!(fake.calls().len(), 1, "only the availability check may run: {:?}", fake.calls());
}

#[test]
fn cancel_stops_early_and_snapshots_are_attached() {
    let (_t, root) = canonical_tmp();
    repo_with_commit(&root.join("a"));
    let (cancelled, _) = run(&[root.clone()], &AtomicBool::new(true));
    assert!(cancelled.repos.is_empty());

    let db = organizemymac_lib::db::Db::open_in_memory().unwrap();
    let repo = root.join("a").to_string_lossy().into_owned();
    db.record_snapshot(&repo, "wip/snapshot-2024-01-01", "old").unwrap();
    std::thread::sleep(std::time::Duration::from_millis(5));
    db.record_snapshot(&repo, "wip/snapshot-2024-01-02", "new").unwrap();
    let last = latest_by_repo(db.snapshots(None).unwrap());
    let r = build(&[root], &Git::new(&Isolated), &last, &AtomicBool::new(false), &mut |_| {});
    assert_eq!(r.repos[0].last_snapshot.as_ref().unwrap().ref_name, "wip/snapshot-2024-01-02");
}
