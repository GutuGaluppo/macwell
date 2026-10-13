//! Developer Care (M12): git repositories, regenerable artifacts, secret checks and WIP snapshots.
//! Everything here is local. Git is only ever read through [`crate::exec::CommandRunner`].

pub mod artifacts;
pub mod discover;
pub mod git;
pub mod push;
pub mod removal;
pub mod report;
pub mod secrets;
pub mod snapshot;
pub mod visibility;

use std::path::{Path, PathBuf};

use crate::error::{AppError, AppResult};

/// Folders that can never be a project root: system locations and the app's own data.
const FORBIDDEN_ROOTS: &[&str] = &["/System", "/bin", "/sbin", "/usr", "/private", "/cores", "/dev", "/Library", "/Applications"];

/// A folder the user may add as a place to look for projects. Returns its canonical path.
pub fn validate_root(path: &Path, own_paths: &[PathBuf]) -> AppResult<PathBuf> {
    validate_root_in(path, own_paths, FORBIDDEN_ROOTS)
}

fn validate_root_in(path: &Path, own_paths: &[PathBuf], forbidden: &[&str]) -> AppResult<PathBuf> {
    if !path.is_absolute() {
        return Err(AppError::Invalid("o caminho precisa ser absoluto".into()));
    }
    let canonical = std::fs::canonicalize(path).map_err(|_| AppError::NotFound(path.display().to_string()))?;
    if !canonical.is_dir() {
        return Err(AppError::Invalid("não é uma pasta".into()));
    }
    if canonical == Path::new("/") || forbidden.iter().any(|f| canonical.starts_with(f)) {
        return Err(AppError::Invalid(format!("{} é um local do sistema", canonical.display())));
    }
    if own_paths.iter().any(|o| canonical.starts_with(o) || o.starts_with(&canonical)) {
        return Err(AppError::Invalid("esta pasta contém arquivos do próprio OrganizeMyMac".into()));
    }
    Ok(canonical)
}

/// A repository the UI asked about: it must be inside one of the user's roots and have a real
/// `.git` (a symlinked `.git` is refused). The UI never gets to point git at an arbitrary folder.
pub fn resolve_repo(roots: &[PathBuf], repo: &Path) -> AppResult<PathBuf> {
    if !repo.is_absolute() {
        return Err(AppError::Invalid("o caminho precisa ser absoluto".into()));
    }
    let canonical = std::fs::canonicalize(repo).map_err(|_| AppError::NotFound(repo.display().to_string()))?;
    let inside = roots.iter().filter_map(|r| std::fs::canonicalize(r).ok()).any(|r| canonical.starts_with(&r));
    if !inside {
        return Err(AppError::Invalid("o repositório não está em nenhuma das pastas de projetos".into()));
    }
    match std::fs::symlink_metadata(canonical.join(".git")) {
        Ok(m) if m.is_dir() || m.is_file() => Ok(canonical),
        _ => Err(AppError::Invalid("não é um repositório git".into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    fn tmp() -> (tempfile::TempDir, PathBuf) {
        let d = tempfile::tempdir().unwrap();
        let p = std::fs::canonicalize(d.path()).unwrap();
        (d, p)
    }

    #[test]
    fn roots_refuse_system_places_files_and_own_data() {
        let (_d, root) = tmp();
        std::fs::create_dir_all(root.join("Projects")).unwrap();
        std::fs::write(root.join("file.txt"), "x").unwrap();
        // The temp folder lives under /private, which is forbidden for real, so use an empty list here.
        let ok = |p: &Path, own: &[PathBuf]| validate_root_in(p, own, &[]);
        assert_eq!(ok(&root.join("Projects"), &[]).unwrap(), root.join("Projects"));
        for bad in ["/", "/System", "/usr/local", "/Library/Fonts", "/Applications", "/private/etc", "relative"] {
            assert!(validate_root(Path::new(bad), &[]).is_err(), "{bad}");
        }
        assert!(ok(&root.join("file.txt"), &[]).is_err());
        assert!(ok(&root.join("missing"), &[]).is_err());
        let own = vec![root.join("Projects")];
        assert!(ok(&root.join("Projects"), &own).is_err());
        assert!(ok(&root, &own).is_err(), "a root that contains the app data is refused too");
        assert!(validate_root_in(&root, &[], &[root.to_str().unwrap()]).is_err(), "the forbidden list is honoured");
    }

    #[test]
    fn repos_must_be_inside_a_root_and_have_a_real_dot_git() {
        let (_d, root) = tmp();
        let (_o, outside) = tmp();
        std::fs::create_dir_all(root.join("a/.git")).unwrap();
        std::fs::create_dir_all(root.join("plain")).unwrap();
        std::fs::create_dir_all(outside.join("elsewhere/.git")).unwrap();
        std::fs::create_dir_all(root.join("fake")).unwrap();
        symlink(outside.join("elsewhere/.git"), root.join("fake/.git")).unwrap();
        symlink(outside.join("elsewhere"), root.join("link-out")).unwrap();
        let roots = vec![root.clone()];
        assert_eq!(resolve_repo(&roots, &root.join("a")).unwrap(), root.join("a"));
        assert!(resolve_repo(&roots, &root.join("plain")).is_err());
        assert!(resolve_repo(&roots, &root.join("fake")).is_err(), "symlinked .git");
        assert!(resolve_repo(&roots, &outside.join("elsewhere")).is_err(), "outside every root");
        assert!(resolve_repo(&roots, &root.join("link-out")).is_err(), "a symlink that leaves the root");
        assert!(resolve_repo(&roots, Path::new("relative/a")).is_err());
        assert!(resolve_repo(&[], &root.join("a")).is_err());
    }
}
