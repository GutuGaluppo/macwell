use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::cleanup::rules::{builtin_rules, custom_rule, evaluate, running_apps, still_allowed, valid_custom_folder, CleanupRule, Risk, RuleResult};
use crate::cleanup::{self, RemovalItem, RemovalRequest};
use crate::backup::gate::BackupAck;
use crate::commands::backup::require_backup_ack;
use crate::error::{AppError, AppResult};
use crate::filesystem::{probe_full_disk_access, AccessStatus};
use crate::state::AppState;
use crate::types::OperationOutcome;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomRuleDef {
    pub id: String,
    pub title: String,
    /// Relative to the home folder.
    pub folder: String,
    pub risk: Risk,
}

const CUSTOM_KEY: &str = "cleanup.customRules";

/// Top-level personal folders whose contents a custom rule may never target as a whole.
const PROTECTED_FOLDERS: &[&str] = &["Desktop", "Documents", "Downloads", "Pictures", "Movies", "Music", "Public", "Library", "Applications", "Library/Mobile Documents", "Library/CloudStorage", "Library/Mail", "Library/Messages", "Library/Photos"];

fn custom_defs(state: &AppState) -> Vec<CustomRuleDef> {
    state.db.lock().unwrap().setting(CUSTOM_KEY).ok().flatten().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default()
}

fn all_rules(state: &AppState) -> Vec<CleanupRule> {
    let mut rules = builtin_rules();
    rules.extend(custom_defs(state).iter().map(|d| custom_rule(&d.id, &d.title, &d.folder, d.risk)));
    rules
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RulesReport {
    pub results: Vec<RuleResult>,
    pub custom: Vec<CustomRuleDef>,
    pub full_disk_access: bool,
}

#[tauri::command]
pub async fn cleanup_rules(app: AppHandle) -> AppResult<RulesReport> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let fda = probe_full_disk_access(&state.paths.home) == AccessStatus::Granted;
        let results = evaluate(&state.paths.home, &all_rules(&state), &running_apps(), fda);
        Ok(RulesReport { results, custom: custom_defs(&state), full_disk_access: fda })
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

#[tauri::command]
pub fn add_custom_rule(state: tauri::State<'_, AppState>, title: String, folder: String, risk: Risk) -> AppResult<()> {
    let home = &state.paths.home;
    let rel = PathBuf::from(&folder)
        .strip_prefix(home)
        .map(|p| p.display().to_string())
        .map_err(|_| AppError::Invalid("the folder must be inside your home folder".into()))?;
    if !valid_custom_folder(&rel) || PROTECTED_FOLDERS.iter().any(|p| rel.eq_ignore_ascii_case(p)) {
        return Err(AppError::Invalid("this folder cannot be used as a cleanup rule".into()));
    }
    if !home.join(&rel).is_dir() {
        return Err(AppError::NotFound(folder));
    }
    let title = title.trim();
    if title.is_empty() || title.len() > 80 {
        return Err(AppError::Invalid("the rule needs a name".into()));
    }
    let mut defs = custom_defs(&state);
    defs.push(CustomRuleDef { id: uuid::Uuid::new_v4().to_string(), title: title.into(), folder: rel, risk });
    state.db.lock().unwrap().set_setting(CUSTOM_KEY, &serde_json::to_string(&defs).unwrap_or_default())?;
    Ok(())
}

#[tauri::command]
pub fn remove_custom_rule(state: tauri::State<'_, AppState>, id: String) -> AppResult<()> {
    let defs: Vec<CustomRuleDef> = custom_defs(&state).into_iter().filter(|d| d.id != id).collect();
    state.db.lock().unwrap().set_setting(CUSTOM_KEY, &serde_json::to_string(&defs).unwrap_or_default())?;
    Ok(())
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleRemoval {
    pub rule_id: String,
    pub path: String,
    pub size: u64,
}

/// Moves matches of cleanup rules to the Trash, re-checking each against its rule and the apps
/// that are open right now.
#[tauri::command]
pub async fn run_cleanup_rules(app: AppHandle, items: Vec<RuleRemoval>, backup_ack: Option<BackupAck>) -> AppResult<Vec<OperationOutcome>> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        require_backup_ack(&state, backup_ack.as_ref())?;
        let home = state.paths.home.clone();
        let rules = all_rules(&state);
        let running = running_apps();
        let mut outcomes = Vec::new();
        let mut accepted = Vec::new();
        for item in items {
            let check = rules
                .iter()
                .find(|r| r.id == item.rule_id)
                .ok_or_else(|| "unknown rule".to_string())
                .and_then(|rule| still_allowed(&home, rule, &PathBuf::from(&item.path), &running));
            match check {
                Ok(()) => accepted.push(RemovalItem { path: item.path, size: item.size }),
                Err(e) => outcomes.push(OperationOutcome { path: item.path, size: item.size, ok: false, error: Some(e) }),
            }
        }
        let policy = state.policy.read().unwrap().clone();
        let db = state.db.lock().unwrap();
        outcomes.extend(cleanup::move_to_trash(&policy, &db, &RemovalRequest { items: accepted, scan_root: Some(home.display().to_string()) }, "trash"));
        Ok(outcomes)
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}
