//! Prints the report for this Mac as Markdown: `cargo run --release --example report_preview [csv]`.
//! Reads the backup state and your project folders (path given as an argument); writes nothing.
use std::sync::atomic::AtomicBool;

use organizemymac_lib::backup::gate::{evaluate, GateInput};
use organizemymac_lib::backup::{read_state, timemachine::DEFAULT_STALE_DAYS};
use organizemymac_lib::db::{now_ms, OperationRecord};
use organizemymac_lib::dev::git::Git;
use organizemymac_lib::dev::report::unsynced_repos;
use organizemymac_lib::exec::RealRunner;
use organizemymac_lib::filesystem::probe_full_disk_access;
use organizemymac_lib::report::{build, render, ReportFormat, ReportInput};

fn main() {
    let home = dirs::home_dir().unwrap();
    let roots: Vec<_> = std::env::args().skip(1).filter(|a| a != "csv").map(std::path::PathBuf::from).collect();
    let csv = std::env::args().any(|a| a == "csv");
    let backup = read_state(&RealRunner, &home, now_ms(), DEFAULT_STALE_DAYS, probe_full_disk_access(&home));
    let (unpushed, unreadable_repos) = if roots.is_empty() { (vec![], 0) } else { unsynced_repos(&roots, &Git::new(&RealRunner), &AtomicBool::new(false)) };
    let gate = evaluate(GateInput { backup, unpushed, unreadable_repos });
    // A few sample actions, since this example has no database.
    let now = now_ms();
    let ops = vec![
        OperationRecord { id: 2, ts: now - 3_600_000, action: "quarantine".into(), path: format!("{}/Downloads/Figma.dmg", home.display()), size: 540_000_000, ok: true, detail: None },
        OperationRecord { id: 1, ts: now - 86_400_000, action: "trash".into(), path: format!("{}/Projects/app/node_modules", home.display()), size: 206_700_000, ok: true, detail: None },
    ];
    let report = build(ReportInput { now_ms: now, period_days: 30, operations: ops, snapshots: vec![], lost_exclusions: vec![], quarantine: vec![], gate });
    print!("{}", render(&report, if csv { ReportFormat::Csv } else { ReportFormat::Markdown }));
}
