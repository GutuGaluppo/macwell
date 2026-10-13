//! Quarantine: an optional alternative to the Trash. Items are moved, never copied, into a dated
//! folder on the same volume, listed in a CSV log, and can be restored or sent on to the Trash.
//!
//! * Moving is `rename(2)` on one volume only. An item on another volume is refused (the Trash is the
//!   answer there); there is no copy followed by delete.
//! * Every item is validated by the safety policy before it moves, like for the Trash. The quarantine
//!   folders themselves are the app's own: no other flow can remove or change what is inside.
//! * Restoring never overwrites: it uses an exclusive rename, so even a file that appears between the
//!   check and the move is left alone.
//! * "Emptying" sends the item to the Trash. The app never deletes anything permanently on its own.

use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

use crate::cleanup::RemovalRequest;
use crate::db::{Db, QuarantineRecord};
use crate::filesystem::safety::{MutationKind, RemovalContext, SafetyPolicy, QUARANTINE_DIR};
use crate::types::OperationOutcome;

/// Setting key that chooses between the Trash and the quarantine.
pub const MODE_SETTING: &str = "removal.mode";
const CSV_NAME: &str = "quarantine.csv";
const MAX_NAME_CHARS: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RemovalMode {
    Trash,
    Quarantine,
}

impl RemovalMode {
    pub fn from_setting(value: Option<&str>) -> Self {
        if value == Some("quarantine") { RemovalMode::Quarantine } else { RemovalMode::Trash }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            RemovalMode::Trash => "trash",
            RemovalMode::Quarantine => "quarantine",
        }
    }
}

/// Which volume a path lives on, and where that volume is mounted.
pub trait Volumes {
    fn device(&self, path: &Path) -> std::io::Result<u64>;
    fn mount_point(&self, path: &Path) -> std::io::Result<PathBuf>;
}

pub struct RealVolumes;

impl Volumes for RealVolumes {
    fn device(&self, path: &Path) -> std::io::Result<u64> {
        Ok(fs::symlink_metadata(path)?.dev())
    }

    fn mount_point(&self, path: &Path) -> std::io::Result<PathBuf> {
        // statfs follows symlinks, so ask about the folder that holds the item.
        let dir = path.parent().unwrap_or(path);
        let c = CString::new(dir.as_os_str().as_bytes()).map_err(|_| std::io::Error::from(std::io::ErrorKind::InvalidInput))?;
        let mut st: libc::statfs = unsafe { std::mem::zeroed() };
        if unsafe { libc::statfs(c.as_ptr(), &mut st) } != 0 {
            return Err(std::io::Error::last_os_error());
        }
        let name = unsafe { CStr::from_ptr(st.f_mntonname.as_ptr()) };
        Ok(PathBuf::from(std::ffi::OsStr::from_bytes(name.to_bytes())))
    }
}

pub struct Env<'a> {
    /// Quarantine for the volume that holds the app's data (the home folder), inside the app data folder.
    pub data_root: PathBuf,
    pub data_dir: PathBuf,
    pub volumes: &'a dyn Volumes,
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) { format!("\"{}\"", s.replace('"', "\"\"")) } else { s.to_string() }
}

fn append_csv(dir: &Path, id: &str, origin: &str, dest: &str, size: u64, date: &str) -> std::io::Result<()> {
    let path = dir.join(CSV_NAME);
    let new = !path.exists();
    let mut f = OpenOptions::new().create(true).append(true).open(path)?;
    if new {
        writeln!(f, "id,origin,destination,size,date")?;
    }
    writeln!(f, "{},{},{},{},{}", csv_field(id), csv_field(origin), csv_field(dest), size, csv_field(date))
}

/// `rename` that fails instead of replacing an existing target.
#[cfg(target_os = "macos")]
pub(crate) fn rename_noreplace(from: &Path, to: &Path) -> std::io::Result<()> {
    let (f, t) = (CString::new(from.as_os_str().as_bytes())?, CString::new(to.as_os_str().as_bytes())?);
    if unsafe { libc::renamex_np(f.as_ptr(), t.as_ptr(), libc::RENAME_EXCL) } == 0 { Ok(()) } else { Err(std::io::Error::last_os_error()) }
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn rename_noreplace(from: &Path, to: &Path) -> std::io::Result<()> {
    if fs::symlink_metadata(to).is_ok() {
        return Err(std::io::Error::from(std::io::ErrorKind::AlreadyExists));
    }
    fs::rename(from, to)
}

fn io_text(e: &std::io::Error) -> String {
    match e.raw_os_error() {
        Some(18) => "o item e o destino estão em volumes diferentes".to_string(),
        Some(17) | Some(66) => "já existe algo nesse lugar".to_string(),
        _ => e.to_string(),
    }
}

fn short_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..8].to_string()
}

fn stored_name(path: &Path) -> String {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "item".into());
    name.chars().take(MAX_NAME_CHARS).collect()
}

/// Moves each item into quarantine after validating it. Items are independent, like for the Trash.
pub fn move_into(policy: &SafetyPolicy, db: &Db, request: &RemovalRequest, env: &Env, now: &DateTime<Local>) -> Vec<OperationOutcome> {
    let ctx = RemovalContext { scan_root: request.scan_root.as_ref().map(PathBuf::from), allow_system_library: false };
    let mut batches: HashMap<PathBuf, String> = HashMap::new();
    let mut items: Vec<_> = request.items.iter().collect();
    items.sort_by_key(|i| std::cmp::Reverse(i.path.matches('/').count()));
    items
        .into_iter()
        .map(|item| {
            let result = quarantine_one(policy, db, &ctx, env, now, &mut batches, &item.path, item.size);
            let (ok, error) = match result {
                Ok(()) => (true, None),
                Err(e) => (false, Some(e)),
            };
            if let Err(err) = db.log_action("quarantine", &item.path, ok, error.as_deref()) {
                tracing::warn!(%err, "could not write the operation log");
            }
            OperationOutcome { path: item.path.clone(), size: item.size, ok, error }
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn quarantine_one(policy: &SafetyPolicy, db: &Db, ctx: &RemovalContext, env: &Env, now: &DateTime<Local>, batches: &mut HashMap<PathBuf, String>, path: &str, size: u64) -> Result<(), String> {
    let resolved = policy.check(Path::new(path), ctx).map_err(|v| v.to_string())?;
    let io = |e: std::io::Error| io_text(&e);

    // Same volume or nothing: the quarantine lives next to the item, so moving is a rename.
    let item_dev = env.volumes.device(&resolved).map_err(io)?;
    let data_dev = env.volumes.device(&env.data_dir).map_err(io)?;
    let root = if item_dev == data_dev {
        env.data_root.clone()
    } else {
        env.volumes.mount_point(&resolved).map_err(io)?.join(QUARANTINE_DIR)
    };
    let dir = root.join(now.format("%Y-%m-%d").to_string());
    if dir.starts_with(&resolved) {
        return Err("a quarentena não pode ficar dentro do próprio item".into());
    }
    fs::create_dir_all(&dir).map_err(io)?;
    if env.volumes.device(&dir).map_err(io)? != item_dev {
        return Err("o item está em outro volume: use a Lixeira para ele".into());
    }

    let id = short_id();
    let dest = dir.join(format!("{id}-{}", stored_name(&resolved)));
    rename_noreplace(&resolved, &dest).map_err(|e| io_text(&e))?;

    // Log it; if the record cannot be written the move is undone, so nothing is ever moved unrecorded.
    let record = (|| -> Result<(), String> {
        let batch = match batches.get(&dir) {
            Some(b) => b.clone(),
            None => {
                let b = uuid::Uuid::new_v4().to_string();
                db.insert_quarantine_batch(&b, &dir.to_string_lossy()).map_err(|e| e.to_string())?;
                batches.insert(dir.clone(), b.clone());
                b
            }
        };
        db.insert_quarantine_item(&id, &batch, &resolved.to_string_lossy(), &dest.to_string_lossy(), size).map_err(|e| e.to_string())?;
        append_csv(&dir, &id, &resolved.to_string_lossy(), &dest.to_string_lossy(), size, &now.to_rfc3339()).map_err(|e| e.to_string())
    })();
    if let Err(e) = record {
        let _ = rename_noreplace(&dest, &resolved);
        return Err(format!("não foi possível registrar a quarentena: {e}"));
    }
    Ok(())
}

/// An item must still be inside the quarantine folder it was recorded in: the database is not trusted
/// to point anywhere else.
fn inside_its_batch(r: &QuarantineRecord) -> bool {
    let dest = Path::new(&r.dest);
    dest.starts_with(&r.batch_dir)
        && dest.components().any(|c| c.as_os_str() == QUARANTINE_DIR || c.as_os_str() == "Quarantine")
        && !dest.components().any(|c| matches!(c, std::path::Component::ParentDir))
}

/// Puts items back. `to` is a folder to restore into when the original place is not usable.
/// Never overwrites: anything already there makes the item stay in quarantine.
pub fn restore(policy: &SafetyPolicy, db: &Db, ids: &[String], to: Option<&Path>) -> Vec<OperationOutcome> {
    ids.iter()
        .map(|id| {
            let rec = db.quarantine_item(id).ok().flatten();
            let size = rec.as_ref().map_or(0, |r| r.size);
            let result = rec.ok_or_else(|| "este item não está na quarentena".to_string()).and_then(|r| restore_one(policy, db, &r, to));
            let (ok, error, path) = match (&result, db.quarantine_item(id)) {
                (Ok(p), _) => (true, None, p.clone()),
                (Err(e), rec) => (false, Some(e.clone()), rec.ok().flatten().map_or(id.clone(), |r| r.origin)),
            };
            if let Err(err) = db.log_action("quarantineRestore", &path, ok, error.as_deref()) {
                tracing::warn!(%err, "could not write the operation log");
            }
            OperationOutcome { path, size, ok, error }
        })
        .collect()
}

fn restore_one(policy: &SafetyPolicy, db: &Db, r: &QuarantineRecord, to: Option<&Path>) -> Result<String, String> {
    if !inside_its_batch(r) {
        return Err("o registro aponta para fora da quarentena: nada foi movido".into());
    }
    let from = Path::new(&r.dest);
    if fs::symlink_metadata(from).is_err() {
        return Err("o item não está mais na pasta da quarentena".into());
    }
    let origin = Path::new(&r.origin);
    let target = match to {
        Some(dir) => dir.join(origin.file_name().ok_or("nome de origem inválido")?),
        None => origin.to_path_buf(),
    };
    let parent = target.parent().ok_or("destino inválido")?;
    if !parent.is_dir() {
        return Err("a pasta original não existe mais: escolha outra pasta".into());
    }
    policy.check_mutation(&target, &RemovalContext::default(), MutationKind::CreateFile).map_err(|v| v.to_string())?;
    if fs::symlink_metadata(&target).is_ok() {
        return Err(format!("já existe algo em {}: escolha outra pasta", target.display()));
    }
    rename_noreplace(from, &target).map_err(|e| io_text(&e))?;
    db.mark_quarantine_restored(&r.id).map_err(|e| e.to_string())?;
    Ok(target.to_string_lossy().into_owned())
}

/// Sends quarantined items to the Trash through `trasher`. Nothing is deleted permanently here.
pub fn empty(db: &Db, ids: &[String], trasher: &dyn Fn(&Path) -> Result<(), String>) -> Vec<OperationOutcome> {
    ids.iter()
        .map(|id| {
            let Some(r) = db.quarantine_item(id).ok().flatten() else {
                return OperationOutcome { path: id.clone(), size: 0, ok: false, error: Some("este item não está na quarentena".into()) };
            };
            let fail = |why: &str| OperationOutcome { path: r.dest.clone(), size: r.size, ok: false, error: Some(why.to_string()) };
            if !inside_its_batch(&r) {
                return fail("o registro aponta para fora da quarentena: nada foi enviado à Lixeira");
            }
            let path = Path::new(&r.dest);
            let result = if fs::symlink_metadata(path).is_err() { Ok(()) } else { trasher(path) }; // already gone: just close the record
            match result {
                Ok(()) => {
                    let _ = db.mark_quarantine_emptied(&r.id);
                    let _ = db.log_operation("quarantineEmpty", &r.dest, r.size, true, None);
                    OperationOutcome { path: r.dest.clone(), size: r.size, ok: true, error: None }
                }
                Err(e) => {
                    let _ = db.log_operation("quarantineEmpty", &r.dest, r.size, false, Some(&e));
                    fail(&e)
                }
            }
        })
        .collect()
}

/// Moves items to the Trash or into quarantine, as the user chose. The one place that decides.
pub fn remove(mode: RemovalMode, policy: &SafetyPolicy, db: &Db, request: &RemovalRequest, action: &str, env: &Env) -> Vec<OperationOutcome> {
    match mode {
        RemovalMode::Trash => crate::cleanup::move_to_trash(policy, db, request, action),
        RemovalMode::Quarantine => move_into(policy, db, request, env, &Local::now()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_exclusive_rename_refuses_an_existing_target_even_without_a_prior_check() {
        let dir = tempfile::tempdir().unwrap();
        let (from, to) = (dir.path().join("from"), dir.path().join("to"));
        fs::write(&from, "new").unwrap();
        fs::write(&to, "already here").unwrap();
        let err = rename_noreplace(&from, &to).unwrap_err();
        assert!(matches!(err.raw_os_error(), Some(17) | Some(66)) || err.kind() == std::io::ErrorKind::AlreadyExists, "{err:?}");
        assert_eq!(fs::read_to_string(&to).unwrap(), "already here");
        assert_eq!(fs::read_to_string(&from).unwrap(), "new");
        // And it moves normally onto a free name, keeping the inode.
        let free = dir.path().join("free");
        let ino = fs::metadata(&from).unwrap().ino();
        rename_noreplace(&from, &free).unwrap();
        assert_eq!(fs::metadata(&free).unwrap().ino(), ino);
    }
}
