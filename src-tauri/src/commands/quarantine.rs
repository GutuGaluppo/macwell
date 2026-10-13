//! Quarantine commands: the removal mode, the list, restoring and sending items on to the Trash.
//! Moving items in happens through `commands::remove_items`, in whichever flow the user is using.

use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::cleanup::send_to_trash;
use crate::error::{AppError, AppResult};
use crate::quarantine::{self, RemovalMode, MODE_SETTING};
use crate::state::AppState;
use crate::types::OperationOutcome;

#[tauri::command]
pub fn get_removal_mode(state: tauri::State<'_, AppState>) -> RemovalMode {
    RemovalMode::from_setting(state.db.lock().unwrap().setting(MODE_SETTING).ok().flatten().as_deref())
}

/// Where removed items go: the Trash (default) or the quarantine.
#[tauri::command]
pub fn set_removal_mode(state: tauri::State<'_, AppState>, mode: RemovalMode) -> AppResult<()> {
    state.db.lock().unwrap().set_setting(MODE_SETTING, mode.as_str())
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuarantineView {
    pub id: String,
    pub origin: String,
    pub dest: String,
    pub size: u64,
    pub quarantined_at: i64,
    /// The item is still where the record says.
    pub present: bool,
    /// `quarantine.csv` of its dated folder.
    pub log_path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuarantineList {
    pub items: Vec<QuarantineView>,
    pub total_bytes: u64,
}

#[tauri::command]
pub async fn list_quarantine(app: AppHandle) -> AppResult<QuarantineList> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let records = state.db.lock().unwrap().quarantine_items()?;
        let items: Vec<QuarantineView> = records
            .into_iter()
            .map(|r| QuarantineView {
                present: std::fs::symlink_metadata(&r.dest).is_ok(),
                log_path: Path::new(&r.batch_dir).join("quarantine.csv").to_string_lossy().into_owned(),
                id: r.id,
                origin: r.origin,
                dest: r.dest,
                size: r.size,
                quarantined_at: r.quarantined_at,
            })
            .collect();
        let total_bytes = items.iter().filter(|i| i.present).map(|i| i.size).sum();
        Ok(QuarantineList { items, total_bytes })
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

/// Puts items back where they came from, or into `to`. Never overwrites. Needs the user's confirmation.
#[tauri::command]
pub async fn restore_quarantine(app: AppHandle, ids: Vec<String>, to: Option<String>, confirmed: bool) -> AppResult<Vec<OperationOutcome>> {
    if !confirmed {
        return Err(AppError::Invalid("restaurar precisa ser confirmado".into()));
    }
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let policy = state.policy.read().unwrap().clone();
        let to: Option<PathBuf> = match to {
            Some(t) => {
                let p = PathBuf::from(&t);
                if !p.is_absolute() || !p.is_dir() {
                    return Err(AppError::Invalid("escolha uma pasta existente".into()));
                }
                Some(p)
            }
            None => None,
        };
        let db = state.db.lock().unwrap();
        Ok(quarantine::restore(&policy, &db, &ids, to.as_deref()))
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}

/// Sends quarantined items to the Trash (where they can still be recovered). Needs the user's confirmation.
#[tauri::command]
pub async fn empty_quarantine(app: AppHandle, ids: Vec<String>, confirmed: bool) -> AppResult<Vec<OperationOutcome>> {
    if !confirmed {
        return Err(AppError::Invalid("esvaziar a quarentena precisa ser confirmado".into()));
    }
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let db = state.db.lock().unwrap();
        Ok(quarantine::empty(&db, &ids, &|p| send_to_trash(p)))
    })
    .await
    .map_err(|e| AppError::Other(e.to_string()))?
}
