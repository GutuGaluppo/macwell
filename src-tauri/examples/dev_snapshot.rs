//! Previews (and with --create, makes) a WIP snapshot: `dev_snapshot <repo> <tmp-dir> [--create] [--exclude path]...`.
//! Use it on a copy of a repository, never on the only copy of your work.
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use organizemymac_lib::dev::git::Git;
use organizemymac_lib::dev::snapshot::{create, plan};
use organizemymac_lib::exec::RealRunner;

fn main() {
    let mut args = std::env::args().skip(1);
    let (Some(repo), Some(tmp)) = (args.next().map(PathBuf::from), args.next().map(PathBuf::from)) else {
        eprintln!("usage: dev_snapshot <repo> <tmp-dir> [--create] [--exclude path]...");
        return;
    };
    let mut do_create = false;
    let mut excludes = Vec::new();
    while let Some(a) = args.next() {
        match a.as_str() {
            "--create" => do_create = true,
            "--exclude" => excludes.extend(args.next()),
            other => eprintln!("ignoring {other}"),
        }
    }
    let git = Git::new(&RealRunner);
    let cancel = AtomicBool::new(false);
    let now = chrono::Local::now();
    let p = plan(&git, &repo, &excludes, &now, &cancel).expect("plan");
    println!("branch={:?} head={:?} -> {}", p.branch, p.head.as_deref().map(|h| &h[..10]), p.ref_name);
    println!("changed tracked files: {}, new files: {} ({} bytes)", p.changed, p.untracked_count, p.untracked_bytes);
    for f in p.untracked.iter().take(10) {
        println!("  new {:>10} B  {}", f.size, f.path);
    }
    for s in &p.secrets {
        println!("  secret? {:?} {}", s.kind, s.path);
    }
    println!("tracked secrets: {}  blockers: {:?}", p.tracked_secrets, p.blockers);
    if do_create {
        match create(&git, &repo, &excludes, &tmp, &now, &cancel) {
            Ok(r) => println!("created {} at {} (parent {:?})", r.ref_name, r.commit, r.parent),
            Err(e) => println!("not created: {e}"),
        }
    }
}
