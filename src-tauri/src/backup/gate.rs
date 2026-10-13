//! The backup gate: before a batch cleanup, what the backup situation is and how firmly the user has
//! to acknowledge it. The decision is made here, on the Rust side, from fresh reads: the UI only
//! shows it and carries the user's answer, so skipping the dialog changes nothing.
//!
//! * `ok`: a recent completed backup and nothing at risk. No question asked.
//! * `warn`: something worth a look (work only on this Mac but a backup exists, automatic backups
//!   off, state not verifiable). The user acknowledges.
//! * `strong`: no destination, no recent backup, or work only on this Mac that no backup is known to
//!   protect. The user types a confirmation word.

use serde::{Deserialize, Serialize};

use super::timemachine::Status;
use super::BackupState;
use crate::dev::report::UnsyncedRepo;
use crate::error::AppError;

/// What the user has to type for a `strong` gate (compared without case or surrounding spaces).
pub const CONFIRM_WORD: &str = "CONFIRMO";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum GateLevel {
    Ok,
    Warn,
    Strong,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupGate {
    pub level: GateLevel,
    /// Neutral codes the UI turns into sentences, most important first.
    pub reasons: Vec<&'static str>,
    pub unpushed: Vec<UnsyncedRepo>,
    /// Repositories git could not read (or all of them, when git is unavailable).
    pub unreadable_repos: u32,
    pub backup: BackupState,
}

pub struct GateInput {
    pub backup: BackupState,
    pub unpushed: Vec<UnsyncedRepo>,
    pub unreadable_repos: u32,
}

pub fn evaluate(input: GateInput) -> BackupGate {
    let tm = &input.backup.time_machine;
    let at_risk = !input.unpushed.is_empty();
    let mut reasons: Vec<&'static str> = Vec::new();

    let level = match tm.status {
        Status::NoDestination => {
            reasons.push("noDestination");
            GateLevel::Strong
        }
        Status::Stale => {
            reasons.push("stale");
            GateLevel::Strong
        }
        Status::NeverBackedUp => {
            reasons.push("neverBackedUp");
            GateLevel::Strong
        }
        Status::Unknown => {
            reasons.push("unknown");
            // Nothing can be said about protection: harmless for a Mac with only caches, but
            // not for work that exists nowhere else.
            if at_risk { GateLevel::Strong } else { GateLevel::Warn }
        }
        Status::Recent => GateLevel::Ok,
    };
    if at_risk {
        reasons.push("unpushedRepos");
    }
    let mut level = level;
    if level == GateLevel::Ok && (at_risk || tm.auto_backup == Some(false) || input.unreadable_repos > 0) {
        level = GateLevel::Warn;
    }
    if tm.auto_backup == Some(false) {
        reasons.push("autoBackupOff");
    }
    if input.unreadable_repos > 0 {
        reasons.push("reposUnreadable");
    }
    BackupGate { level, reasons, unpushed: input.unpushed, unreadable_repos: input.unreadable_repos, backup: input.backup }
}

/// The user's answer to a gate.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupAck {
    /// The user saw the gate and pressed continue.
    pub acknowledged: bool,
    /// What they typed for a `strong` gate.
    pub typed: Option<String>,
}

/// Whether `ack` is enough for `gate`. Called by every gated command before it does anything.
pub fn verify(gate: &BackupGate, ack: Option<&BackupAck>) -> Result<(), AppError> {
    match gate.level {
        GateLevel::Ok => Ok(()),
        GateLevel::Warn => match ack {
            Some(a) if a.acknowledged => Ok(()),
            _ => Err(AppError::BackupGate("há um aviso sobre o backup: confirme que viu antes de continuar".into())),
        },
        GateLevel::Strong => match ack {
            Some(a) if a.acknowledged && a.typed.as_deref().is_some_and(|t| t.trim().eq_ignore_ascii_case(CONFIRM_WORD)) => Ok(()),
            _ => Err(AppError::BackupGate(format!("não há backup recente: digite {CONFIRM_WORD} para continuar"))),
        },
    }
}
