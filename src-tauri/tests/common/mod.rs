//! Synthetic filesystem fixtures (spec §31): duplicates, hard links, symlinks, huge sparse files,
//! zero-byte files, permission-denied folders, deep trees, unicode and hidden names.
#![allow(dead_code)]

pub mod git;

use std::fs::{self, File};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

pub struct Fixture {
    pub dir: tempfile::TempDir,
    pub root: PathBuf,
}

impl Drop for Fixture {
    fn drop(&mut self) {
        // Restore permissions so the temp dir can be removed.
        let _ = fs::set_permissions(self.root.join("locked"), fs::Permissions::from_mode(0o755));
    }
}

pub const SPARSE_SIZE: u64 = 5_000_000_000;
pub const DEEP_LEVELS: usize = 60;

pub fn write(path: &Path, bytes: &[u8]) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
}

/// Deterministic pseudo-random content.
pub fn content(seed: u64, len: usize) -> Vec<u8> {
    let mut x = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1;
    (0..len)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x as u8
        })
        .collect()
}

pub fn build() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap().join("fixture");
    fs::create_dir_all(&root).unwrap();

    // Duplicates: two identical 300 KB files, a third copy elsewhere, and a same-size different file.
    let dup = content(1, 300_000);
    write(&root.join("dups/a/photo.jpg"), &dup);
    write(&root.join("dups/b/photo copy.jpg"), &dup);
    write(&root.join("dups/c/third.jpg"), &dup);
    let mut near = dup.clone();
    near[150_000] ^= 0xFF; // differs only in the middle
    write(&root.join("dups/d/near.jpg"), &near);
    let mut tail = dup.clone();
    *tail.last_mut().unwrap() ^= 0xFF; // differs only in the last byte
    write(&root.join("dups/d/tail.jpg"), &tail);

    // Hard links: one inode, three names.
    write(&root.join("links/original.bin"), &content(2, 100_000));
    fs::hard_link(root.join("links/original.bin"), root.join("links/hard1.bin")).unwrap();
    fs::hard_link(root.join("links/original.bin"), root.join("links/hard2.bin")).unwrap();

    // Symlinks: to a file, to a folder outside, and a loop.
    std::os::unix::fs::symlink(root.join("dups/a/photo.jpg"), root.join("links/to-photo")).unwrap();
    std::os::unix::fs::symlink("/usr", root.join("links/to-usr")).unwrap();
    std::os::unix::fs::symlink(root.join("links"), root.join("links/loop")).unwrap();

    // Huge sparse file: 5 GB logical, almost nothing allocated.
    let sparse = File::create(root.join("big/sparse.img")).or_else(|_| {
        fs::create_dir_all(root.join("big")).unwrap();
        File::create(root.join("big/sparse.img"))
    }).unwrap();
    sparse.set_len(SPARSE_SIZE).unwrap();
    write(&root.join("big/video.mov"), &content(3, 2_000_000));
    write(&root.join("big/installer.dmg"), &content(4, 1_500_000));

    // Zero-byte files (all the same size, but they are never duplicates worth reporting).
    for i in 0..3 {
        write(&root.join(format!("empty/zero{i}.txt")), b"");
    }

    // Unicode and hidden names.
    write(&root.join("unicode/relatório — ação ✓.txt"), b"unicode");
    write(&root.join("unicode/日本語/ファイル.txt"), b"nihongo");
    write(&root.join("hidden/.secret"), b"hidden");
    write(&root.join(".hidden-dir/file.txt"), b"in hidden dir");

    // Deep tree.
    let mut deep = root.join("deep");
    for i in 0..DEEP_LEVELS {
        deep.push(format!("l{i}"));
    }
    write(&deep.join("leaf.txt"), b"leaf");

    // Permission-denied folder.
    write(&root.join("locked/inside.txt"), b"locked");
    fs::set_permissions(root.join("locked"), fs::Permissions::from_mode(0o000)).unwrap();

    Fixture { dir, root }
}
