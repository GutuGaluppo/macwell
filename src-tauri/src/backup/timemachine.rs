//! Time Machine, read from the places that never have side effects:
//!
//! * the preferences file (through `defaults`): destinations, `AutoBackup`, and `SnapshotDates`, the
//!   completed backups macOS remembers. It needs no disk to be connected;
//! * `tmutil destinationinfo`, `status` and `listlocalsnapshotdates`.
//!
//! `tmutil latestbackup` is only called when a destination is already mounted: it tries to mount the
//! disk, and reading the state must not do that. `listbackups` is never used (it needs Full Disk
//! Access and a mounted disk).

use std::time::{SystemTime, UNIX_EPOCH};

use chrono::{Local, NaiveDateTime, TimeZone};
use regex::Regex;
use serde::Serialize;

use super::{run_text, Reason, Source, Unreadable};
use crate::exec::CommandRunner;

const TMUTIL: &str = "/usr/bin/tmutil";
const DEFAULTS: &str = "/usr/bin/defaults";
const PREFS_DOMAIN: &str = "/Library/Preferences/com.apple.TimeMachine";
pub const DEFAULT_STALE_DAYS: u32 = 7;
const DAY_MS: i64 = 86_400_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    /// Time Machine has no destination: nothing is being backed up this way.
    NoDestination,
    /// A completed backup within the limit.
    Recent,
    /// The newest completed backup is older than the limit.
    Stale,
    /// A destination exists but no completed backup is known.
    NeverBackedUp,
    /// The needed sources could not be read, so nothing can be said.
    Unknown,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Destination {
    pub id: String,
    pub name: Option<String>,
    pub kind: Option<String>,
    /// `None` when `tmutil destinationinfo` could not be read.
    pub mounted: Option<bool>,
    pub quota_gb: Option<u64>,
    pub bytes_used: Option<u64>,
    pub bytes_available: Option<u64>,
    pub encrypted: Option<bool>,
    pub last_backup_ms: Option<i64>,
    pub last_attempt_ms: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TimeMachineState {
    pub status: Status,
    pub destinations: Vec<Destination>,
    /// `Some(false)`: automatic backups are switched off.
    pub auto_backup: Option<bool>,
    /// Newest completed backup on any destination.
    pub last_backup_ms: Option<i64>,
    pub last_backup_source: Option<Source>,
    pub last_attempt_ms: Option<i64>,
    pub age_days: Option<i64>,
    pub running: bool,
    pub percent: Option<f64>,
    /// Snapshots on the startup disk itself. They are not a backup: they vanish with the disk.
    pub local_snapshot_count: u32,
    pub newest_local_snapshot_ms: Option<i64>,
    pub unreadable: Vec<Unreadable>,
}

// ---- parsers (pure)

fn date_ms(v: &plist::Value) -> Option<i64> {
    let t: SystemTime = v.as_date()?.into();
    Some(t.duration_since(UNIX_EPOCH).ok()?.as_millis() as i64)
}

fn dates(v: Option<&plist::Value>) -> Vec<i64> {
    v.and_then(|v| v.as_array()).map(|a| a.iter().filter_map(date_ms).collect()).unwrap_or_default()
}

fn uint(v: Option<&plist::Value>) -> Option<u64> {
    v.and_then(|v| v.as_unsigned_integer().or_else(|| v.as_signed_integer().and_then(|i| u64::try_from(i).ok())))
}

#[derive(Debug, Default, PartialEq)]
pub struct PrefsDestination {
    pub id: String,
    pub name: Option<String>,
    pub quota_gb: Option<u64>,
    pub bytes_used: Option<u64>,
    pub bytes_available: Option<u64>,
    pub encrypted: Option<bool>,
    pub snapshot_dates: Vec<i64>,
    pub attempt_dates: Vec<i64>,
}

#[derive(Debug, Default, PartialEq)]
pub struct Prefs {
    pub auto_backup: Option<bool>,
    pub destinations: Vec<PrefsDestination>,
}

/// `defaults export /Library/Preferences/com.apple.TimeMachine -`.
pub fn parse_preferences(xml: &str) -> Result<Prefs, String> {
    let root = plist::Value::from_reader_xml(xml.as_bytes()).map_err(|e| e.to_string())?;
    let dict = root.as_dictionary().ok_or("not a dictionary")?;
    let auto_backup = dict.get("AutoBackup").and_then(|v| v.as_boolean().or_else(|| v.as_signed_integer().map(|i| i != 0)));
    let destinations = dict
        .get("Destinations")
        .and_then(|v| v.as_array())
        .map(|list| {
            list.iter()
                .filter_map(|d| d.as_dictionary())
                .map(|d| PrefsDestination {
                    id: d.get("DestinationID").and_then(|v| v.as_string()).unwrap_or_default().to_string(),
                    name: d.get("LastKnownVolumeName").and_then(|v| v.as_string()).map(String::from),
                    quota_gb: uint(d.get("QuotaGB")),
                    bytes_used: uint(d.get("BytesUsed")),
                    bytes_available: uint(d.get("BytesAvailable")),
                    encrypted: d.get("LastKnownEncryptionState").and_then(|v| v.as_string()).map(|s| s.eq_ignore_ascii_case("encrypted")),
                    snapshot_dates: dates(d.get("SnapshotDates")),
                    attempt_dates: dates(d.get("AttemptDates")),
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(Prefs { auto_backup, destinations })
}

#[derive(Debug, PartialEq)]
pub struct InfoDestination {
    pub id: String,
    pub name: Option<String>,
    pub kind: Option<String>,
    pub mounted: bool,
}

/// `tmutil destinationinfo -X`. No destination at all is a valid answer, not an error.
pub fn parse_destination_info(text: &str) -> Result<Vec<InfoDestination>, String> {
    if text.contains("No destinations") {
        return Ok(Vec::new());
    }
    let root = plist::Value::from_reader_xml(text.as_bytes()).map_err(|e| e.to_string())?;
    let list = root.as_dictionary().and_then(|d| d.get("Destinations")).and_then(|v| v.as_array());
    Ok(list
        .map(|l| {
            l.iter()
                .filter_map(|d| d.as_dictionary())
                .map(|d| InfoDestination {
                    id: d.get("ID").and_then(|v| v.as_string()).unwrap_or_default().to_string(),
                    name: d.get("Name").and_then(|v| v.as_string()).map(String::from),
                    kind: d.get("Kind").and_then(|v| v.as_string()).map(String::from),
                    mounted: d.get("MountPoint").is_some(),
                })
                .collect()
        })
        .unwrap_or_default())
}

#[derive(Debug, PartialEq)]
pub struct Running {
    pub running: bool,
    pub percent: Option<f64>,
}

/// `tmutil status -X`. A percentage below zero means "not applicable".
pub fn parse_status(text: &str) -> Result<Running, String> {
    let root = plist::Value::from_reader_xml(text.as_bytes()).map_err(|e| e.to_string())?;
    let d = root.as_dictionary().ok_or("not a dictionary")?;
    let running = d.get("Running").and_then(|v| v.as_boolean()).unwrap_or(false);
    let percent = d.get("Percent").and_then(|v| v.as_real()).filter(|p| *p >= 0.0);
    Ok(Running { running, percent })
}

/// `2026-10-13-112402` in local time, as `tmutil` prints it.
fn parse_local_stamp(s: &str) -> Option<i64> {
    let naive = NaiveDateTime::parse_from_str(s, "%Y-%m-%d-%H%M%S").ok()?;
    Some(Local.from_local_datetime(&naive).earliest()?.timestamp_millis())
}

fn stamp_regex() -> Regex {
    Regex::new(r"\d{4}-\d{2}-\d{2}-\d{6}").unwrap()
}

/// The newest backup named in a `tmutil latestbackup` path.
pub fn parse_latest_backup(text: &str) -> Option<i64> {
    let last = text.lines().map(str::trim).filter(|l| !l.is_empty()).last()?;
    let name = last.rsplit('/').next()?;
    stamp_regex().find(name).and_then(|m| parse_local_stamp(m.as_str()))
}

/// `tmutil listlocalsnapshotdates`: one `YYYY-MM-DD-HHMMSS` per line after a header.
pub fn parse_local_snapshots(text: &str) -> Vec<i64> {
    let re = Regex::new(r"^\d{4}-\d{2}-\d{2}-\d{6}$").unwrap();
    text.lines().map(str::trim).filter(|l| re.is_match(l)).filter_map(parse_local_stamp).collect()
}

// ---- classification (pure)

#[derive(Debug, Clone, Copy)]
pub struct Facts {
    /// How many destinations are configured; `None` when no source could tell.
    pub destinations: Option<usize>,
    pub prefs_readable: bool,
    pub last_backup_ms: Option<i64>,
}

pub fn classify(f: &Facts, now_ms: i64, stale_days: u32) -> Status {
    match (f.destinations, f.last_backup_ms) {
        (Some(0), _) => Status::NoDestination,
        (_, Some(last)) => {
            if now_ms - last <= stale_days as i64 * DAY_MS {
                Status::Recent
            } else {
                Status::Stale
            }
        }
        (Some(_), None) if f.prefs_readable => Status::NeverBackedUp,
        _ => Status::Unknown,
    }
}

// ---- reading

pub fn read(runner: &dyn CommandRunner, now_ms: i64, stale_days: u32) -> TimeMachineState {
    let mut unreadable = Vec::new();
    let mut fail = |source: Source, (reason, detail): (Reason, Option<String>)| unreadable.push(Unreadable { source, reason, detail });
    let odd = |_: String| (Reason::Unavailable, Some("formato de resposta inesperado".to_string()));

    let prefs = match run_text(runner, DEFAULTS, &["export", PREFS_DOMAIN, "-"], 10) {
        Ok(t) => parse_preferences(&t).map_err(|e| fail(Source::Preferences, odd(e))).ok(),
        Err(e) => {
            fail(Source::Preferences, e);
            None
        }
    };
    let info = match run_text(runner, TMUTIL, &["destinationinfo", "-X"], 15) {
        Ok(t) => parse_destination_info(&t).map_err(|e| fail(Source::DestinationInfo, odd(e))).ok(),
        Err(e) => {
            fail(Source::DestinationInfo, e);
            None
        }
    };
    let running = match run_text(runner, TMUTIL, &["status", "-X"], 15) {
        Ok(t) => parse_status(&t).map_err(|e| fail(Source::Status, odd(e))).ok(),
        Err(e) => {
            fail(Source::Status, e);
            None
        }
    };
    let local = match run_text(runner, TMUTIL, &["listlocalsnapshotdates"], 15) {
        Ok(t) => parse_local_snapshots(&t),
        Err(e) => {
            fail(Source::LocalSnapshots, e);
            Vec::new()
        }
    };

    // The authoritative list of configured destinations is `destinationinfo`; the preferences add detail.
    let empty = Prefs::default();
    let pref_dests = prefs.as_ref().map(|p| &p.destinations).unwrap_or(&empty.destinations);
    let find_pref = |id: &str| pref_dests.iter().find(|d| d.id == id);
    let mut destinations: Vec<Destination> = match &info {
        Some(list) => list
            .iter()
            .map(|i| {
                let p = find_pref(&i.id);
                Destination {
                    id: i.id.clone(),
                    name: i.name.clone().or_else(|| p.and_then(|p| p.name.clone())),
                    kind: i.kind.clone(),
                    mounted: Some(i.mounted),
                    quota_gb: p.and_then(|p| p.quota_gb),
                    bytes_used: p.and_then(|p| p.bytes_used),
                    bytes_available: p.and_then(|p| p.bytes_available),
                    encrypted: p.and_then(|p| p.encrypted),
                    last_backup_ms: p.and_then(|p| p.snapshot_dates.iter().copied().max()),
                    last_attempt_ms: p.and_then(|p| p.attempt_dates.iter().copied().max()),
                }
            })
            .collect(),
        None => pref_dests
            .iter()
            .map(|p| Destination {
                id: p.id.clone(),
                name: p.name.clone(),
                kind: None,
                mounted: None,
                quota_gb: p.quota_gb,
                bytes_used: p.bytes_used,
                bytes_available: p.bytes_available,
                encrypted: p.encrypted,
                last_backup_ms: p.snapshot_dates.iter().copied().max(),
                last_attempt_ms: p.attempt_dates.iter().copied().max(),
            })
            .collect(),
    };

    // `latestbackup` mounts the destination, so it only runs when one is already mounted.
    let mut from_tmutil = None;
    if destinations.iter().any(|d| d.mounted == Some(true)) {
        match run_text(runner, TMUTIL, &["latestbackup"], 20) {
            Ok(t) => from_tmutil = parse_latest_backup(&t),
            Err(e) => unreadable.push(Unreadable { source: Source::LatestBackup, reason: e.0, detail: e.1 }),
        }
    }

    let from_prefs = destinations.iter().filter_map(|d| d.last_backup_ms).max();
    let (last_backup_ms, last_backup_source) = match (from_tmutil, from_prefs) {
        (Some(t), Some(p)) if t >= p => (Some(t), Some(Source::LatestBackup)),
        (Some(t), None) => (Some(t), Some(Source::LatestBackup)),
        (_, Some(p)) => (Some(p), Some(Source::Preferences)),
        (None, None) => (None, None),
    };
    let last_attempt_ms = destinations.iter().filter_map(|d| d.last_attempt_ms).max();
    destinations.sort_by(|a, b| a.id.cmp(&b.id));

    let facts = Facts {
        destinations: info.as_ref().map(|l| l.len()).or_else(|| prefs.as_ref().map(|p| p.destinations.len())),
        prefs_readable: prefs.is_some(),
        last_backup_ms,
    };
    TimeMachineState {
        status: classify(&facts, now_ms, stale_days),
        destinations,
        auto_backup: prefs.as_ref().and_then(|p| p.auto_backup),
        last_backup_ms,
        last_backup_source,
        last_attempt_ms,
        age_days: last_backup_ms.map(|t| ((now_ms - t) / DAY_MS).max(0)),
        running: running.as_ref().is_some_and(|r| r.running),
        percent: running.and_then(|r| r.percent),
        local_snapshot_count: local.len() as u32,
        newest_local_snapshot_ms: local.iter().copied().max(),
        unreadable,
    }
}
