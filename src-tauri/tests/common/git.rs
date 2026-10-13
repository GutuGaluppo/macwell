//! Real git repositories in temp folders, isolated from the user's configuration.
#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub fn canonical_tmp() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let root = fs::canonicalize(dir.path()).unwrap();
    (dir, root)
}

/// Runs git with no global or system config and a fixed identity and date. Panics on failure.
pub fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("/usr/bin/git")
        .current_dir(dir)
        .args(args)
        .env_clear()
        .env("PATH", "/usr/bin:/bin")
        .env("HOME", dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.invalid")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.invalid")
        .env("GIT_AUTHOR_DATE", "2024-01-02T03:04:05Z")
        .env("GIT_COMMITTER_DATE", "2024-01-02T03:04:05Z")
        .output()
        .expect("git runs");
    assert!(out.status.success(), "git {args:?} failed: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

pub fn init(dir: &Path) {
    fs::create_dir_all(dir).unwrap();
    git(dir, &["init", "-q", "-b", "main"]);
}

pub fn write(dir: &Path, name: &str, content: &str) {
    let p = dir.join(name);
    fs::create_dir_all(p.parent().unwrap()).unwrap();
    fs::write(p, content).unwrap();
}

pub fn commit_file(dir: &Path, name: &str, content: &str) {
    write(dir, name, content);
    git(dir, &["add", name]);
    git(dir, &["commit", "-q", "-m", &format!("add {name}")]);
}

/// A repository with one commit.
pub fn repo_with_commit(dir: &Path) {
    init(dir);
    commit_file(dir, "README.md", "hello\n");
}

/// The real runner with the user's global and system git configuration switched off, so results do
/// not depend on who runs the tests (a global Git LFS setup, commit signing, aliases…).
pub struct Isolated;

impl organizemymac_lib::exec::CommandRunner for Isolated {
    fn run(&self, cmd: &organizemymac_lib::exec::Cmd) -> organizemymac_lib::error::AppResult<organizemymac_lib::exec::CmdOutput> {
        let mut cmd = cmd.clone();
        cmd.env.push(("GIT_CONFIG_GLOBAL".into(), "/dev/null".into()));
        cmd.env.push(("GIT_CONFIG_SYSTEM".into(), "/dev/null".into()));
        organizemymac_lib::exec::RealRunner.run(&cmd)
    }
}
