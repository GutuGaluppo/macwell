//! Local SQLite database: scan history, operation log, ignore list, settings and local-only metrics.
//! Nothing here ever leaves the Mac.

use std::path::Path;

use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};

use crate::error::AppResult;

const MIGRATIONS: &[&str] = &[
    // v1
    "CREATE TABLE scans (
        id TEXT PRIMARY KEY,
        module TEXT NOT NULL,
        root TEXT NOT NULL,
        started_at INTEGER NOT NULL,
        finished_at INTEGER,
        files INTEGER NOT NULL DEFAULT 0,
        directories INTEGER NOT NULL DEFAULT 0,
        bytes_scanned INTEGER NOT NULL DEFAULT 0,
        bytes_allocated INTEGER NOT NULL DEFAULT 0,
        reclaimable_bytes INTEGER NOT NULL DEFAULT 0,
        warnings INTEGER NOT NULL DEFAULT 0,
        duration_ms INTEGER NOT NULL DEFAULT 0,
        cancelled INTEGER NOT NULL DEFAULT 0
     );
     CREATE INDEX scans_started ON scans(started_at DESC);
     CREATE TABLE operations (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        ts INTEGER NOT NULL,
        action TEXT NOT NULL,
        path TEXT NOT NULL,
        size INTEGER NOT NULL DEFAULT 0,
        ok INTEGER NOT NULL,
        detail TEXT
     );
     CREATE INDEX operations_ts ON operations(ts DESC);
     CREATE TABLE ignore_list (
        path TEXT PRIMARY KEY,
        added_at INTEGER NOT NULL,
        reason TEXT
     );
     CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
     CREATE TABLE metrics (key TEXT PRIMARY KEY, value INTEGER NOT NULL);",
    // v2: developer care (M12), backup guard (M13) and quarantine (M14)
    "CREATE TABLE dev_roots (
        path TEXT PRIMARY KEY,
        added_at INTEGER NOT NULL
     );
     CREATE TABLE dev_repos (
        path TEXT PRIMARY KEY,
        last_scan INTEGER,
        remote_visibility TEXT,
        visibility_checked_at INTEGER
     );
     CREATE TABLE dev_snapshots (
        id TEXT PRIMARY KEY,
        repo TEXT NOT NULL,
        ref_name TEXT NOT NULL,
        commit_id TEXT NOT NULL,
        created_at INTEGER NOT NULL,
        pushed_at INTEGER
     );
     CREATE INDEX dev_snapshots_repo ON dev_snapshots(repo, created_at DESC);
     CREATE TABLE tm_exclusions (
        path TEXT PRIMARY KEY,
        added_at INTEGER NOT NULL,
        source_artifact_kind TEXT,
        removed_at INTEGER
     );
     CREATE TABLE quarantine_batches (
        id TEXT PRIMARY KEY,
        dir TEXT NOT NULL,
        created_at INTEGER NOT NULL
     );
     CREATE TABLE quarantine_items (
        id TEXT PRIMARY KEY,
        batch_id TEXT NOT NULL REFERENCES quarantine_batches(id),
        origin TEXT NOT NULL,
        dest TEXT NOT NULL,
        size INTEGER NOT NULL DEFAULT 0,
        quarantined_at INTEGER NOT NULL,
        restored_at INTEGER,
        emptied_at INTEGER
     );
     CREATE INDEX quarantine_items_batch ON quarantine_items(batch_id);",
    // v3: remember which remote URL a cached visibility answer was about
    "ALTER TABLE dev_repos ADD COLUMN remote_url TEXT;",
];

pub struct Db {
    conn: Connection,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanRecord {
    pub id: String,
    pub module: String,
    pub root: String,
    pub started_at: i64,
    pub finished_at: Option<i64>,
    pub files: u64,
    pub directories: u64,
    pub bytes_scanned: u64,
    pub bytes_allocated: u64,
    pub reclaimable_bytes: u64,
    pub warnings: u64,
    pub duration_ms: u64,
    pub cancelled: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OperationRecord {
    pub id: i64,
    pub ts: i64,
    pub action: String,
    pub path: String,
    pub size: u64,
    pub ok: bool,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IgnoreEntry {
    pub path: String,
    pub added_at: i64,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotRecord {
    pub id: String,
    pub repo: String,
    pub ref_name: String,
    pub commit_id: String,
    pub created_at: i64,
    pub pushed_at: Option<i64>,
}

/// A remote visibility answer the user asked for earlier. Only valid for the URL it was about.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CachedVisibility {
    pub visibility: String,
    pub remote_url: String,
    pub checked_at: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevRoot {
    pub path: String,
    pub added_at: i64,
}

/// A Time Machine exclusion this app added (and can therefore undo).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TmExclusionRecord {
    pub path: String,
    pub added_at: i64,
    pub kind: Option<String>,
}

/// An item currently in quarantine.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuarantineRecord {
    pub id: String,
    pub batch_id: String,
    /// The dated folder holding the item and `quarantine.csv`.
    pub batch_dir: String,
    pub origin: String,
    pub dest: String,
    pub size: u64,
    pub quarantined_at: i64,
}

pub fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

impl Db {
    pub fn open(path: &Path) -> AppResult<Self> {
        let conn = Connection::open(path)?;
        Self::init(conn)
    }

    pub fn open_in_memory() -> AppResult<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(mut conn: Connection) -> AppResult<Self> {
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
        for (i, sql) in MIGRATIONS.iter().enumerate().skip(version as usize) {
            let tx = conn.transaction()?;
            tx.execute_batch(sql)?;
            tx.pragma_update(None, "user_version", (i + 1) as i64)?;
            tx.commit()?;
        }
        Ok(Db { conn })
    }

    pub fn insert_scan(&self, r: &ScanRecord) -> AppResult<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO scans (id, module, root, started_at, finished_at, files, directories,
                bytes_scanned, bytes_allocated, reclaimable_bytes, warnings, duration_ms, cancelled)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                r.id, r.module, r.root, r.started_at, r.finished_at, r.files as i64, r.directories as i64,
                r.bytes_scanned as i64, r.bytes_allocated as i64, r.reclaimable_bytes as i64,
                r.warnings as i64, r.duration_ms as i64, r.cancelled
            ],
        )?;
        self.bump_metric("scans_total", 1)?;
        self.bump_metric("files_scanned_total", r.files as i64)?;
        self.bump_metric("bytes_scanned_total", r.bytes_scanned as i64)?;
        Ok(())
    }

    pub fn recent_scans(&self, module: Option<&str>, limit: u32) -> AppResult<Vec<ScanRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, module, root, started_at, finished_at, files, directories, bytes_scanned,
                    bytes_allocated, reclaimable_bytes, warnings, duration_ms, cancelled
             FROM scans WHERE (?1 IS NULL OR module = ?1) ORDER BY started_at DESC LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![module, limit], |r| {
            Ok(ScanRecord {
                id: r.get(0)?,
                module: r.get(1)?,
                root: r.get(2)?,
                started_at: r.get(3)?,
                finished_at: r.get(4)?,
                files: r.get::<_, i64>(5)? as u64,
                directories: r.get::<_, i64>(6)? as u64,
                bytes_scanned: r.get::<_, i64>(7)? as u64,
                bytes_allocated: r.get::<_, i64>(8)? as u64,
                reclaimable_bytes: r.get::<_, i64>(9)? as u64,
                warnings: r.get::<_, i64>(10)? as u64,
                duration_ms: r.get::<_, i64>(11)? as u64,
                cancelled: r.get(12)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn log_operation(&self, action: &str, path: &str, size: u64, ok: bool, detail: Option<&str>) -> AppResult<()> {
        self.conn.execute(
            "INSERT INTO operations (ts, action, path, size, ok, detail) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![now_ms(), action, path, size as i64, ok, detail],
        )?;
        if ok {
            self.bump_metric("bytes_reclaimed_total", size as i64)?;
            self.bump_metric("items_removed_total", 1)?;
        } else {
            self.bump_metric("operation_errors_total", 1)?;
        }
        Ok(())
    }

    pub fn operations(&self, limit: u32) -> AppResult<Vec<OperationRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, ts, action, path, size, ok, detail FROM operations ORDER BY id DESC LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit], |r| {
            Ok(OperationRecord {
                id: r.get(0)?,
                ts: r.get(1)?,
                action: r.get(2)?,
                path: r.get(3)?,
                size: r.get::<_, i64>(4)? as u64,
                ok: r.get(5)?,
                detail: r.get(6)?,
            })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn ignore_list(&self) -> AppResult<Vec<IgnoreEntry>> {
        let mut stmt = self.conn.prepare("SELECT path, added_at, reason FROM ignore_list ORDER BY path")?;
        let rows = stmt.query_map([], |r| Ok(IgnoreEntry { path: r.get(0)?, added_at: r.get(1)?, reason: r.get(2)? }))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn add_ignore(&self, path: &str, reason: Option<&str>) -> AppResult<()> {
        self.conn.execute(
            "INSERT OR REPLACE INTO ignore_list (path, added_at, reason) VALUES (?1, ?2, ?3)",
            params![path, now_ms(), reason],
        )?;
        Ok(())
    }

    pub fn remove_ignore(&self, path: &str) -> AppResult<()> {
        self.conn.execute("DELETE FROM ignore_list WHERE path = ?1", params![path])?;
        Ok(())
    }

    /// Records an action that is not a removal (snapshot, push…) in the operation log without
    /// touching the "space reclaimed" metrics that `log_operation` keeps.
    pub fn log_action(&self, action: &str, path: &str, ok: bool, detail: Option<&str>) -> AppResult<()> {
        self.conn.execute(
            "INSERT INTO operations (ts, action, path, size, ok, detail) VALUES (?1, ?2, ?3, 0, ?4, ?5)",
            params![now_ms(), action, path, ok, detail],
        )?;
        Ok(())
    }

    pub fn dev_roots(&self) -> AppResult<Vec<DevRoot>> {
        let mut stmt = self.conn.prepare("SELECT path, added_at FROM dev_roots ORDER BY path")?;
        let rows = stmt.query_map([], |r| Ok(DevRoot { path: r.get(0)?, added_at: r.get(1)? }))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn add_dev_root(&self, path: &str) -> AppResult<()> {
        self.conn.execute("INSERT OR IGNORE INTO dev_roots (path, added_at) VALUES (?1, ?2)", params![path, now_ms()])?;
        Ok(())
    }

    pub fn remove_dev_root(&self, path: &str) -> AppResult<()> {
        self.conn.execute("DELETE FROM dev_roots WHERE path = ?1", params![path])?;
        Ok(())
    }

    pub fn insert_quarantine_batch(&self, id: &str, dir: &str) -> AppResult<()> {
        self.conn.execute("INSERT INTO quarantine_batches (id, dir, created_at) VALUES (?1, ?2, ?3)", params![id, dir, now_ms()])?;
        Ok(())
    }

    pub fn insert_quarantine_item(&self, id: &str, batch_id: &str, origin: &str, dest: &str, size: u64) -> AppResult<()> {
        self.conn.execute(
            "INSERT INTO quarantine_items (id, batch_id, origin, dest, size, quarantined_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, batch_id, origin, dest, size as i64, now_ms()],
        )?;
        Ok(())
    }

    fn quarantine_query(&self, filter: &str, args: &[&dyn rusqlite::ToSql]) -> AppResult<Vec<QuarantineRecord>> {
        let sql = format!(
            "SELECT i.id, i.batch_id, b.dir, i.origin, i.dest, i.size, i.quarantined_at
             FROM quarantine_items i JOIN quarantine_batches b ON b.id = i.batch_id
             WHERE i.restored_at IS NULL AND i.emptied_at IS NULL {filter} ORDER BY i.quarantined_at DESC, i.origin"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(args, |r| {
            Ok(QuarantineRecord { id: r.get(0)?, batch_id: r.get(1)?, batch_dir: r.get(2)?, origin: r.get(3)?, dest: r.get(4)?, size: r.get::<_, i64>(5)? as u64, quarantined_at: r.get(6)? })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    /// Items that are still in quarantine (not restored, not sent to the Trash).
    pub fn quarantine_items(&self) -> AppResult<Vec<QuarantineRecord>> {
        self.quarantine_query("", &[])
    }

    pub fn quarantine_item(&self, id: &str) -> AppResult<Option<QuarantineRecord>> {
        Ok(self.quarantine_query("AND i.id = ?1", &[&id])?.into_iter().next())
    }

    pub fn mark_quarantine_restored(&self, id: &str) -> AppResult<()> {
        self.conn.execute("UPDATE quarantine_items SET restored_at = ?2 WHERE id = ?1", params![id, now_ms()])?;
        Ok(())
    }

    pub fn mark_quarantine_emptied(&self, id: &str) -> AppResult<()> {
        self.conn.execute("UPDATE quarantine_items SET emptied_at = ?2 WHERE id = ?1", params![id, now_ms()])?;
        Ok(())
    }

    pub fn record_tm_exclusion(&self, path: &str, kind: &str) -> AppResult<()> {
        self.conn.execute(
            "INSERT INTO tm_exclusions (path, added_at, source_artifact_kind, removed_at) VALUES (?1, ?2, ?3, NULL)
             ON CONFLICT(path) DO UPDATE SET added_at = ?2, source_artifact_kind = ?3, removed_at = NULL",
            params![path, now_ms(), kind],
        )?;
        Ok(())
    }

    /// Exclusions this app added and has not undone.
    pub fn tm_exclusions(&self) -> AppResult<Vec<TmExclusionRecord>> {
        let mut stmt = self.conn.prepare("SELECT path, added_at, source_artifact_kind FROM tm_exclusions WHERE removed_at IS NULL ORDER BY path")?;
        let rows = stmt.query_map([], |r| Ok(TmExclusionRecord { path: r.get(0)?, added_at: r.get(1)?, kind: r.get(2)? }))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn mark_tm_exclusion_removed(&self, path: &str) -> AppResult<()> {
        self.conn.execute("UPDATE tm_exclusions SET removed_at = ?2 WHERE path = ?1 AND removed_at IS NULL", params![path, now_ms()])?;
        Ok(())
    }

    pub fn mark_snapshot_pushed_by_ref(&self, repo: &str, ref_name: &str) -> AppResult<()> {
        self.conn.execute(
            "UPDATE dev_snapshots SET pushed_at = ?3 WHERE repo = ?1 AND ref_name = ?2",
            params![repo, ref_name, now_ms()],
        )?;
        Ok(())
    }

    pub fn record_snapshot(&self, repo: &str, ref_name: &str, commit_id: &str) -> AppResult<String> {
        let id = uuid::Uuid::new_v4().to_string();
        self.conn.execute(
            "INSERT INTO dev_snapshots (id, repo, ref_name, commit_id, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![id, repo, ref_name, commit_id, now_ms()],
        )?;
        Ok(id)
    }

    pub fn snapshots(&self, repo: Option<&str>) -> AppResult<Vec<SnapshotRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, repo, ref_name, commit_id, created_at, pushed_at FROM dev_snapshots
             WHERE (?1 IS NULL OR repo = ?1) ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map(params![repo], |r| {
            Ok(SnapshotRecord { id: r.get(0)?, repo: r.get(1)?, ref_name: r.get(2)?, commit_id: r.get(3)?, created_at: r.get(4)?, pushed_at: r.get(5)? })
        })?;
        Ok(rows.collect::<Result<_, _>>()?)
    }

    pub fn set_visibility(&self, repo: &str, remote_url: &str, visibility: &str) -> AppResult<()> {
        self.conn.execute(
            "INSERT INTO dev_repos (path, remote_visibility, visibility_checked_at, remote_url) VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(path) DO UPDATE SET remote_visibility = ?2, visibility_checked_at = ?3, remote_url = ?4",
            params![repo, visibility, now_ms(), remote_url],
        )?;
        Ok(())
    }

    /// The cached answer for `repo`, only if it was about `remote_url`.
    pub fn visibility(&self, repo: &str, remote_url: &str) -> AppResult<Option<CachedVisibility>> {
        Ok(self
            .conn
            .query_row(
                "SELECT remote_visibility, remote_url, visibility_checked_at FROM dev_repos
                 WHERE path = ?1 AND remote_url = ?2 AND remote_visibility IS NOT NULL",
                params![repo, remote_url],
                |r| Ok(CachedVisibility { visibility: r.get(0)?, remote_url: r.get(1)?, checked_at: r.get(2)? }),
            )
            .optional()?)
    }

    pub fn mark_snapshot_pushed(&self, id: &str) -> AppResult<()> {
        self.conn.execute("UPDATE dev_snapshots SET pushed_at = ?2 WHERE id = ?1", params![id, now_ms()])?;
        Ok(())
    }

    pub fn setting(&self, key: &str) -> AppResult<Option<String>> {
        Ok(self.conn.query_row("SELECT value FROM settings WHERE key = ?1", params![key], |r| r.get(0)).optional()?)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> AppResult<()> {
        self.conn.execute("INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)", params![key, value])?;
        Ok(())
    }

    pub fn bump_metric(&self, key: &str, by: i64) -> AppResult<()> {
        self.conn.execute(
            "INSERT INTO metrics (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = value + ?2",
            params![key, by],
        )?;
        Ok(())
    }

    pub fn metrics(&self) -> AppResult<Vec<(String, i64)>> {
        let mut stmt = self.conn.prepare("SELECT key, value FROM metrics ORDER BY key")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<Result<_, _>>()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_and_roundtrip() {
        let db = Db::open_in_memory().unwrap();
        db.insert_scan(&ScanRecord {
            id: "a".into(), module: "scanner".into(), root: "/tmp".into(), started_at: 1, finished_at: Some(2),
            files: 10, directories: 2, bytes_scanned: 100, bytes_allocated: 120, reclaimable_bytes: 0,
            warnings: 0, duration_ms: 5, cancelled: false,
        }).unwrap();
        assert_eq!(db.recent_scans(Some("scanner"), 10).unwrap().len(), 1);
        assert!(db.recent_scans(Some("other"), 10).unwrap().is_empty());
        db.log_operation("trash", "/x", 50, true, None).unwrap();
        db.log_operation("trash", "/y", 10, false, Some("denied")).unwrap();
        assert_eq!(db.operations(10).unwrap().len(), 2);
        let metrics: std::collections::HashMap<_, _> = db.metrics().unwrap().into_iter().collect();
        assert_eq!(metrics["bytes_reclaimed_total"], 50);
        assert_eq!(metrics["files_scanned_total"], 10);
        db.add_ignore("/x", Some("keep")).unwrap();
        assert_eq!(db.ignore_list().unwrap()[0].path, "/x");
        db.remove_ignore("/x").unwrap();
        assert!(db.ignore_list().unwrap().is_empty());
        db.set_setting("k", "v").unwrap();
        assert_eq!(db.setting("k").unwrap().as_deref(), Some("v"));
    }

    #[test]
    fn v2_tables_exist_and_user_version_matches() {
        let db = Db::open_in_memory().unwrap();
        let version: i64 = db.conn.pragma_query_value(None, "user_version", |r| r.get(0)).unwrap();
        assert_eq!(version as usize, MIGRATIONS.len());
        for table in ["dev_roots", "dev_repos", "dev_snapshots", "tm_exclusions", "quarantine_batches", "quarantine_items"] {
            let n: i64 = db
                .conn
                .query_row("SELECT count(*) FROM sqlite_master WHERE type='table' AND name=?1", params![table], |r| r.get(0))
                .unwrap();
            assert_eq!(n, 1, "{table}");
        }
    }

    #[test]
    fn snapshots_are_recorded_and_marked_pushed() {
        let db = Db::open_in_memory().unwrap();
        let a = db.record_snapshot("/r", "wip/snapshot-2024-01-01", "abc").unwrap();
        db.record_snapshot("/other", "wip/snapshot-2024-01-02", "def").unwrap();
        assert_eq!(db.snapshots(Some("/r")).unwrap().len(), 1);
        assert_eq!(db.snapshots(None).unwrap().len(), 2);
        assert!(db.snapshots(Some("/r")).unwrap()[0].pushed_at.is_none());
        db.mark_snapshot_pushed(&a).unwrap();
        assert!(db.snapshots(Some("/r")).unwrap()[0].pushed_at.is_some());
    }

    #[test]
    fn visibility_cache_is_only_valid_for_the_same_remote_url() {
        let db = Db::open_in_memory().unwrap();
        assert!(db.visibility("/r", "git@github.com:me/x.git").unwrap().is_none());
        db.set_visibility("/r", "git@github.com:me/x.git", "public").unwrap();
        let hit = db.visibility("/r", "git@github.com:me/x.git").unwrap().unwrap();
        assert_eq!(hit.visibility, "public");
        assert!(db.visibility("/r", "git@github.com:me/other.git").unwrap().is_none());
        db.set_visibility("/r", "git@github.com:me/other.git", "privateOrMissing").unwrap();
        assert_eq!(db.visibility("/r", "git@github.com:me/other.git").unwrap().unwrap().visibility, "privateOrMissing");
        assert!(db.visibility("/r", "git@github.com:me/x.git").unwrap().is_none());
    }

    #[test]
    fn dev_roots_and_non_removal_actions() {
        let db = Db::open_in_memory().unwrap();
        db.add_dev_root("/Users/me/Projects").unwrap();
        db.add_dev_root("/Users/me/Projects").unwrap(); // idempotent
        db.add_dev_root("/Users/me/Work").unwrap();
        assert_eq!(db.dev_roots().unwrap().iter().map(|r| r.path.as_str()).collect::<Vec<_>>(), ["/Users/me/Projects", "/Users/me/Work"]);
        db.remove_dev_root("/Users/me/Work").unwrap();
        assert_eq!(db.dev_roots().unwrap().len(), 1);

        // A snapshot is logged, but it must not count as space reclaimed or items removed.
        db.log_action("snapshot", "/Users/me/Projects/app", true, Some("wip/snapshot-2024-05-06")).unwrap();
        assert_eq!(db.operations(10).unwrap()[0].action, "snapshot");
        let metrics: std::collections::HashMap<_, _> = db.metrics().unwrap().into_iter().collect();
        assert!(!metrics.contains_key("items_removed_total") && !metrics.contains_key("bytes_reclaimed_total"));

        let id = db.record_snapshot("/r", "wip/snapshot-2024-05-06", "abc").unwrap();
        db.mark_snapshot_pushed_by_ref("/r", "wip/snapshot-2024-05-06").unwrap();
        let snap = db.snapshots(Some("/r")).unwrap();
        assert_eq!(snap[0].id, id);
        assert!(snap[0].pushed_at.is_some());
    }

    #[test]
    fn tm_exclusions_are_recorded_and_undone() {
        let db = Db::open_in_memory().unwrap();
        db.record_tm_exclusion("/p/a/node_modules", "node-modules").unwrap();
        db.record_tm_exclusion("/p/b/target", "rust-target").unwrap();
        assert_eq!(db.tm_exclusions().unwrap().len(), 2);
        db.mark_tm_exclusion_removed("/p/a/node_modules").unwrap();
        let left = db.tm_exclusions().unwrap();
        assert_eq!((left.len(), left[0].path.as_str(), left[0].kind.as_deref()), (1, "/p/b/target", Some("rust-target")));
        // Excluding the same folder again after an undo makes it active again.
        db.record_tm_exclusion("/p/a/node_modules", "node-modules").unwrap();
        assert_eq!(db.tm_exclusions().unwrap().len(), 2);
        // Undoing something the app never added changes nothing.
        db.mark_tm_exclusion_removed("/somewhere/else").unwrap();
        assert_eq!(db.tm_exclusions().unwrap().len(), 2);
    }

    #[test]
    fn quarantine_items_move_through_their_states() {
        let db = Db::open_in_memory().unwrap();
        db.insert_quarantine_batch("b1", "/q/2024-05-06").unwrap();
        db.insert_quarantine_item("i1", "b1", "/Users/me/a.txt", "/q/2024-05-06/i1-a.txt", 10).unwrap();
        db.insert_quarantine_item("i2", "b1", "/Users/me/b.txt", "/q/2024-05-06/i2-b.txt", 20).unwrap();
        db.insert_quarantine_item("i3", "b1", "/Users/me/c.txt", "/q/2024-05-06/i3-c.txt", 30).unwrap();
        assert_eq!(db.quarantine_items().unwrap().len(), 3);
        let one = db.quarantine_item("i2").unwrap().unwrap();
        assert_eq!((one.origin.as_str(), one.batch_dir.as_str(), one.size), ("/Users/me/b.txt", "/q/2024-05-06", 20));
        db.mark_quarantine_restored("i1").unwrap();
        db.mark_quarantine_emptied("i2").unwrap();
        let left = db.quarantine_items().unwrap();
        assert_eq!(left.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), ["i3"]);
        assert!(db.quarantine_item("i1").unwrap().is_none() && db.quarantine_item("i2").unwrap().is_none());
        assert!(db.quarantine_item("nope").unwrap().is_none());
    }

    #[test]
    fn upgrades_a_v1_database_in_place() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("old.sqlite");
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(MIGRATIONS[0]).unwrap();
            conn.pragma_update(None, "user_version", 1i64).unwrap();
            conn.execute("INSERT INTO settings (key, value) VALUES ('k', 'kept')", []).unwrap();
        }
        let db = Db::open(&path).unwrap();
        assert_eq!(db.setting("k").unwrap().as_deref(), Some("kept"));
        let version: i64 = db.conn.pragma_query_value(None, "user_version", |r| r.get(0)).unwrap();
        assert_eq!(version as usize, MIGRATIONS.len());
    }
}
