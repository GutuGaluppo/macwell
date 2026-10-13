//! Homebrew, read only: which packages are outdated, how risky each upgrade is, and which of the user's
//! projects probably depend on it.
//!
//! The only command ever run is `brew outdated --json=v2`, with auto-update and analytics switched off,
//! so nothing is updated, downloaded or reported. The app never runs `brew upgrade`: it shows the
//! command and the user decides.

pub mod projects;

use serde::{Deserialize, Serialize};

use crate::exec::{Cmd, CommandRunner};

pub const BREW_PATHS: &[&str] = &["/opt/homebrew/bin/brew", "/usr/local/bin/brew"];

/// Where Homebrew is installed, if it is.
pub fn find_brew() -> Option<&'static str> {
    BREW_PATHS.iter().copied().find(|p| std::path::Path::new(p).is_file())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PackageKind {
    Formula,
    Cask,
}

/// How far the new version is from the installed one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Change {
    Major,
    Minor,
    Patch,
    /// The versions do not follow a pattern we can compare.
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Risk {
    /// `brew upgrade` skips it: the user pinned it.
    Pinned,
    Low,
    Medium,
    High,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Outdated {
    pub name: String,
    pub kind: PackageKind,
    pub installed: Vec<String>,
    pub current: String,
    pub pinned: bool,
    pub change: Change,
    pub risk: Risk,
    /// A language runtime, database or toolchain: projects tend to depend on its exact version.
    pub runtime: bool,
    /// For the user to run; the app never does.
    pub upgrade_command: String,
}

#[derive(Debug, Deserialize)]
struct Raw {
    #[serde(default)]
    formulae: Vec<RawPackage>,
    #[serde(default)]
    casks: Vec<RawPackage>,
}

#[derive(Debug, Deserialize)]
struct RawPackage {
    name: String,
    /// A list for formulae, a single string for casks in some versions.
    #[serde(default)]
    installed_versions: serde_json::Value,
    #[serde(default)]
    current_version: String,
    #[serde(default)]
    pinned: bool,
}

fn versions(v: &serde_json::Value) -> Vec<String> {
    match v {
        serde_json::Value::String(s) => vec![s.clone()],
        serde_json::Value::Array(a) => a.iter().filter_map(|x| x.as_str().map(String::from)).collect(),
        _ => Vec::new(),
    }
}

/// Numeric parts of a version: `v1.2.3_1` → `[1, 2, 3, 1]`. Letters are ignored, so `1.2.3-beta` compares as `1.2.3`.
fn parts(version: &str) -> Vec<u64> {
    version.split(|c: char| !c.is_ascii_digit()).filter(|p| !p.is_empty()).filter_map(|p| p.parse().ok()).collect()
}

/// How far `current` is from the newest installed version.
pub fn classify_change(installed: &[String], current: &str) -> Change {
    let cur = parts(current);
    let Some(old) = installed.iter().map(|v| parts(v)).max() else { return Change::Other };
    if cur.is_empty() || old.is_empty() {
        return Change::Other;
    }
    for i in 0..cur.len().max(old.len()) {
        let (a, b) = (old.get(i).copied().unwrap_or(0), cur.get(i).copied().unwrap_or(0));
        if a != b {
            return match i {
                0 => Change::Major,
                1 => Change::Minor,
                _ => Change::Patch,
            };
        }
    }
    Change::Other // same numbers: only a revision or a label changed
}

const RUNTIMES: &[&str] = &[
    "node", "python", "ruby", "go", "rust", "rustup", "php", "perl", "openjdk", "postgresql", "mysql", "mariadb", "redis", "mongodb-community", "sqlite", "nginx", "llvm", "cmake", "gcc", "docker", "terraform",
];

pub fn is_runtime(name: &str) -> bool {
    let base = name.split('@').next().unwrap_or(name);
    RUNTIMES.contains(&base)
}

/// Risk of upgrading: how big the jump is, raised for runtimes (projects depend on their version) and
/// capped for apps (casks), which no project links against.
pub fn risk_of(kind: PackageKind, change: Change, runtime: bool, pinned: bool) -> Risk {
    if pinned {
        return Risk::Pinned;
    }
    let base = match (kind, change) {
        (PackageKind::Cask, Change::Major) => Risk::Medium,
        (PackageKind::Cask, _) => Risk::Low,
        (_, Change::Major) => Risk::High,
        (_, Change::Minor) => Risk::Medium,
        (_, Change::Other) => Risk::Medium, // cannot tell how big the jump is
        (_, Change::Patch) => Risk::Low,
    };
    if runtime && kind == PackageKind::Formula && base == Risk::Medium { Risk::High } else { base }
}

/// `brew outdated --json=v2`.
pub fn parse_outdated(json: &str) -> Result<Vec<Outdated>, String> {
    let raw: Raw = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let mut out: Vec<Outdated> = raw
        .formulae
        .into_iter()
        .map(|p| (PackageKind::Formula, p))
        .chain(raw.casks.into_iter().map(|p| (PackageKind::Cask, p)))
        .map(|(kind, p)| {
            let installed = versions(&p.installed_versions);
            let change = classify_change(&installed, &p.current_version);
            let runtime = kind == PackageKind::Formula && is_runtime(&p.name);
            Outdated {
                risk: risk_of(kind, change, runtime, p.pinned),
                upgrade_command: format!("brew upgrade{} {}", if kind == PackageKind::Cask { " --cask" } else { "" }, p.name),
                name: p.name,
                kind,
                installed,
                current: p.current_version,
                pinned: p.pinned,
                change,
                runtime,
            }
        })
        .collect();
    // Riskiest first, then by name.
    out.sort_by(|a, b| b.risk.cmp(&a.risk).then_with(|| a.name.cmp(&b.name)));
    Ok(out)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum BrewError {
    NotInstalled,
    Failed { message: String },
}

/// Runs the one command this module is allowed to run.
pub fn read_outdated(runner: &dyn CommandRunner, brew: &str) -> Result<Vec<Outdated>, BrewError> {
    let cmd = Cmd::new(brew, &["outdated", "--json=v2"])
        .env("HOMEBREW_NO_AUTO_UPDATE", "1")
        .env("HOMEBREW_NO_ANALYTICS", "1")
        .env("HOMEBREW_NO_ENV_HINTS", "1")
        .timeout(std::time::Duration::from_secs(120));
    let out = runner.run(&cmd).map_err(|e| BrewError::Failed { message: e.to_string() })?;
    if out.timed_out {
        return Err(BrewError::Failed { message: "o Homebrew demorou demais para responder".into() });
    }
    if !out.success() {
        let err = out.stderr_str();
        return Err(BrewError::Failed { message: err.lines().find(|l| !l.trim().is_empty()).unwrap_or("o Homebrew falhou").trim().chars().take(200).collect() });
    }
    parse_outdated(&out.stdout_str()).map_err(|e| BrewError::Failed { message: format!("resposta inesperada do Homebrew: {e}") })
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Package {
    #[serde(flatten)]
    pub outdated: Outdated,
    /// Projects that probably use it (exact when a `Brewfile` lists it).
    pub projects: Vec<projects::ProjectUse>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HomebrewReport {
    pub installed: bool,
    pub packages: Vec<Package>,
    pub error: Option<String>,
    pub repos_scanned: u32,
}

/// The whole analysis: outdated packages, riskiest first, each with the projects that probably use it.
/// `brew` is `None` when Homebrew is not installed. Reads only.
pub fn analyze(runner: &dyn CommandRunner, brew: Option<&str>, repos: &[std::path::PathBuf]) -> HomebrewReport {
    let Some(brew) = brew else { return HomebrewReport { installed: false, packages: vec![], error: None, repos_scanned: 0 } };
    match read_outdated(runner, brew) {
        Err(e) => HomebrewReport {
            installed: true,
            packages: vec![],
            error: Some(match e {
                BrewError::NotInstalled => "o Homebrew não está instalado".to_string(),
                BrewError::Failed { message } => message,
            }),
            repos_scanned: 0,
        },
        Ok(outdated) => {
            let index = projects::scan(repos);
            let packages = outdated.into_iter().map(|o| Package { projects: index.uses(&o.name, o.kind), outdated: o }).collect();
            HomebrewReport { installed: true, packages, error: None, repos_scanned: repos.len() as u32 }
        }
    }
}
