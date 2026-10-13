//! Writing a report to the place the user chose. It never replaces a file unless told to, never writes
//! through a symbolic link, and never leaves a half-written file behind.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use crate::error::{AppError, AppResult};
use crate::filesystem::safety::{MutationKind, RemovalContext, SafetyPolicy};
use crate::quarantine::rename_noreplace;

/// Writes `content` to `path`. Returns how many bytes were written.
pub fn write_report(policy: &SafetyPolicy, path: &Path, extension: &str, content: &str, overwrite: bool) -> AppResult<u64> {
    if !path.is_absolute() {
        return Err(AppError::Invalid("o caminho do relatório precisa ser absoluto".into()));
    }
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).ok_or_else(|| AppError::Invalid("falta o nome do arquivo".into()))?;
    if !name.to_lowercase().ends_with(&format!(".{extension}")) {
        return Err(AppError::Invalid(format!("o arquivo precisa terminar em .{extension}")));
    }
    let parent = path.parent().filter(|p| p.is_dir()).ok_or_else(|| AppError::Invalid("a pasta de destino não existe".into()))?;
    // The same rules as any change the app makes: not system places, not its own data, not ignored folders.
    policy.check_mutation(path, &RemovalContext::default(), MutationKind::CreateFile)?;

    match fs::symlink_metadata(path) {
        Ok(m) if m.file_type().is_symlink() => return Err(AppError::Invalid("o destino é um link simbólico: escolha outro nome".into())),
        Ok(m) if m.is_dir() => return Err(AppError::Invalid("o destino é uma pasta".into())),
        Ok(_) if !overwrite => return Err(AppError::Exists(format!("já existe um arquivo em {}", path.display()))),
        _ => {}
    }

    // Write beside the target and move it into place: a failure never leaves a partial report, and
    // without `overwrite` the move itself refuses to replace anything that appeared meanwhile.
    let tmp = parent.join(format!(".{name}.omm-{}.tmp", uuid::Uuid::new_v4().simple()));
    let result = (|| -> AppResult<()> {
        fs::write(&tmp, content)?;
        fs::set_permissions(&tmp, fs::Permissions::from_mode(0o600))?;
        if overwrite {
            fs::rename(&tmp, path)?;
        } else {
            rename_noreplace(&tmp, path).map_err(|e| match e.raw_os_error() {
                Some(17) | Some(66) => AppError::Exists(format!("já existe um arquivo em {}", path.display())),
                _ => AppError::Io(e),
            })?;
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result?;
    Ok(content.len() as u64)
}
