//! Executes reviewed cleanup plans. Every item is validated by the safety layer immediately before
//! it is touched, moved to the Trash (never deleted, except when emptying the Trash itself) and
//! written to the operation log.

pub mod downloads;
pub mod rules;
pub mod trash_bin;

use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::db::Db;
use crate::filesystem::safety::{RemovalContext, SafetyPolicy};
use crate::types::OperationOutcome;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemovalItem {
    pub path: String,
    /// Size shown to the user; recorded in the log and in the "space freed" metric.
    pub size: u64,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemovalRequest {
    pub items: Vec<RemovalItem>,
    /// The folder the items were found in; they must still be inside it.
    pub scan_root: Option<String>,
}

fn trash_context() -> trash::TrashContext {
    use trash::macos::{DeleteMethod, TrashContextExtMacos};
    let mut ctx = trash::TrashContext::default();
    // NSFileManager: no Finder automation permission, no sound.
    ctx.set_delete_method(DeleteMethod::NsFileManager);
    ctx
}

/// Sends one path to the Trash. Used for what has already been validated elsewhere.
pub fn send_to_trash(path: &Path) -> Result<(), String> {
    trash_context().delete(path).map_err(|e| e.to_string())
}

/// Moves each item to the Trash after validating it. Items are independent: one failure does not
/// stop the others.
pub fn move_to_trash(policy: &SafetyPolicy, db: &Db, request: &RemovalRequest, action: &str) -> Vec<OperationOutcome> {
    let ctx = RemovalContext { scan_root: request.scan_root.as_ref().map(PathBuf::from), allow_system_library: false };
    let trash = trash_context();
    // Deepest paths first: a file inside a folder that is also being removed goes before it.
    let mut items: Vec<&RemovalItem> = request.items.iter().collect();
    items.sort_by_key(|i| std::cmp::Reverse(i.path.matches('/').count()));
    items
        .into_iter()
        .map(|item| {
            let path = Path::new(&item.path);
            let result = policy
                .check(path, &ctx)
                .map_err(|v| v.to_string())
                .and_then(|resolved| trash.delete(&resolved).map_err(|e| e.to_string()));
            let (ok, error) = match result {
                Ok(()) => (true, None),
                Err(e) => (false, Some(e)),
            };
            if let Err(err) = db.log_operation(action, &item.path, item.size, ok, error.as_deref()) {
                tracing::warn!(%err, "could not write the operation log");
            }
            tracing::info!(path = %item.path, ok, error = error.as_deref().unwrap_or(""), "move to trash");
            OperationOutcome { path: item.path.clone(), size: item.size, ok, error }
        })
        .collect()
}
