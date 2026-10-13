//! Read-only Homebrew analysis: `cargo run --release --example brew_report -- ~/Projects`.
//! Runs only `brew outdated --json=v2` (auto-update and analytics off) and reads file names.
use std::sync::atomic::AtomicBool;

use organizemymac_lib::brew::{analyze, find_brew};
use organizemymac_lib::dev::discover::{discover, DEFAULT_MAX_DEPTH};
use organizemymac_lib::exec::RealRunner;

fn main() {
    let roots: Vec<std::path::PathBuf> = std::env::args().skip(1).map(Into::into).collect();
    let repos: Vec<_> = if roots.is_empty() { vec![] } else { discover(&roots, DEFAULT_MAX_DEPTH, &AtomicBool::new(false)).repos.into_iter().map(|r| r.path).collect() };
    let started = std::time::Instant::now();
    let r = analyze(&RealRunner, find_brew(), &repos);
    println!("brew: {:?}  instalado={}  repositórios lidos={}  ({:.1} s)", find_brew(), r.installed, r.repos_scanned, started.elapsed().as_secs_f64());
    if let Some(e) = &r.error {
        println!("erro: {e}");
    }
    println!("{} pacotes desatualizados", r.packages.len());
    for p in &r.packages {
        println!("  {:?} {:<20} {} -> {}  {:?}  projetos={}", p.outdated.risk, p.outdated.name, p.outdated.installed.join(","), p.outdated.current, p.outdated.change, p.projects.len());
    }
}
