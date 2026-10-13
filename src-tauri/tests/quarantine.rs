mod common;

use std::fs;
use std::os::unix::fs::{symlink, MetadataExt};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use chrono::{Local, TimeZone};
use common::git::canonical_tmp;
use organizemymac_lib::cleanup::{RemovalItem, RemovalRequest};
use organizemymac_lib::db::Db;
use organizemymac_lib::filesystem::safety::{SafetyPolicy, QUARANTINE_DIR};
use organizemymac_lib::quarantine::{empty, move_into, restore, Env, RealVolumes, RemovalMode, Volumes};

/// Pretends that everything under `other` lives on another volume, mounted at `other`.
struct FakeVolumes {
    other: PathBuf,
    /// Where the other volume claims to be mounted (a wrong answer simulates a destination on the wrong device).
    other_mount: PathBuf,
}

impl Volumes for FakeVolumes {
    fn device(&self, p: &Path) -> std::io::Result<u64> {
        Ok(if p.starts_with(&self.other) { 2 } else { 1 })
    }
    fn mount_point(&self, _: &Path) -> std::io::Result<PathBuf> {
        Ok(self.other_mount.clone())
    }
}

struct World {
    _t: tempfile::TempDir,
    root: PathBuf,
    data_dir: PathBuf,
    data_root: PathBuf,
    db: Db,
    policy: SafetyPolicy,
}

fn world() -> World {
    let (t, root) = canonical_tmp();
    let data_dir = root.join("data");
    fs::create_dir_all(&data_dir).unwrap();
    let data_root = data_dir.join("Quarantine");
    // The home folder is where items live; the app's own data (and so the quarantine) is protected.
    let policy = SafetyPolicy::custom(vec![root.join("system")], vec![root.join("home")], vec![data_dir.clone()]);
    fs::create_dir_all(root.join("home/docs")).unwrap();
    World { _t: t, root, data_dir, data_root, db: Db::open_in_memory().unwrap(), policy }
}

fn now() -> chrono::DateTime<Local> {
    Local.with_ymd_and_hms(2024, 5, 6, 7, 8, 9).unwrap()
}

fn request(paths: &[&Path]) -> RemovalRequest {
    RemovalRequest { items: paths.iter().map(|p| RemovalItem { path: p.display().to_string(), size: 100 }).collect(), scan_root: None }
}

fn real_env(w: &World) -> Env<'static> {
    Env { data_root: w.data_root.clone(), data_dir: w.data_dir.clone(), volumes: &RealVolumes }
}

fn ino(p: &Path) -> u64 {
    fs::symlink_metadata(p).unwrap().ino()
}

#[test]
fn moves_by_rename_into_a_dated_folder_and_logs_it_everywhere() {
    let w = world();
    let file = w.root.join("home/docs/report.txt");
    fs::write(&file, "content").unwrap();
    let dir = w.root.join("home/docs/project");
    fs::create_dir_all(dir.join("src")).unwrap();
    fs::write(dir.join("src/a.rs"), "fn a() {}").unwrap();
    let (file_ino, dir_ino) = (ino(&file), ino(&dir));

    let out = move_into(&w.policy, &w.db, &request(&[&file, &dir]), &real_env(&w), &now());
    assert!(out.iter().all(|o| o.ok), "{out:?}");
    assert!(!file.exists() && !dir.exists());

    let items = w.db.quarantine_items().unwrap();
    assert_eq!(items.len(), 2);
    let batch = w.data_root.join("2024-05-06");
    for rec in &items {
        assert!(Path::new(&rec.dest).starts_with(&batch), "{}", rec.dest);
        assert!(rec.dest.contains(&format!("{}-", Path::new(&rec.dest).file_name().unwrap().to_string_lossy().split('-').next().unwrap())));
    }
    // A rename keeps the inode: nothing was copied.
    let moved = |name: &str| PathBuf::from(&items.iter().find(|r| r.origin.ends_with(name)).unwrap().dest);
    assert_eq!(ino(&moved("report.txt")), file_ino);
    assert_eq!(ino(&moved("project")), dir_ino);
    assert_eq!(fs::read_to_string(moved("project").join("src/a.rs")).unwrap(), "fn a() {}");

    // CSV: header plus one row per item, with the columns the user asked for.
    let csv = fs::read_to_string(batch.join("quarantine.csv")).unwrap();
    let lines: Vec<_> = csv.lines().collect();
    assert_eq!(lines[0], "id,origin,destination,size,date");
    assert_eq!(lines.len(), 3);
    let row = lines.iter().find(|l| l.contains("report.txt")).unwrap();
    assert!(row.contains(&file.display().to_string()) && row.contains(",100,2024-05-06T07:08:09"), "{row}");

    // The log records it, but it is not "space freed": nothing left the disk.
    assert_eq!(w.db.operations(10).unwrap().iter().filter(|o| o.action == "quarantine" && o.ok).count(), 2);
    assert!(w.db.metrics().unwrap().iter().all(|(k, _)| k != "bytes_reclaimed_total" && k != "items_removed_total"));
}

#[test]
fn a_symlink_moves_as_itself_and_never_drags_its_target() {
    let w = world();
    let target = w.root.join("home/docs/real.txt");
    fs::write(&target, "keep").unwrap();
    let link = w.root.join("home/docs/link");
    symlink(&target, &link).unwrap();
    let out = move_into(&w.policy, &w.db, &request(&[&link]), &real_env(&w), &now());
    assert!(out[0].ok, "{out:?}");
    assert!(target.exists() && fs::symlink_metadata(&link).is_err());
    let rec = &w.db.quarantine_items().unwrap()[0];
    assert!(fs::symlink_metadata(&rec.dest).unwrap().file_type().is_symlink());
}

#[test]
fn the_safety_policy_decides_exactly_as_for_the_trash() {
    let mut w = world();
    let protected = w.root.join("system/core");
    fs::create_dir_all(&protected).unwrap();
    fs::write(protected.join("kernel"), "k").unwrap();
    let ignored = w.root.join("home/docs/keep.txt");
    fs::write(&ignored, "k").unwrap();
    w.policy.set_ignored(vec![ignored.clone()]);
    let outside = w.root.join("outside.txt");
    fs::write(&outside, "o").unwrap();
    let mut req = request(&[&protected.join("kernel"), &ignored, &outside, &w.root.join("home/docs/missing.txt"), &w.root.join("home")]);
    req.scan_root = Some(w.root.join("home/docs").display().to_string());
    let out = move_into(&w.policy, &w.db, &req, &real_env(&w), &now());
    assert!(out.iter().all(|o| !o.ok), "{out:?}");
    assert!(protected.join("kernel").exists() && ignored.exists() && outside.exists());
    assert!(w.db.quarantine_items().unwrap().is_empty());
}

#[test]
fn nothing_inside_a_quarantine_can_be_removed_by_another_flow() {
    let w = world();
    let file = w.root.join("home/docs/a.txt");
    fs::write(&file, "x").unwrap();
    move_into(&w.policy, &w.db, &request(&[&file]), &real_env(&w), &now());
    let rec = &w.db.quarantine_items().unwrap()[0];
    let ctx = organizemymac_lib::filesystem::safety::RemovalContext::default();
    assert!(w.policy.check(Path::new(&rec.dest), &ctx).is_err(), "own files");
    // And also on another volume, where the folder is not under the app data folder.
    let other = w.root.join("volB");
    fs::create_dir_all(other.join(QUARANTINE_DIR).join("2024-05-06")).unwrap();
    fs::write(other.join(QUARANTINE_DIR).join("2024-05-06/x-file"), "x").unwrap();
    assert!(w.policy.check(&other.join(QUARANTINE_DIR).join("2024-05-06/x-file"), &ctx).is_err());
}

#[test]
fn another_volume_gets_its_own_quarantine_at_its_root_or_is_refused() {
    let w = world();
    let b = w.root.join("volB");
    fs::create_dir_all(b.join("files")).unwrap();
    let f = b.join("files/big.bin");
    fs::write(&f, "b").unwrap();
    let on_a = w.root.join("home/docs/a.txt");
    fs::write(&on_a, "a").unwrap();

    let vols = FakeVolumes { other: b.clone(), other_mount: b.clone() };
    let env = Env { data_root: w.data_root.clone(), data_dir: w.data_dir.clone(), volumes: &vols };
    let out = move_into(&w.policy, &w.db, &request(&[&f, &on_a]), &env, &now());
    assert!(out.iter().all(|o| o.ok), "{out:?}");
    let items = w.db.quarantine_items().unwrap();
    let dest_b = items.iter().find(|r| r.origin.ends_with("big.bin")).unwrap();
    let dest_a = items.iter().find(|r| r.origin.ends_with("a.txt")).unwrap();
    assert!(Path::new(&dest_b.dest).starts_with(b.join(QUARANTINE_DIR).join("2024-05-06")), "{}", dest_b.dest);
    assert!(Path::new(&dest_a.dest).starts_with(w.data_root.join("2024-05-06")));
    assert!(b.join(QUARANTINE_DIR).join("2024-05-06/quarantine.csv").exists(), "each volume keeps its own log");

    // A volume whose quarantine would land on the wrong device is refused and the item stays put.
    let g = b.join("files/other.bin");
    fs::write(&g, "g").unwrap();
    let wrong = FakeVolumes { other: b.clone(), other_mount: w.root.join("home") }; // mount point on device 1, item on 2
    let env = Env { data_root: w.data_root.clone(), data_dir: w.data_dir.clone(), volumes: &wrong };
    let out = move_into(&w.policy, &w.db, &request(&[&g]), &env, &now());
    assert!(!out[0].ok && out[0].error.as_deref().unwrap().contains("outro volume"), "{out:?}");
    assert!(g.exists(), "refused items are never moved or copied");
}

#[test]
fn same_names_never_collide_and_long_names_are_shortened() {
    let w = world();
    fs::create_dir_all(w.root.join("home/a")).unwrap();
    fs::create_dir_all(w.root.join("home/b")).unwrap();
    let (x, y) = (w.root.join("home/a/same.txt"), w.root.join("home/b/same.txt"));
    fs::write(&x, "x").unwrap();
    fs::write(&y, "y").unwrap();
    let long = w.root.join(format!("home/{}.txt", "ç".repeat(200)));
    fs::write(&long, "l").unwrap();
    let out = move_into(&w.policy, &w.db, &request(&[&x, &y, &long]), &real_env(&w), &now());
    assert!(out.iter().all(|o| o.ok), "{out:?}");
    let dests: Vec<String> = w.db.quarantine_items().unwrap().into_iter().map(|r| r.dest).collect();
    assert_eq!(dests.iter().collect::<std::collections::HashSet<_>>().len(), 3);
    assert!(dests.iter().all(|d| Path::new(d).file_name().unwrap().to_string_lossy().chars().count() <= 100 + 9));
}

#[test]
fn csv_quotes_awkward_names() {
    let w = world();
    let f = w.root.join("home/docs/a, \"b\".txt");
    fs::write(&f, "x").unwrap();
    move_into(&w.policy, &w.db, &request(&[&f]), &real_env(&w), &now());
    let csv = fs::read_to_string(w.data_root.join("2024-05-06/quarantine.csv")).unwrap();
    assert!(csv.contains("\"a, \"\"b\"\".txt\"") || csv.contains("a, \"\"b\"\".txt\""), "{csv}");
    assert_eq!(csv.lines().count(), 2);
}

#[test]
fn a_failed_record_undoes_the_move() {
    let w = world();
    let f = w.root.join("home/docs/a.txt");
    fs::write(&f, "x").unwrap();
    // The dated folder exists but cannot be written to, so the CSV cannot be appended.
    let dated = w.data_root.join("2024-05-06");
    fs::create_dir_all(&dated).unwrap();
    fs::create_dir(dated.join("quarantine.csv")).unwrap(); // a directory where the log file should be
    let out = move_into(&w.policy, &w.db, &request(&[&f]), &real_env(&w), &now());
    assert!(!out[0].ok && out[0].error.as_deref().unwrap().contains("registrar"), "{out:?}");
    assert!(f.exists(), "nothing is ever moved without a record");
    assert_eq!(fs::read_to_string(&f).unwrap(), "x");
}

#[test]
fn restore_puts_the_item_back_without_overwriting_anything() {
    let w = world();
    let f = w.root.join("home/docs/a.txt");
    fs::write(&f, "original").unwrap();
    let before = ino(&f);
    move_into(&w.policy, &w.db, &request(&[&f]), &real_env(&w), &now());
    let id = w.db.quarantine_items().unwrap()[0].id.clone();

    // Something new took its place meanwhile: it is left alone, and the item stays in quarantine.
    fs::write(&f, "newer").unwrap();
    let out = restore(&w.policy, &w.db, &[id.clone()], None);
    assert!(!out[0].ok && out[0].error.as_deref().unwrap().contains("já existe"), "{out:?}");
    assert_eq!(fs::read_to_string(&f).unwrap(), "newer");
    assert_eq!(w.db.quarantine_items().unwrap().len(), 1);

    // Restoring somewhere else works, with the original name.
    let elsewhere = w.root.join("home/restored");
    fs::create_dir_all(&elsewhere).unwrap();
    let out = restore(&w.policy, &w.db, &[id.clone()], Some(&elsewhere));
    assert!(out[0].ok, "{out:?}");
    assert_eq!(fs::read_to_string(elsewhere.join("a.txt")).unwrap(), "original");
    assert_eq!(ino(&elsewhere.join("a.txt")), before, "moved back, not copied");
    assert!(w.db.quarantine_items().unwrap().is_empty());
    // Restoring twice is refused.
    assert!(!restore(&w.policy, &w.db, &[id], None)[0].ok);
}

#[test]
fn restore_to_the_original_place_when_it_is_free() {
    let w = world();
    let f = w.root.join("home/docs/a.txt");
    fs::write(&f, "original").unwrap();
    move_into(&w.policy, &w.db, &request(&[&f]), &real_env(&w), &now());
    let id = w.db.quarantine_items().unwrap()[0].id.clone();
    assert!(restore(&w.policy, &w.db, &[id], None)[0].ok);
    assert_eq!(fs::read_to_string(&f).unwrap(), "original");
}

#[test]
fn restore_needs_a_usable_place_and_never_trusts_the_database() {
    let w = world();
    let sub = w.root.join("home/gone-soon");
    fs::create_dir_all(&sub).unwrap();
    let f = sub.join("a.txt");
    fs::write(&f, "x").unwrap();
    move_into(&w.policy, &w.db, &request(&[&f]), &real_env(&w), &now());
    let id = w.db.quarantine_items().unwrap()[0].id.clone();
    fs::remove_dir(&sub).unwrap();
    let out = restore(&w.policy, &w.db, &[id], None);
    assert!(!out[0].ok && out[0].error.as_deref().unwrap().contains("não existe mais"), "{out:?}");
    assert!(!sub.exists(), "the folder is not recreated behind the user's back");

    // A record pointing outside the quarantine is refused for restore and for emptying.
    let outside = w.root.join("home/precious.txt");
    fs::write(&outside, "precious").unwrap();
    w.db.insert_quarantine_batch("bx", w.root.join("home").to_str().unwrap()).unwrap();
    w.db.insert_quarantine_item("evil", "bx", w.root.join("home/docs/x").to_str().unwrap(), outside.to_str().unwrap(), 1).unwrap();
    assert!(!restore(&w.policy, &w.db, &["evil".into()], None)[0].ok);
    let called = Mutex::new(Vec::new());
    let out = empty(&w.db, &["evil".into()], &|p| { called.lock().unwrap().push(p.to_path_buf()); Ok(()) });
    assert!(!out[0].ok);
    assert!(called.lock().unwrap().is_empty(), "the Trash was never asked about a path outside the quarantine");
    assert!(outside.exists());
}

#[test]
fn emptying_sends_to_the_trash_and_never_deletes() {
    let w = world();
    let (a, b) = (w.root.join("home/docs/a.txt"), w.root.join("home/docs/b.txt"));
    fs::write(&a, "a").unwrap();
    fs::write(&b, "b").unwrap();
    move_into(&w.policy, &w.db, &request(&[&a, &b]), &real_env(&w), &now());
    let items = w.db.quarantine_items().unwrap();
    let ids: Vec<String> = items.iter().map(|r| r.id.clone()).collect();

    let trashed = Mutex::new(Vec::<PathBuf>::new());
    let out = empty(&w.db, &ids[..1], &|p| { trashed.lock().unwrap().push(p.to_path_buf()); Ok(()) });
    assert!(out[0].ok);
    assert_eq!(trashed.lock().unwrap().len(), 1);
    assert_eq!(w.db.quarantine_items().unwrap().len(), 1);
    // The files are still on disk: the injected "Trash" did not delete anything, and neither did we.
    assert!(items.iter().all(|r| Path::new(&r.dest).exists()));
    assert_eq!(w.db.metrics().unwrap().iter().find(|(k, _)| k == "items_removed_total").map(|(_, v)| *v), Some(1));

    // A Trash that refuses leaves the item in quarantine.
    let out = empty(&w.db, &ids[1..], &|_| Err("a Lixeira recusou".into()));
    assert!(!out[0].ok && out[0].error.as_deref() == Some("a Lixeira recusou"));
    assert_eq!(w.db.quarantine_items().unwrap().len(), 1);

    // Already gone from the folder: the record is just closed, the Trash is not asked.
    let rec = &w.db.quarantine_items().unwrap()[0];
    fs::remove_file(&rec.dest).unwrap();
    let asked = Mutex::new(0);
    let out = empty(&w.db, &ids[1..], &|_| { *asked.lock().unwrap() += 1; Ok(()) });
    assert!(out[0].ok);
    assert_eq!(*asked.lock().unwrap(), 0);
    assert!(w.db.quarantine_items().unwrap().is_empty());
}

#[test]
fn the_real_volume_lookup_agrees_with_itself() {
    let w = world();
    let f = w.root.join("home/docs/a.txt");
    fs::write(&f, "x").unwrap();
    let v = RealVolumes;
    assert_eq!(v.device(&f).unwrap(), v.device(&w.data_dir).unwrap());
    // On the data volume the mount point (/System/Volumes/Data) is not a text prefix of the path the
    // user sees (firmlinks), so what matters is that it is the same device. External volumes mount
    // under /Volumes, where the prefix does hold.
    let mount = v.mount_point(&f).unwrap();
    assert!(mount.is_dir());
    assert_eq!(v.device(&mount).unwrap(), v.device(&f).unwrap(), "{mount:?}");
    // A symlink is looked up as itself, not as what it points to.
    let link = w.root.join("home/docs/l");
    symlink("/", &link).unwrap();
    assert_eq!(v.device(&link).unwrap(), v.device(&f).unwrap());
    assert_eq!(v.device(&v.mount_point(&link).unwrap()).unwrap(), v.device(&link).unwrap());
}

#[test]
fn the_mode_defaults_to_the_trash() {
    assert_eq!(RemovalMode::from_setting(None), RemovalMode::Trash);
    assert_eq!(RemovalMode::from_setting(Some("trash")), RemovalMode::Trash);
    assert_eq!(RemovalMode::from_setting(Some("garbage")), RemovalMode::Trash);
    assert_eq!(RemovalMode::from_setting(Some("quarantine")), RemovalMode::Quarantine);
    assert_eq!(RemovalMode::Quarantine.as_str(), "quarantine");
}
