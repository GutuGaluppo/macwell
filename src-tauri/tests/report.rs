mod common;

use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::PathBuf;

use common::git::canonical_tmp;
use organizemymac_lib::backup::gate::{evaluate, BackupGate, GateInput};
use organizemymac_lib::backup::icloud::ICloudState;
use organizemymac_lib::backup::timemachine::{Status, TimeMachineState};
use organizemymac_lib::backup::BackupState;
use organizemymac_lib::db::{OperationRecord, SnapshotRecord};
use organizemymac_lib::dev::report::UnsyncedRepo;
use organizemymac_lib::error::AppError;
use organizemymac_lib::filesystem::safety::SafetyPolicy;
use organizemymac_lib::filesystem::AccessStatus;
use organizemymac_lib::report::export::write_report;
use organizemymac_lib::report::{build, to_csv, to_markdown, Report, ReportInput, MAX_LINES};

const DAY: i64 = 86_400_000;
const NOW: i64 = 1_791_900_000_000; // 2026-10-13

fn gate(status: Status, auto: Option<bool>, unpushed: &[&str]) -> BackupGate {
    let backup = BackupState {
        time_machine: TimeMachineState {
            status,
            destinations: vec![],
            auto_backup: auto,
            last_backup_ms: (status == Status::Recent || status == Status::Stale).then_some(NOW - 2 * DAY),
            last_backup_source: None,
            last_attempt_ms: None,
            age_days: (status == Status::Stale).then_some(30),
            running: false,
            percent: None,
            local_snapshot_count: 0,
            newest_local_snapshot_ms: None,
            unreadable: vec![],
        },
        icloud: ICloudState { drive_folder_present: true, signed_in: Some(true), drive_enabled: Some(true), desktop_synced: Some(true), documents_synced: Some(true), inferred: true, unreadable: vec![] },
        full_disk_access: AccessStatus::Denied,
        stale_days: 7,
        read_at: NOW,
    };
    evaluate(GateInput { backup, unpushed: unpushed.iter().map(|p| UnsyncedRepo { path: PathBuf::from(p), reasons: vec!["uncommitted"] }).collect(), unreadable_repos: 0 })
}

fn op(id: i64, age_days: i64, action: &str, path: &str, size: u64, ok: bool) -> OperationRecord {
    OperationRecord { id, ts: NOW - age_days * DAY, action: action.into(), path: path.into(), size, ok, detail: (!ok).then(|| "recusado".to_string()) }
}

fn input(ops: Vec<OperationRecord>, g: BackupGate) -> ReportInput {
    ReportInput { now_ms: NOW, period_days: 30, operations: ops, snapshots: vec![], lost_exclusions: vec![], quarantine: vec![], gate: g }
}

fn healthy() -> BackupGate {
    gate(Status::Recent, Some(true), &[])
}

#[test]
fn totals_count_everything_in_the_period_and_only_successes_add_up_in_size() {
    let r = build(input(
        vec![
            op(5, 1, "trash", "/a", 100, true),
            op(4, 2, "trash", "/b", 200, true),
            op(3, 2, "trash", "/c", 999, false), // failed: counted as an attempt, not as freed
            op(2, 3, "quarantine", "/d", 50, true),
            op(1, 45, "trash", "/old", 7777, true), // outside the period
        ],
        healthy(),
    ));
    let g = |a: &str| r.groups.iter().find(|g| g.action == a).unwrap();
    assert_eq!((g("trash").total, g("trash").ok, g("trash").bytes), (3, 2, 300));
    assert_eq!((g("quarantine").total, g("quarantine").ok, g("quarantine").bytes), (1, 1, 50));
    assert_eq!(r.groups[0].action, "trash", "most recent first");
    assert_eq!(r.recent.len(), 4);
    assert!(r.recent.iter().all(|l| l.path != "/old"));
    assert_eq!(g("trash").label, "Movido para a Lixeira");
}

#[test]
fn a_long_log_is_listed_up_to_a_cap_but_totals_still_count_it_all() {
    let ops: Vec<_> = (0..250).map(|i| op(i, 1, "trash", &format!("/f{i}"), 10, true)).collect();
    let r = build(input(ops, healthy()));
    assert_eq!(r.recent.len(), MAX_LINES);
    assert_eq!(r.recent_omitted, 50);
    assert_eq!(r.groups[0].total, 250);
    assert!(to_markdown(&r).contains("e mais 50 ações"));
}

#[test]
fn next_decision_follows_a_fixed_order_of_importance() {
    let next = |g: BackupGate, snaps: Vec<SnapshotRecord>, q: Vec<(String, u64, bool)>, lost: Vec<String>| {
        let mut i = input(vec![], g);
        i.snapshots = snaps;
        i.quarantine = q;
        i.lost_exclusions = lost;
        build(i).next.code
    };
    let local = |pushed| vec![SnapshotRecord { id: "s".into(), repo: "/r".into(), ref_name: "wip/snapshot-2026-10-12".into(), commit_id: "c".into(), created_at: NOW - DAY, pushed_at: pushed }];
    let held = vec![("/x".to_string(), 10, true)];
    let lost = vec!["/p/node_modules".to_string()];
    // Protection before anything else, even with every other pending item.
    assert_eq!(next(gate(Status::NoDestination, None, &["/r"]), local(None), held.clone(), lost.clone()), "backup");
    assert_eq!(next(gate(Status::Stale, None, &[]), vec![], vec![], vec![]), "backup");
    assert_eq!(next(gate(Status::Unknown, None, &[]), vec![], vec![], vec![]), "backup");
    // Then work that only exists here.
    assert_eq!(next(gate(Status::Recent, Some(true), &["/r"]), local(None), held.clone(), lost.clone()), "snapshot");
    assert_eq!(next(gate(Status::Recent, Some(true), &[]), local(None), held.clone(), lost.clone()), "pushSnapshots");
    assert_eq!(next(gate(Status::Recent, Some(true), &[]), local(Some(1)), held.clone(), lost.clone()), "quarantine");
    assert_eq!(next(gate(Status::Recent, Some(true), &[]), vec![], vec![], lost), "exclusions");
    assert_eq!(next(gate(Status::Recent, Some(false), &[]), vec![], vec![], vec![]), "autoBackup");
    assert_eq!(next(gate(Status::Recent, Some(true), &[]), vec![], vec![], vec![]), "none");
}

#[test]
fn pending_items_say_what_is_open_with_examples_and_sizes() {
    let mut i = input(vec![], gate(Status::Stale, Some(false), &["/p/a", "/p/b"]));
    i.quarantine = vec![("/q/one".into(), 100, true), ("/q/two".into(), 50, false)];
    let r = build(i);
    let by = |c: &str| r.pending.iter().find(|p| p.code == c).unwrap();
    assert_eq!(by("backupStale").detail, "O último backup concluído foi há 30 dias; o limite é de 7.");
    assert_eq!(by("unpushedRepos").count, 2);
    assert_eq!(by("unpushedRepos").items[0].text, "/p/a");
    assert_eq!(by("unpushedRepos").items[0].note.as_deref(), Some("alterações sem commit"), "reasons are in plain words, not codes");
    assert_eq!((by("quarantine").count, by("quarantine").bytes), (2, 100), "only what is still in place takes space");
    assert!(r.pending.iter().any(|p| p.code == "autoBackupOff"));
    assert!(build(input(vec![], healthy())).pending.is_empty());
}

#[test]
fn the_same_state_always_gives_the_same_report() {
    let make = || build(input(vec![op(1, 1, "trash", "/a", 1, true)], gate(Status::Stale, None, &["/r"])));
    assert_eq!(to_markdown(&make()), to_markdown(&make()));
    assert_eq!(to_csv(&make()), to_csv(&make()));
}

// ---- hostile names

const HOSTILE: [&str; 7] = [
    "/Users/me/a|b.txt",
    "/Users/me/back`tick`.txt",
    "/Users/me/<script>alert(1)</script>.html",
    "/Users/me/[click](javascript:alert(1)).txt",
    "/Users/me/line\nbreak.txt",
    "/Users/me/=HYPERLINK(\"http://evil\").txt",
    "/Users/me/, \"quoted\".txt",
];

fn hostile_report() -> Report {
    let ops = HOSTILE.iter().enumerate().map(|(i, p)| op(i as i64, 1, "trash", p, 10, true)).collect();
    build(input(ops, healthy()))
}

/// Text the report writes itself (labels, details) never carries raw angle brackets.
fn md_text_is_escaped(md: &str) -> bool {
    md.lines().filter(|l| !l.contains('`')).all(|l| !l.contains('<'))
}

/// Splits a Markdown table row on unescaped pipes.
fn md_cells(line: &str) -> Vec<String> {
    let mut cells = vec![String::new()];
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '\\' if chars.peek() == Some(&'|') => cells.last_mut().unwrap().push(chars.next().unwrap()),
            '|' => cells.push(String::new()),
            _ => cells.last_mut().unwrap().push(c),
        }
    }
    cells
}

#[test]
fn markdown_cannot_be_broken_or_turned_into_html_by_a_file_name() {
    let md = to_markdown(&hostile_report());
    // A name is shown as inline code, where a Markdown reader prints it literally: no HTML, no link. Every
    // dangerous fragment must sit inside such a span (an odd number of backticks before it on its line).
    for needle in ["<script", "](javascript:"] {
        let mut seen = 0;
        for line in md.lines().filter(|l| l.contains(needle)) {
            seen += 1;
            let before = &line[..line.find(needle).unwrap()];
            assert_eq!(before.matches('`').count() % 2, 1, "{needle} outside a code span: {line}");
        }
        assert!(seen > 0, "{needle} should appear, literally, in the report");
    }
    // Outside code spans the report's own text is escaped.
    assert!(md_text_is_escaped(&md));
    // Every row of the actions table is one line with exactly the five columns.
    let rows: Vec<&str> = md.lines().skip_while(|l| !l.starts_with("| Quando | Ação")).skip(2).take_while(|l| l.starts_with('|')).collect();
    assert_eq!(rows.len(), HOSTILE.len(), "a newline in a name broke a row:\n{md}");
    for row in rows {
        assert_eq!(md_cells(row).len(), 5 + 2, "{row}");
    }
}

/// Minimal RFC 4180 reader for the test.
fn parse_csv(text: &str) -> Vec<Vec<String>> {
    let (mut rows, mut row, mut cell, mut quoted) = (Vec::new(), Vec::new(), String::new(), false);
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, quoted) {
            ('"', true) if chars.peek() == Some(&'"') => {
                cell.push('"');
                chars.next();
            }
            ('"', true) => quoted = false,
            ('"', false) if cell.is_empty() => quoted = true,
            (',', false) => row.push(std::mem::take(&mut cell)),
            ('\n', false) => {
                row.push(std::mem::take(&mut cell));
                rows.push(std::mem::take(&mut row));
            }
            (c, _) => cell.push(c),
        }
    }
    rows
}

#[test]
fn csv_neutralises_formulas_and_round_trips_awkward_cells() {
    let csv = to_csv(&hostile_report());
    let rows = parse_csv(&csv);
    assert_eq!(rows[0], ["secao", "tipo", "quando", "caminho", "tamanho", "resultado", "detalhe"]);
    assert!(rows.iter().all(|r| r.len() == 7), "{rows:?}");
    let paths: Vec<&str> = rows.iter().filter(|r| r[0] == "feito").map(|r| r[3].as_str()).collect();
    assert_eq!(paths.len(), HOSTILE.len());
    for original in HOSTILE {
        assert!(paths.contains(&original), "{original:?} did not survive: {paths:?}");
    }
    // A cell that starts like a formula carries a leading apostrophe, so no spreadsheet runs it.
    for r in &rows {
        for cell in r {
            assert!(!cell.starts_with(['=', '+', '-', '@', '\t', '\r']), "formula-like cell: {cell:?}");
        }
    }
    let guarded = parse_csv(&to_csv(&build(input(
        ["=1+1", "+2", "-3", "@SUM(A1)", "\tx"].iter().enumerate().map(|(i, p)| op(i as i64, 1, "trash", p, 1, true)).collect(),
        healthy(),
    ))));
    let cells: Vec<&str> = guarded.iter().filter(|r| r[0] == "feito").map(|r| r[3].as_str()).collect();
    assert_eq!(cells.len(), 5);
    assert!(cells.iter().all(|c| c.starts_with('\'')), "{cells:?}");
}

#[test]
fn the_report_has_its_sections_in_a_useful_order() {
    let mut i = input(vec![op(1, 1, "trash", "/a", 5, true)], gate(Status::Stale, Some(false), &["/r"]));
    i.snapshots = vec![SnapshotRecord { id: "s".into(), repo: "/r".into(), ref_name: "wip/snapshot-2026-10-12".into(), commit_id: "c".into(), created_at: NOW - DAY, pushed_at: None }];
    let md = to_markdown(&build(i));
    let at = |h: &str| md.find(h).unwrap_or_else(|| panic!("missing {h}"));
    assert!(at("## Próxima decisão") < at("## Backup") && at("## Backup") < at("## O que foi feito") && at("## O que foi feito") < at("## O que falta"));
    assert!(md.contains("só neste Mac"));
    assert!(md.starts_with("# Relatório do OrganizeMyMac"));
}

// ---- writing the file

fn policy(root: &std::path::Path) -> SafetyPolicy {
    SafetyPolicy::custom(vec![root.join("system")], vec![root.join("home")], vec![root.join("data")])
}

#[test]
fn writes_a_new_file_privately_and_leaves_no_temporary_file() {
    let (_t, root) = canonical_tmp();
    let dir = root.join("home/Desktop");
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("relatorio.md");
    let n = write_report(&policy(&root), &path, "md", "# oi\n", false).unwrap();
    assert_eq!(n, 5);
    assert_eq!(fs::read_to_string(&path).unwrap(), "# oi\n");
    assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
    assert_eq!(fs::read_dir(&dir).unwrap().count(), 1, "no leftovers");
    // The extension check is case-insensitive. (The volume is too: relatorio.md and RELATORIO.MD are one file.)
    assert!(write_report(&policy(&root), &dir.join("OUTRO.MD"), "md", "x", false).is_ok());
    assert!(matches!(write_report(&policy(&root), &dir.join("RELATORIO.MD"), "md", "x", false), Err(AppError::Exists(_))));
}

#[test]
fn never_replaces_without_being_told_to() {
    let (_t, root) = canonical_tmp();
    let dir = root.join("home");
    let path = dir.join("r.csv");
    fs::create_dir_all(&dir).unwrap();
    fs::write(&path, "precious").unwrap();
    let err = write_report(&policy(&root), &path, "csv", "new", false).unwrap_err();
    assert!(matches!(err, AppError::Exists(_)), "{err:?}");
    assert_eq!(serde_json::to_value(&err).unwrap()["kind"], "exists");
    assert_eq!(fs::read_to_string(&path).unwrap(), "precious");
    assert_eq!(fs::read_dir(&dir).unwrap().count(), 1, "the failed write left nothing behind");
    // With permission it replaces, atomically.
    write_report(&policy(&root), &path, "csv", "new", true).unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "new");
    assert_eq!(fs::read_dir(&dir).unwrap().count(), 1);
}

#[test]
fn refuses_unsafe_or_malformed_targets() {
    let (_t, root) = canonical_tmp();
    let home = root.join("home");
    fs::create_dir_all(&home).unwrap();
    fs::create_dir_all(root.join("system")).unwrap();
    fs::create_dir_all(root.join("data")).unwrap();
    let p = policy(&root);
    let w = |path: PathBuf, ext: &str| write_report(&p, &path, ext, "x", true);

    assert!(w(PathBuf::from("relative.md"), "md").is_err());
    assert!(w(home.join("r.txt"), "md").is_err(), "wrong extension");
    assert!(w(home.join("missing-dir/r.md"), "md").is_err());
    assert!(w(root.join("system/r.md"), "md").is_err(), "protected place");
    assert!(w(root.join("data/r.md"), "md").is_err(), "the app's own data");
    fs::create_dir_all(home.join("folder.md")).unwrap();
    assert!(w(home.join("folder.md"), "md").is_err(), "a folder");

    // A symbolic link is never written through, even to replace.
    let victim = home.join("victim.txt");
    fs::write(&victim, "keep me").unwrap();
    symlink(&victim, home.join("link.md")).unwrap();
    assert!(w(home.join("link.md"), "md").is_err());
    assert_eq!(fs::read_to_string(&victim).unwrap(), "keep me");
    assert!(root.join("system").read_dir().unwrap().next().is_none());
}
