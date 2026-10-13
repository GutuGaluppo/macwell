//! Read-only Smart Care report: `cargo run --release --example smart_care_report`.

use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use organizemymac_lib::filesystem::scanner::{prompting_locations, ScanEvent, ScanOptions};
use organizemymac_lib::smart_care::run;

fn main() {
    let home = dirs::home_dir().unwrap();
    let base = ScanOptions { root: home.clone(), skipped: prompting_locations(&home, false), ..Default::default() };
    let cancel = Arc::new(AtomicBool::new(false));
    let r = run(&home, &base, &cancel, &[], &mut |e| {
        if let ScanEvent::Stage { stage, .. } = e {
            eprintln!("stage {stage}");
        }
    })
    .unwrap();
    let gb = |b: u64| b as f64 / 1e9;
    println!("took {:.1} s; disk free {:.0} of {:.0} GB", r.duration_ms as f64 / 1000.0, gb(r.disk_free), gb(r.disk_total));
    println!("trash readable={} {:.2} GB", r.trash.readable, gb(r.trash.bytes));
    println!("downloads {} items, {} preselected", r.downloads.len(), r.downloads.iter().filter(|d| d.selected).count());
    println!("duplicates {} groups, {:.2} GB", r.duplicates.len(), gb(r.duplicates.iter().map(|g| g.wasted).sum()));
    println!("large old {} files", r.large_old.len());
    println!("unused apps {}: {:?}", r.unused_apps.len(), r.unused_apps.iter().map(|a| a.name.as_str()).collect::<Vec<_>>());
    println!("leftover groups {}", r.leftovers.len());
    for rec in &r.recommendations {
        println!("  [{}] {} — {}", rec.level, rec.title, rec.detail);
    }
}
