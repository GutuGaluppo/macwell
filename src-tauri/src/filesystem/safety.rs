//! Safety layer. Every destructive operation goes through [`SafetyPolicy::check`] right before it
//! runs, even when the path came from our own scanner: the filesystem may have changed since the
//! scan, and the UI is never trusted to send a safe path.

use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::path::{Component, Path, PathBuf};

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, Serialize)]
#[serde(tag = "rule", rename_all = "camelCase")]
pub enum SafetyViolation {
    #[error("path must be absolute and must not contain '..'")]
    NotAbsolute,
    #[error("the item no longer exists")]
    Missing,
    #[error("{0} is a protected system location")]
    ProtectedLocation(String),
    #[error("{0} is an essential folder and cannot be removed itself")]
    EssentialFolder(String),
    #[error("the item is outside the folder that was scanned")]
    OutsideScanRoot,
    #[error("the path goes through a symbolic link that leaves the scanned folder")]
    SymlinkEscape,
    #[error("the item is on a read-only volume")]
    ReadOnlyVolume,
    #[error("the item belongs to OrganizeMyMac")]
    OwnFiles,
    #[error("the item is in your ignore list")]
    Ignored,
}

/// Context of a single removal request.
#[derive(Debug, Clone, Default)]
pub struct RemovalContext {
    /// When the item came from a scan, it must still be inside this folder after resolving symlinks.
    pub scan_root: Option<PathBuf>,
    /// `/Library` is blocked unless the request comes from an explicitly supported, reviewed rule.
    pub allow_system_library: bool,
}

/// What a non-removing action does to its target path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MutationKind {
    /// The item exists and is changed or moved (quarantine source, exclusion target).
    ExistingItem,
    /// A new file is created at the path (it need not exist yet; its parent must).
    CreateFile,
}

#[derive(Debug, Clone)]
pub struct SafetyPolicy {
    protected_roots: Vec<PathBuf>,
    system_library: PathBuf,
    essential: Vec<PathBuf>,
    own_paths: Vec<PathBuf>,
    ignored: Vec<PathBuf>,
}

impl SafetyPolicy {
    /// The policy used by the app.
    pub fn system(home: &Path, own_paths: Vec<PathBuf>) -> Self {
        let protected_roots = ["/System", "/bin", "/sbin", "/usr", "/private", "/cores", "/dev"]
            .iter()
            .map(PathBuf::from)
            .collect();
        let mut essential: Vec<PathBuf> = ["/", "/Applications", "/Users", "/Library", "/Volumes"]
            .iter()
            .map(PathBuf::from)
            .collect();
        essential.push(home.to_path_buf());
        for sub in [
            "Library", "Desktop", "Documents", "Downloads", "Movies", "Music", "Pictures", "Public",
            "Applications", ".Trash", "Library/Application Support", "Library/Caches",
            "Library/Preferences", "Library/Containers", "Library/Group Containers", "Library/Logs",
            "Library/Mobile Documents", "Library/CloudStorage",
        ] {
            essential.push(home.join(sub));
        }
        SafetyPolicy {
            protected_roots,
            system_library: PathBuf::from("/Library"),
            essential,
            own_paths,
            ignored: Vec::new(),
        }
    }

    /// A policy with explicit lists, for tests on synthetic fixtures.
    pub fn custom(protected_roots: Vec<PathBuf>, essential: Vec<PathBuf>, own_paths: Vec<PathBuf>) -> Self {
        SafetyPolicy {
            protected_roots,
            system_library: PathBuf::from("/Library"),
            essential,
            own_paths,
            ignored: Vec::new(),
        }
    }

    pub fn set_ignored(&mut self, ignored: Vec<PathBuf>) {
        self.ignored = ignored;
    }

    pub fn is_ignored(&self, path: &Path) -> bool {
        self.ignored.iter().any(|i| path.starts_with(i))
    }

    /// Validates that `path` may be moved to the Trash (or deleted). Returns the resolved path: the
    /// parent with symlinks resolved, plus the final component as is (a symlink is removed itself,
    /// never its target).
    pub fn check(&self, path: &Path, ctx: &RemovalContext) -> Result<PathBuf, SafetyViolation> {
        self.validate(path, ctx, true)
    }

    /// Same rules as [`check`](Self::check) for actions that do not remove anything: creating a
    /// marker file, moving an item into quarantine, setting a Time Machine exclusion. It never
    /// relaxes a rule; `CreateFile` only waives the "must already exist" test, in which case the
    /// parent folder is validated and must exist. Removal still goes through `check`.
    pub fn check_mutation(&self, path: &Path, ctx: &RemovalContext, kind: MutationKind) -> Result<PathBuf, SafetyViolation> {
        self.validate(path, ctx, kind == MutationKind::ExistingItem)
    }

    fn validate(&self, path: &Path, ctx: &RemovalContext, require_exists: bool) -> Result<PathBuf, SafetyViolation> {
        if !path.is_absolute() || path.components().any(|c| matches!(c, Component::ParentDir)) {
            return Err(SafetyViolation::NotAbsolute);
        }
        let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
            return Err(SafetyViolation::EssentialFolder("/".into()));
        };
        if require_exists && std::fs::symlink_metadata(path).is_err() {
            return Err(SafetyViolation::Missing);
        }
        let parent = std::fs::canonicalize(parent).map_err(|_| SafetyViolation::Missing)?;
        let resolved = parent.join(name);

        if let Some(root) = &ctx.scan_root {
            let root = std::fs::canonicalize(root).map_err(|_| SafetyViolation::Missing)?;
            if !resolved.starts_with(&root) || resolved == root {
                return Err(if path.starts_with(ctx.scan_root.clone().unwrap_or_default()) {
                    SafetyViolation::SymlinkEscape
                } else {
                    SafetyViolation::OutsideScanRoot
                });
            }
        }

        for candidate in [path, resolved.as_path()] {
            if let Some(e) = self.essential.iter().find(|e| candidate == e.as_path()) {
                return Err(SafetyViolation::EssentialFolder(e.display().to_string()));
            }
            if is_volume_root(candidate) {
                return Err(SafetyViolation::EssentialFolder(candidate.display().to_string()));
            }
            if let Some(p) = self.protected_roots.iter().find(|p| candidate.starts_with(p)) {
                return Err(SafetyViolation::ProtectedLocation(p.display().to_string()));
            }
            if !ctx.allow_system_library && candidate.starts_with(&self.system_library) {
                return Err(SafetyViolation::ProtectedLocation(self.system_library.display().to_string()));
            }
            if self.own_paths.iter().any(|o| candidate.starts_with(o) || o.starts_with(candidate)) {
                return Err(SafetyViolation::OwnFiles);
            }
            if self.is_ignored(candidate) {
                return Err(SafetyViolation::Ignored);
            }
        }

        if is_read_only(&parent) {
            return Err(SafetyViolation::ReadOnlyVolume);
        }
        Ok(resolved)
    }
}

/// `/Volumes/<name>` itself.
fn is_volume_root(path: &Path) -> bool {
    let mut comps = path.components();
    matches!(
        (comps.next(), comps.next(), comps.next(), comps.next()),
        (Some(Component::RootDir), Some(Component::Normal(v)), Some(Component::Normal(_)), None) if v == "Volumes"
    )
}

pub fn is_read_only(path: &Path) -> bool {
    let Ok(c) = CString::new(path.as_os_str().as_bytes()) else { return false };
    let mut st: libc::statfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statfs(c.as_ptr(), &mut st) } != 0 {
        return false;
    }
    st.f_flags & (libc::MNT_RDONLY as u32) != 0
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    struct Fixture {
        _dir: tempfile::TempDir,
        root: PathBuf,
        policy: SafetyPolicy,
    }

    fn fixture() -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        let scan = root.join("scan");
        fs::create_dir_all(scan.join("docs")).unwrap();
        fs::write(scan.join("docs/a.txt"), b"a").unwrap();
        fs::create_dir_all(root.join("outside")).unwrap();
        fs::write(root.join("outside/secret.txt"), b"s").unwrap();
        fs::create_dir_all(root.join("system/core")).unwrap();
        fs::write(root.join("system/core/kernel"), b"k").unwrap();
        fs::create_dir_all(root.join("app-data")).unwrap();
        fs::write(root.join("app-data/db.sqlite"), b"d").unwrap();
        std::os::unix::fs::symlink(root.join("outside"), scan.join("escape")).unwrap();
        std::os::unix::fs::symlink(root.join("outside/secret.txt"), scan.join("link.txt")).unwrap();
        let policy = SafetyPolicy::custom(
            vec![root.join("system")],
            vec![scan.clone(), root.join("home")],
            vec![root.join("app-data")],
        );
        Fixture { _dir: dir, root, policy }
    }

    fn ctx(root: &Path) -> RemovalContext {
        RemovalContext { scan_root: Some(root.join("scan")), allow_system_library: false }
    }

    #[test]
    fn allowed_path() {
        let f = fixture();
        let p = f.root.join("scan/docs/a.txt");
        assert_eq!(f.policy.check(&p, &ctx(&f.root)).unwrap(), p);
    }

    #[test]
    fn blocked_path() {
        let f = fixture();
        let err = f.policy.check(&f.root.join("system/core/kernel"), &RemovalContext::default()).unwrap_err();
        assert!(matches!(err, SafetyViolation::ProtectedLocation(_)));
        let real = SafetyPolicy::system(Path::new("/Users/nobody"), vec![]);
        for p in ["/System/Library", "/usr/bin/true", "/bin/ls", "/private/etc/hosts"] {
            assert!(real.check(Path::new(p), &RemovalContext::default()).is_err(), "{p}");
        }
    }

    #[test]
    fn system_library_needs_reviewed_rule() {
        let real = SafetyPolicy::system(Path::new("/Users/nobody"), vec![]);
        let err = real.check(Path::new("/Library/Fonts"), &RemovalContext::default()).unwrap_err();
        assert_eq!(err, SafetyViolation::ProtectedLocation("/Library".into()));
    }

    #[test]
    fn symlink_escape() {
        let f = fixture();
        // A file reached through a symlinked folder that points outside the scan root.
        let err = f.policy.check(&f.root.join("scan/escape/secret.txt"), &ctx(&f.root)).unwrap_err();
        assert_eq!(err, SafetyViolation::SymlinkEscape);
        // The symlink itself is inside the root: removing it is fine and never touches the target.
        assert!(f.policy.check(&f.root.join("scan/link.txt"), &ctx(&f.root)).is_ok());
    }

    #[test]
    fn outside_scan_root() {
        let f = fixture();
        let err = f.policy.check(&f.root.join("outside/secret.txt"), &ctx(&f.root)).unwrap_err();
        assert_eq!(err, SafetyViolation::OutsideScanRoot);
    }

    #[test]
    fn root_protection() {
        let f = fixture();
        let err = f.policy.check(&f.root.join("scan"), &RemovalContext::default()).unwrap_err();
        assert!(matches!(err, SafetyViolation::EssentialFolder(_)));
        let real = SafetyPolicy::system(Path::new("/Users/nobody"), vec![]);
        assert!(real.check(Path::new("/"), &RemovalContext::default()).is_err());
        assert!(is_volume_root(Path::new("/Volumes/Backup")));
        assert!(!is_volume_root(Path::new("/Volumes/Backup/file")));
    }

    #[test]
    fn own_files_and_ignore_list() {
        let mut f = fixture();
        let err = f.policy.check(&f.root.join("app-data/db.sqlite"), &RemovalContext::default()).unwrap_err();
        assert_eq!(err, SafetyViolation::OwnFiles);
        f.policy.set_ignored(vec![f.root.join("scan/docs")]);
        let err = f.policy.check(&f.root.join("scan/docs/a.txt"), &ctx(&f.root)).unwrap_err();
        assert_eq!(err, SafetyViolation::Ignored);
    }

    #[test]
    fn relative_and_missing() {
        let f = fixture();
        assert_eq!(f.policy.check(Path::new("docs/a.txt"), &RemovalContext::default()).unwrap_err(), SafetyViolation::NotAbsolute);
        let dotted = f.root.join("scan/docs/../docs/a.txt");
        assert_eq!(f.policy.check(&dotted, &RemovalContext::default()).unwrap_err(), SafetyViolation::NotAbsolute);
        assert_eq!(f.policy.check(&f.root.join("scan/nope"), &RemovalContext::default()).unwrap_err(), SafetyViolation::Missing);
    }

    #[test]
    fn mutation_applies_the_same_rules_as_removal() {
        let mut f = fixture();
        let any = RemovalContext::default();
        let existing = MutationKind::ExistingItem;
        // Every rule that blocks removal blocks the mutation too.
        for (p, expected) in [
            (f.root.join("system/core/kernel"), "protected"),
            (f.root.join("app-data/db.sqlite"), "own"),
            (f.root.join("scan"), "essential"),
        ] {
            assert!(f.policy.check_mutation(&p, &any, existing).is_err(), "{expected}");
            assert!(f.policy.check(&p, &any).is_err(), "{expected}");
        }
        assert_eq!(
            f.policy.check_mutation(&f.root.join("scan/escape/secret.txt"), &ctx(&f.root), existing).unwrap_err(),
            SafetyViolation::SymlinkEscape
        );
        assert_eq!(
            f.policy.check_mutation(&f.root.join("scan/nope"), &any, existing).unwrap_err(),
            SafetyViolation::Missing
        );
        assert_eq!(
            f.policy.check_mutation(Path::new("docs/a.txt"), &any, MutationKind::CreateFile).unwrap_err(),
            SafetyViolation::NotAbsolute
        );
        f.policy.set_ignored(vec![f.root.join("scan/docs")]);
        assert_eq!(
            f.policy.check_mutation(&f.root.join("scan/docs/a.txt"), &ctx(&f.root), existing).unwrap_err(),
            SafetyViolation::Ignored
        );
    }

    #[test]
    fn create_file_only_waives_existence() {
        let f = fixture();
        let new = f.root.join("scan/docs/.metadata_never_index");
        assert_eq!(f.policy.check(&new, &RemovalContext::default()).unwrap_err(), SafetyViolation::Missing);
        assert_eq!(f.policy.check_mutation(&new, &ctx(&f.root), MutationKind::CreateFile).unwrap(), new);
        // Still blocked inside protected, own or escaping locations, and when the parent is missing.
        assert!(f.policy.check_mutation(&f.root.join("system/core/new"), &RemovalContext::default(), MutationKind::CreateFile).is_err());
        assert!(f.policy.check_mutation(&f.root.join("app-data/new"), &RemovalContext::default(), MutationKind::CreateFile).is_err());
        assert_eq!(
            f.policy.check_mutation(&f.root.join("scan/escape/new"), &ctx(&f.root), MutationKind::CreateFile).unwrap_err(),
            SafetyViolation::SymlinkEscape
        );
        assert_eq!(
            f.policy.check_mutation(&f.root.join("scan/missing-dir/new"), &ctx(&f.root), MutationKind::CreateFile).unwrap_err(),
            SafetyViolation::Missing
        );
    }

    #[test]
    fn read_only_volume() {
        // The sealed system volume is always mounted read-only.
        assert!(is_read_only(Path::new("/System")));
    }
}
