//! Looks for secrets before anything is snapshotted or pushed. It only ever reports a path and a
//! kind: no line number, no excerpt, no value. File contents are matched in memory and dropped, and
//! media, archives and binaries are never read.

use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use rayon::prelude::*;
use regex::{RegexSet, RegexSetBuilder};
use serde::Serialize;

use crate::dev::git::Git;
use crate::error::AppResult;
use crate::filesystem::metadata::{category_for, extension_of, FileCategory};
use crate::filesystem::worker_pool;

/// Largest file whose content is looked at.
pub const MAX_CONTENT_BYTES: u64 = 1024 * 1024;
const SNIFF_BYTES: usize = 8192;
const MAX_FINDINGS: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum SecretKind {
    EnvFile,
    PrivateKeyFile,
    CredentialsFile,
    PrivateKeyBlock,
    AwsAccessKey,
    GithubToken,
    ApiKey,
    SlackToken,
    StripeKey,
    GoogleApiKey,
    Jwt,
    NpmToken,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Source {
    /// Decided from the file name alone.
    Name,
    /// A pattern matched inside a text file.
    Content,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretFinding {
    /// Repository-relative.
    pub path: String,
    pub kind: SecretKind,
    pub source: Source,
    /// Already in the repository. Untracked ones are the new exposure a snapshot would create.
    pub tracked: bool,
}

#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SecretScan {
    pub findings: Vec<SecretFinding>,
    pub files_checked: u32,
    /// Skipped on purpose: media, archives, binaries and files above [`MAX_CONTENT_BYTES`].
    pub content_skipped: u32,
    pub unreadable: u32,
    /// More findings existed than are listed.
    pub truncated: bool,
}

const PATTERNS: &[(SecretKind, &str)] = &[
    (SecretKind::PrivateKeyBlock, r"-----BEGIN (?:RSA |EC |DSA |OPENSSH |PGP |ENCRYPTED )?PRIVATE KEY(?: BLOCK)?-----"),
    (SecretKind::AwsAccessKey, r"\b(?:AKIA|ASIA)[0-9A-Z]{16}\b"),
    (SecretKind::GithubToken, r"\bgh[pousr]_[A-Za-z0-9]{36,}\b|\bgithub_pat_[A-Za-z0-9_]{22,}\b"),
    (SecretKind::ApiKey, r"\bsk-[A-Za-z0-9_-]{20,}\b"),
    (SecretKind::SlackToken, r"\bxox[abprs]-[A-Za-z0-9-]{10,}\b"),
    (SecretKind::StripeKey, r"\b[sr]k_live_[A-Za-z0-9]{16,}\b"),
    (SecretKind::GoogleApiKey, r"\bAIza[0-9A-Za-z_-]{35}\b"),
    (SecretKind::Jwt, r"\beyJ[A-Za-z0-9_-]{10,}\.eyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\b"),
    // `_authToken=${NPM_TOKEN}` is a reference to the environment, not a secret.
    (SecretKind::NpmToken, r"_authToken\s*=\s*[^\s$\x22'][^\s]{7,}"),
];

fn matcher() -> RegexSet {
    RegexSetBuilder::new(PATTERNS.iter().map(|(_, p)| *p)).size_limit(10 << 20).build().expect("secret patterns compile")
}

/// Classification by file name only. Template files such as `.env.example` are not secrets.
pub fn kind_for_name(path: &str) -> Option<SecretKind> {
    let name = path.rsplit('/').next().unwrap_or(path).to_lowercase();
    let in_aws_dir = path.split('/').rev().nth(1) == Some(".aws");
    if name == ".env" || (name.starts_with(".env.") && !is_template(&name)) || name.ends_with(".env") && name.len() > 4 && !is_template(&name) {
        return Some(SecretKind::EnvFile);
    }
    if matches!(name.as_str(), "id_rsa" | "id_dsa" | "id_ecdsa" | "id_ed25519")
        || [".p12", ".pfx", ".jks", ".keystore", ".ppk", ".p8"].iter().any(|e| name.ends_with(e))
    {
        return Some(SecretKind::PrivateKeyFile);
    }
    if matches!(name.as_str(), ".netrc" | ".pgpass" | ".htpasswd" | ".git-credentials" | ".dockercfg" | "credentials.json" | "secrets.json" | "secrets.yml" | "secrets.yaml")
        || name.starts_with("service-account") && name.ends_with(".json")
        || name.ends_with(".tfstate")
        || (name == "credentials" && in_aws_dir)
    {
        return Some(SecretKind::CredentialsFile);
    }
    None
}

fn is_template(name: &str) -> bool {
    ["example", "sample", "template", "dist", "defaults"].iter().any(|t| name.ends_with(&format!(".{t}")))
}

fn content_is_off_limits(path: &Path) -> bool {
    let skipped_category = matches!(
        category_for(path),
        FileCategory::Image | FileCategory::Video | FileCategory::Audio | FileCategory::Archive | FileCategory::DiskImage | FileCategory::Installer | FileCategory::Application
    );
    let ext = extension_of(path).unwrap_or_default();
    skipped_category
        || matches!(
            ext.as_str(),
            "pdf" | "doc" | "docx" | "xls" | "xlsx" | "ppt" | "pptx" | "pages" | "numbers" | "epub" | "ttf" | "otf" | "woff" | "woff2" | "wasm"
                | "sqlite" | "sqlite3" | "db" | "class" | "jar" | "war" | "so" | "dylib" | "a" | "o" | "pyc" | "bin" | "exe" | "dll" | "car"
                | "p12" | "pfx" | "jks" | "keystore"
        )
}

enum Outcome {
    Findings(Vec<SecretFinding>),
    ContentSkipped(Vec<SecretFinding>),
    Unreadable(Vec<SecretFinding>),
}

fn inspect(repo: &Path, rel: &str, tracked: bool, set: &RegexSet) -> Outcome {
    let mut found = Vec::new();
    if let Some(kind) = kind_for_name(rel) {
        found.push(SecretFinding { path: rel.to_string(), kind, source: Source::Name, tracked });
    }
    let path = repo.join(rel);
    // Symlinks are never followed; deleted tracked files and gitlinks are simply not files.
    let Ok(meta) = std::fs::symlink_metadata(&path) else { return Outcome::Findings(found) };
    if !meta.is_file() {
        return Outcome::Findings(found);
    }
    if meta.len() > MAX_CONTENT_BYTES || content_is_off_limits(&path) {
        return Outcome::ContentSkipped(found);
    }
    let mut buf = Vec::with_capacity(meta.len() as usize);
    let Ok(file) = File::open(&path) else { return Outcome::Unreadable(found) };
    if file.take(MAX_CONTENT_BYTES).read_to_end(&mut buf).is_err() {
        return Outcome::Unreadable(found);
    }
    if buf[..buf.len().min(SNIFF_BYTES)].contains(&0) {
        return Outcome::ContentSkipped(found); // binary
    }
    let text = String::from_utf8_lossy(&buf);
    for idx in set.matches(&text) {
        let kind = PATTERNS[idx].0;
        // The name already told us the file is sensitive; do not list the same thing twice.
        if !found.iter().any(|f| f.kind == kind) {
            found.push(SecretFinding { path: rel.to_string(), kind, source: Source::Content, tracked });
        }
    }
    Outcome::Findings(found)
}

/// Checks `files` (repository-relative, with whether each is tracked). Reads nothing outside `repo`.
pub fn scan_files(repo: &Path, files: &[(String, bool)], cancel: &AtomicBool) -> SecretScan {
    let set = matcher();
    let outcomes: Vec<Option<Outcome>> = worker_pool().install(|| {
        files
            .par_iter()
            .map(|(rel, tracked)| (!cancel.load(Ordering::Relaxed)).then(|| inspect(repo, rel, *tracked, &set)))
            .collect()
    });
    let mut scan = SecretScan::default();
    for outcome in outcomes.into_iter().flatten() {
        scan.files_checked += 1;
        let found = match outcome {
            Outcome::Findings(f) => f,
            Outcome::ContentSkipped(f) => {
                scan.content_skipped += 1;
                f
            }
            Outcome::Unreadable(f) => {
                scan.unreadable += 1;
                f
            }
        };
        scan.findings.extend(found);
    }
    scan.findings.sort_by(|a, b| a.path.cmp(&b.path).then((a.kind as u8).cmp(&(b.kind as u8))));
    if scan.findings.len() > MAX_FINDINGS {
        scan.findings.truncate(MAX_FINDINGS);
        scan.truncated = true;
    }
    scan
}

/// Scans exactly what a snapshot would contain: tracked files plus untracked ones git does not ignore.
pub fn scan_repo(git: &Git, repo: &Path, cancel: &AtomicBool) -> AppResult<SecretScan> {
    let (tracked, untracked) = git.snapshot_files(repo)?;
    let files: Vec<(String, bool)> = tracked.into_iter().map(|p| (p, true)).chain(untracked.into_iter().map(|p| (p, false))).collect();
    Ok(scan_files(repo, &files, cancel))
}

/// `path` is one of the excluded paths or lives inside one of them.
pub fn is_excluded(path: &str, excludes: &[String]) -> bool {
    excludes.iter().any(|e| path == e || path.starts_with(&format!("{}/", e.trim_end_matches('/'))))
}

/// Findings that are not covered by the paths the user chose to leave out of the snapshot.
pub fn unresolved<'a>(findings: &'a [SecretFinding], excludes: &[String]) -> Vec<&'a SecretFinding> {
    findings.iter().filter(|f| !is_excluded(&f.path, excludes)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        for (p, k) in [
            (".env", SecretKind::EnvFile),
            ("app/.env.local", SecretKind::EnvFile),
            ("app/prod.env", SecretKind::EnvFile),
            ("keys/id_ed25519", SecretKind::PrivateKeyFile),
            ("ios/AuthKey.p8", SecretKind::PrivateKeyFile),
            ("release.keystore", SecretKind::PrivateKeyFile),
            (".netrc", SecretKind::CredentialsFile),
            ("infra/terraform.tfstate", SecretKind::CredentialsFile),
            (".aws/credentials", SecretKind::CredentialsFile),
            ("gcp/service-account-prod.json", SecretKind::CredentialsFile),
        ] {
            assert_eq!(kind_for_name(p), Some(k), "{p}");
        }
        for p in [".env.example", ".env.sample", ".env.template", "id_rsa.pub", "src/environment.ts", "credentials", "docs/env.md", "README.md"] {
            assert_eq!(kind_for_name(p), None, "{p}");
        }
    }

    #[test]
    fn patterns_match_synthetic_tokens_and_not_lookalikes() {
        let set = matcher();
        let hits = |s: &str| -> Vec<SecretKind> { set.matches(s).into_iter().map(|i| PATTERNS[i].0).collect() };
        // Assembled at run time so this file does not trip its own scanner.
        let join = |parts: &[&str]| parts.concat();
        assert_eq!(hits(&join(&["key = AK", "IAABCDEFGHIJKLMNOP"])), [SecretKind::AwsAccessKey]);
        assert_eq!(hits(&join(&["t = gh", "p_", &"a1".repeat(18)])), [SecretKind::GithubToken]);
        assert_eq!(hits(&join(&["-----BEGIN OPENSSH ", "PRIVATE KEY-----"])), [SecretKind::PrivateKeyBlock]);
        assert_eq!(hits(&join(&["//registry.npmjs.org/:_auth", "Token=abcd1234efgh"])), [SecretKind::NpmToken]);
        assert_eq!(hits(&join(&["s", "k-ant-api03-ABCDEFGHIJKLMNOPQRSTUVWXYZ"])), [SecretKind::ApiKey]);
        assert!(hits(&join(&["//registry.npmjs.org/:_auth", "Token=${NPM_TOKEN}"])).is_empty());
        assert!(hits("-----BEGIN CERTIFICATE-----").is_empty());
        assert!(hits("AKIA is a prefix; sk-short; ghp_tooshort").is_empty());
        assert!(hits("integrity sha512-AbCdEfGhIjKlMnOpQrStUvWxYz0123456789==").is_empty());
    }

    #[test]
    fn unresolved_respects_exclusions() {
        let f = |p: &str| SecretFinding { path: p.into(), kind: SecretKind::EnvFile, source: Source::Name, tracked: false };
        let all = [f(".env"), f("app/.env.local"), f("app2/.env")];
        let left = unresolved(&all, &[".env".into(), "app".into()]);
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].path, "app2/.env");
        assert_eq!(unresolved(&all, &[]).len(), 3);
    }
}
