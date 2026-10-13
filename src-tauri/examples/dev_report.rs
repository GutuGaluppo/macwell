//! Read-only repository report: `cargo run --release --example dev_report -- ~/Projects`.
//! Nothing is modified and nothing leaves the machine.
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;

use organizemymac_lib::dev::artifacts::{builtin_rules, scan_repo};
use organizemymac_lib::dev::discover::{discover, DEFAULT_MAX_DEPTH};
use organizemymac_lib::dev::git::Git;
use organizemymac_lib::dev::secrets;
use organizemymac_lib::exec::RealRunner;

fn main() {
    let roots: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    if roots.is_empty() {
        eprintln!("usage: dev_report <folder>...");
        return;
    }
    let t = std::time::Instant::now();
    let found = discover(&roots, DEFAULT_MAX_DEPTH, &AtomicBool::new(false));
    println!("{} repositories in {:.2} s", found.repos.len(), t.elapsed().as_secs_f64());
    for w in &found.warnings {
        println!("  aviso: {w}");
    }
    let runner = RealRunner;
    let git = Git::new(&runner);
    if let Err(e) = git.available() {
        println!("{e}");
        return;
    }
    let rules = builtin_rules();
    let cancel = AtomicBool::new(false);
    let mut total = 0u64;
    let mut safe = 0u64;
    for repo in &found.repos {
        let name = repo.path.strip_prefix(&repo.root).unwrap_or(&repo.path).display();
        match git.state(&repo.path) {
            Ok(s) => {
                let st = &s.status;
                let ab = match (st.ahead, st.behind) {
                    (Some(a), Some(b)) => format!("↑{a} ↓{b}"),
                    _ => "sem upstream".into(),
                };
                println!(
                    "  {name:<40} {:<18} {ab:<14} alterações={} remote={} sub={} embutidos={}{}",
                    st.branch.clone().unwrap_or_else(|| "(detached)".into()),
                    st.uncommitted(),
                    s.remotes.first().map_or("nenhum", |r| r.name.as_str()),
                    s.submodules.len(),
                    s.embedded.len(),
                    if repo.parent.is_some() { "  [aninhado]" } else { "" },
                );
            }
            Err(e) => println!("  {name:<40} erro: {e}"),
        }
        if let Ok(sc) = secrets::scan_repo(&git, &repo.path, &cancel) {
            for f in &sc.findings {
                println!("      segredo?       {:?} ({:?}{}) {}", f.kind, f.source, if f.tracked { ", versionado" } else { ", NÃO versionado" }, f.path);
            }
        }
        for a in scan_repo(&git, &repo.path, &rules, &cancel) {
            total += a.size;
            if a.confidence == organizemymac_lib::types::Confidence::Safe {
                safe += a.size;
            }
            println!(
                "      {:<14} {:>9.1} MB  {:<7?} {:?}  {}",
                a.kind,
                a.size as f64 / 1e6,
                a.confidence,
                a.notes,
                a.path.strip_prefix(&repo.path).unwrap_or(&a.path).display()
            );
        }
    }
    println!("regeneráveis: {:.2} GB no total, {:.2} GB marcados como seguros", total as f64 / 1e9, safe as f64 / 1e9);
}
