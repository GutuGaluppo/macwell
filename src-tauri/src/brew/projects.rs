//! Which of the user's projects probably use a Homebrew package.
//!
//! A `Brewfile` is exact: it lists what the project needs. Everything else is an estimate from the files
//! that identify a project's toolchain (`package.json` suggests `node`, `Cargo.toml` suggests `rust`),
//! and the screen says so. Only file names are looked at, plus the text of a `Brewfile` and a
//! `.tool-versions`; symbolic links are never followed.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;

use super::PackageKind;
use crate::dev::discover::PRUNED;

const MAX_DEPTH: usize = 3;
const MAX_TEXT: u64 = 256 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectUse {
    pub repo: PathBuf,
    /// The file that points to the package, relative to the repository.
    pub via: String,
    /// Listed in a `Brewfile`: not a guess.
    pub exact: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Signal {
    Node,
    Pnpm,
    Yarn,
    Bun,
    Python,
    Rust,
    Go,
    Ruby,
    CocoaPods,
    Java,
    Docker,
    Terraform,
}

fn signal_of_file(name: &str) -> Option<Signal> {
    Some(match name {
        "package.json" | ".nvmrc" | ".node-version" => Signal::Node,
        "pnpm-lock.yaml" | "pnpm-workspace.yaml" => Signal::Pnpm,
        "yarn.lock" => Signal::Yarn,
        "bun.lock" | "bun.lockb" => Signal::Bun,
        "pyproject.toml" | "requirements.txt" | "Pipfile" | "setup.py" | ".python-version" => Signal::Python,
        "Cargo.toml" => Signal::Rust,
        "go.mod" => Signal::Go,
        "Gemfile" | ".ruby-version" => Signal::Ruby,
        "Podfile" => Signal::CocoaPods,
        "pom.xml" | "build.gradle" | "build.gradle.kts" => Signal::Java,
        "Dockerfile" | "docker-compose.yml" | "docker-compose.yaml" | "compose.yaml" => Signal::Docker,
        n if n.ends_with(".tf") => Signal::Terraform,
        _ => return None,
    })
}

fn tool_signal(tool: &str) -> Option<Signal> {
    Some(match tool {
        "nodejs" | "node" => Signal::Node,
        "python" => Signal::Python,
        "ruby" => Signal::Ruby,
        "golang" | "go" => Signal::Go,
        "rust" => Signal::Rust,
        "java" => Signal::Java,
        "terraform" => Signal::Terraform,
        _ => return None,
    })
}

fn matches_signal(signal: Signal, name: &str, kind: PackageKind) -> bool {
    let base = name.split('@').next().unwrap_or(name);
    match signal {
        Signal::Node => kind == PackageKind::Formula && base == "node",
        Signal::Pnpm => base == "pnpm",
        Signal::Yarn => base == "yarn",
        Signal::Bun => base == "bun",
        Signal::Python => kind == PackageKind::Formula && base == "python",
        Signal::Rust => matches!(name, "rust" | "rustup"),
        Signal::Go => name == "go",
        Signal::Ruby => kind == PackageKind::Formula && base == "ruby",
        Signal::CocoaPods => name == "cocoapods",
        Signal::Java => matches!(base, "openjdk" | "maven" | "gradle"),
        Signal::Docker => matches!(name, "docker" | "docker-compose" | "docker-desktop"),
        Signal::Terraform => name == "terraform",
    }
}

#[derive(Default)]
pub struct ProjectIndex {
    signals: Vec<(PathBuf, Signal, String)>,
    brewfile: Vec<(PathBuf, String, PackageKind, String)>,
}

/// Looks for toolchain files in each repository, a few folders deep.
pub fn scan(repos: &[PathBuf]) -> ProjectIndex {
    let mut index = ProjectIndex::default();
    for repo in repos {
        walk(repo, repo, 0, &mut index);
    }
    index
}

fn read_small(path: &Path) -> Option<String> {
    let meta = fs::symlink_metadata(path).ok()?;
    if !meta.is_file() || meta.len() > MAX_TEXT {
        return None;
    }
    fs::read_to_string(path).ok()
}

fn walk(repo: &Path, dir: &Path, depth: usize, index: &mut ProjectIndex) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    let rel = |p: &Path| p.strip_prefix(repo).map(|r| r.to_string_lossy().into_owned()).unwrap_or_default();
    let mut subdirs = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Ok(kind) = entry.file_type() else { continue }; // does not follow symlinks
        if kind.is_dir() {
            if depth < MAX_DEPTH && !PRUNED.contains(&name.as_str()) && fs::symlink_metadata(entry.path().join(".git")).is_err() {
                subdirs.push(entry.path());
            }
            continue;
        }
        if !kind.is_file() {
            continue;
        }
        if let Some(sig) = signal_of_file(&name) {
            index.signals.push((repo.to_path_buf(), sig, rel(&entry.path())));
        }
        if name == "Brewfile" {
            for (pkg, k) in parse_brewfile(&read_small(&entry.path()).unwrap_or_default()) {
                index.brewfile.push((repo.to_path_buf(), pkg, k, rel(&entry.path())));
            }
        }
        if name == ".tool-versions" {
            for line in read_small(&entry.path()).unwrap_or_default().lines() {
                if let Some(sig) = line.split_whitespace().next().and_then(tool_signal) {
                    index.signals.push((repo.to_path_buf(), sig, rel(&entry.path())));
                }
            }
        }
    }
    subdirs.sort();
    for sub in subdirs {
        walk(repo, &sub, depth + 1, index);
    }
}

/// `brew "name"` and `cask "name"` lines. A tap prefix (`user/tap/name`) is dropped.
pub fn parse_brewfile(text: &str) -> Vec<(String, PackageKind)> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            let (kind, rest) = if let Some(r) = line.strip_prefix("brew ") {
                (PackageKind::Formula, r)
            } else if let Some(r) = line.strip_prefix("cask ") {
                (PackageKind::Cask, r)
            } else {
                return None;
            };
            let quote = rest.trim_start().chars().next().filter(|c| *c == '"' || *c == '\'')?;
            let name = rest.trim_start()[1..].split(quote).next()?;
            Some((name.rsplit('/').next().unwrap_or(name).to_string(), kind))
        })
        .collect()
}

impl ProjectIndex {
    /// Projects that use `name`, one entry per repository, exact ones first.
    pub fn uses(&self, name: &str, kind: PackageKind) -> Vec<ProjectUse> {
        let mut out: Vec<ProjectUse> = Vec::new();
        let mut seen: HashSet<&Path> = HashSet::new();
        for (repo, pkg, k, via) in &self.brewfile {
            if pkg == name && *k == kind && seen.insert(repo) {
                out.push(ProjectUse { repo: repo.clone(), via: via.clone(), exact: true });
            }
        }
        for (repo, sig, via) in &self.signals {
            if matches_signal(*sig, name, kind) && seen.insert(repo) {
                out.push(ProjectUse { repo: repo.clone(), via: via.clone(), exact: false });
            }
        }
        out.sort_by(|a, b| b.exact.cmp(&a.exact).then_with(|| a.repo.cmp(&b.repo)));
        out
    }
}
