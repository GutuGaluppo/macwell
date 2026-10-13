//! iCloud Drive, inferred from local signals. It says whether the Mac is signed in and syncing, and
//! whether Desktop and Documents are kept in iCloud. It never reads account details: from the
//! account list only the names and on/off flags of services are looked at.

use std::path::Path;

use serde::Serialize;

use super::{run_text, Reason, Source, Unreadable};
use crate::exec::CommandRunner;

const DEFAULTS: &str = "/usr/bin/defaults";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ICloudState {
    /// `~/Library/Mobile Documents/com~apple~CloudDocs` exists.
    pub drive_folder_present: bool,
    /// An Apple Account is signed in on this Mac; `None` when it could not be read.
    pub signed_in: Option<bool>,
    pub drive_enabled: Option<bool>,
    pub desktop_synced: Option<bool>,
    pub documents_synced: Option<bool>,
    /// Always true: these are local signals, not a check against iCloud.
    pub inferred: bool,
    pub unreadable: Vec<Unreadable>,
}

#[derive(Debug, PartialEq)]
pub struct Accounts {
    pub signed_in: bool,
    pub drive_enabled: bool,
}

/// `defaults export MobileMeAccounts -`. Only `Services[].Name` and `.Enabled` are examined.
pub fn parse_accounts(xml: &str) -> Result<Accounts, String> {
    let root = plist::Value::from_reader_xml(xml.as_bytes()).map_err(|e| e.to_string())?;
    let accounts = root.as_dictionary().and_then(|d| d.get("Accounts")).and_then(|v| v.as_array());
    let Some(accounts) = accounts else { return Ok(Accounts { signed_in: false, drive_enabled: false }) };
    let drive_enabled = accounts.iter().filter_map(|a| a.as_dictionary()).any(|a| {
        a.get("Services").and_then(|s| s.as_array()).is_some_and(|services| {
            services.iter().filter_map(|s| s.as_dictionary()).any(|s| {
                s.get("Name").and_then(|n| n.as_string()) == Some("MOBILE_DOCUMENTS") && s.get("Enabled").and_then(|e| e.as_boolean()) == Some(true)
            })
        })
    });
    Ok(Accounts { signed_in: !accounts.is_empty(), drive_enabled })
}

/// `defaults read com.apple.finder <key>`: `1`/`0`. A key that was never set is "not on", not an error.
fn finder_flag(runner: &dyn CommandRunner, key: &str, unreadable: &mut Vec<Unreadable>) -> Option<bool> {
    match run_text(runner, DEFAULTS, &["read", "com.apple.finder", key], 10) {
        Ok(t) => match t.trim() {
            "1" => Some(true),
            "0" => Some(false),
            _ => None,
        },
        Err((_, detail)) if detail.as_deref().is_some_and(|d| d.contains("does not exist")) => Some(false),
        Err((reason, detail)) => {
            unreadable.push(Unreadable { source: Source::FinderPreferences, reason, detail });
            None
        }
    }
}

pub fn read(runner: &dyn CommandRunner, home: &Path) -> ICloudState {
    let mut unreadable = Vec::new();
    let accounts = match run_text(runner, DEFAULTS, &["export", "MobileMeAccounts", "-"], 10) {
        Ok(t) => match parse_accounts(&t) {
            Ok(a) => Some(a),
            Err(_) => {
                unreadable.push(Unreadable { source: Source::MobileMeAccounts, reason: Reason::Unavailable, detail: Some("formato de resposta inesperado".into()) });
                None
            }
        },
        Err((reason, detail)) => {
            unreadable.push(Unreadable { source: Source::MobileMeAccounts, reason, detail });
            None
        }
    };
    let desktop_synced = finder_flag(runner, "FXICloudDriveDesktop", &mut unreadable);
    let documents_synced = finder_flag(runner, "FXICloudDriveDocuments", &mut unreadable);
    ICloudState {
        drive_folder_present: std::fs::symlink_metadata(home.join("Library/Mobile Documents/com~apple~CloudDocs")).is_ok_and(|m| m.is_dir()),
        signed_in: accounts.as_ref().map(|a| a.signed_in),
        drive_enabled: accounts.as_ref().map(|a| a.drive_enabled),
        desktop_synced,
        documents_synced,
        inferred: true,
        unreadable,
    }
}
