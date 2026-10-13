//! Backup Guard (M13): what the Mac knows about its own backups, read without changing anything.
//!
//! The rule that matters most: a failed read is never "all good". Every source that could not be
//! read is listed with the reason, and a state that depends on it becomes `unknown`.

pub mod exclusions;
pub mod gate;
pub mod icloud;
pub mod timemachine;

use serde::Serialize;

use crate::exec::{CmdOutput, CommandRunner};
use crate::filesystem::AccessStatus;

pub use icloud::ICloudState;
pub use timemachine::TimeMachineState;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupState {
    pub time_machine: TimeMachineState,
    pub icloud: ICloudState,
    /// Whether the app has Full Disk Access (inferred): without it some reads are refused.
    pub full_disk_access: AccessStatus,
    pub stale_days: u32,
    pub read_at: i64,
}

pub fn read_state(runner: &dyn CommandRunner, home: &std::path::Path, now_ms: i64, stale_days: u32, full_disk_access: AccessStatus) -> BackupState {
    BackupState {
        time_machine: timemachine::read(runner, now_ms, stale_days),
        icloud: icloud::read(runner, home),
        full_disk_access,
        stale_days,
        read_at: now_ms,
    }
}

// ---- shared by the readers

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Source {
    Preferences,
    DestinationInfo,
    Status,
    LatestBackup,
    LocalSnapshots,
    MobileMeAccounts,
    FinderPreferences,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Reason {
    /// macOS refused the read: usually Full Disk Access is missing.
    PermissionDenied,
    /// The backup disk is not connected or could not be mounted.
    NotMounted,
    /// The command failed, timed out, or answered in a format we do not know.
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Unreadable {
    pub source: Source,
    pub reason: Reason,
    /// First line of what the tool said, shortened.
    pub detail: Option<String>,
}

fn first_line(text: &str) -> Option<String> {
    text.lines().map(str::trim).find(|l| !l.is_empty()).map(|l| l.chars().take(160).collect())
}

/// Why a command did not give an answer.
pub(crate) fn classify_failure(text: &str) -> (Reason, Option<String>) {
    let lower = text.to_lowercase();
    let reason = if lower.contains("not permitted") || lower.contains("permission") || lower.contains("not authorized") {
        Reason::PermissionDenied
    } else if lower.contains("mount") {
        Reason::NotMounted
    } else {
        Reason::Unavailable
    };
    (reason, first_line(text))
}

/// Runs a read-only tool. Success gives its text; anything else becomes a reason.
pub(crate) fn run_text(runner: &dyn CommandRunner, program: &str, args: &[&str], timeout_secs: u64) -> Result<String, (Reason, Option<String>)> {
    let cmd = crate::exec::Cmd::new(program, args).timeout(std::time::Duration::from_secs(timeout_secs));
    let out: CmdOutput = runner.run(&cmd).map_err(|e| (Reason::Unavailable, first_line(&e.to_string())))?;
    if out.timed_out {
        return Err((Reason::Unavailable, Some("demorou demais para responder".into())));
    }
    if !out.success() {
        return Err(classify_failure(&format!("{}\n{}", out.stderr_str(), out.stdout_str())));
    }
    Ok(out.stdout_str())
}
