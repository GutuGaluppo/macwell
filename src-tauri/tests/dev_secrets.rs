mod common;

use std::fs;
use std::os::unix::fs::symlink;
use std::path::Path;
use std::sync::atomic::AtomicBool;

use common::git::{canonical_tmp, commit_file, git, repo_with_commit, write};
use organizemymac_lib::dev::git::Git;
use organizemymac_lib::dev::secrets::{scan_repo, unresolved, SecretKind, SecretScan, Source};
use organizemymac_lib::exec::RealRunner;

// Built at run time so this source file holds nothing that looks like a credential.
fn aws() -> String {
    format!("AKIA{}", "ABCDEFGHIJKLMNOP")
}
fn pem_header() -> String {
    format!("-----BEGIN {} PRIVATE KEY-----", "RSA")
}
fn github() -> String {
    format!("ghp_{}", "x9".repeat(18))
}

fn scan(repo: &Path) -> SecretScan {
    scan_repo(&Git::new(&RealRunner), repo, &AtomicBool::new(false)).unwrap()
}

fn kinds(s: &SecretScan) -> Vec<(String, SecretKind, Source, bool)> {
    s.findings.iter().map(|f| (f.path.clone(), f.kind, f.source, f.tracked)).collect()
}

#[test]
fn finds_names_and_content_in_what_a_snapshot_would_contain() {
    let (_t, root) = canonical_tmp();
    let r = root.join("r");
    repo_with_commit(&r);
    commit_file(&r, ".gitignore", ".env.local\nbuild/\n");
    commit_file(&r, "src/config.js", &format!("const key = '{}';\n", aws())); // tracked
    write(&r, ".env", "DB=x\n"); // untracked, not ignored
    write(&r, ".env.local", "ignored\n"); // ignored: not part of a snapshot
    write(&r, ".env.example", "DB=\n"); // template
    write(&r, "keys/id_ed25519", "binary-ish\n");
    write(&r, "keys/deploy.pem", &format!("{}\nMIIE...\n", pem_header()));
    write(&r, "build/out.js", &format!("// {}\n", aws())); // ignored folder
    write(&r, "notes.md", &format!("token {}\n", github()));
    let s = scan(&r);
    let k = kinds(&s);
    assert!(k.contains(&(".env".into(), SecretKind::EnvFile, Source::Name, false)), "{k:?}");
    assert!(k.contains(&("keys/id_ed25519".into(), SecretKind::PrivateKeyFile, Source::Name, false)));
    assert!(k.contains(&("keys/deploy.pem".into(), SecretKind::PrivateKeyBlock, Source::Content, false)));
    assert!(k.contains(&("src/config.js".into(), SecretKind::AwsAccessKey, Source::Content, true)));
    assert!(k.contains(&("notes.md".into(), SecretKind::GithubToken, Source::Content, false)));
    assert_eq!(k.len(), 5, "ignored files and templates must not be listed: {k:?}");
}

#[test]
fn a_finding_never_carries_the_secret() {
    let (_t, root) = canonical_tmp();
    let r = root.join("r");
    repo_with_commit(&r);
    write(&r, "a.txt", &format!("one {} two {}\nline 2: {}\n", aws(), github(), pem_header()));
    let s = scan(&r);
    assert_eq!(s.findings.len(), 3);
    let dump = format!("{}{:?}", serde_json::to_string(&s).unwrap(), s);
    for secret in [aws(), github(), "ABCDEFGHIJKLMNOP".to_string(), "x9x9x9".to_string(), pem_header()] {
        assert!(!dump.contains(&secret), "output leaked {secret}");
    }
    // Only a path and a kind: no line numbers or excerpts exist in the data.
    let v = serde_json::to_value(&s.findings[0]).unwrap();
    let mut keys: Vec<_> = v.as_object().unwrap().keys().cloned().collect();
    keys.sort();
    assert_eq!(keys, ["kind", "path", "source", "tracked"]);
}

#[test]
fn media_archives_binaries_and_big_files_are_never_read() {
    let (_t, root) = canonical_tmp();
    let r = root.join("r");
    repo_with_commit(&r);
    let tok = aws();
    for name in ["photo.png", "clip.mp4", "song.mp3", "bundle.zip", "doc.pdf", "disk.dmg"] {
        write(&r, name, &format!("{tok}\n"));
    }
    let mut with_nul = vec![0u8; 16];
    with_nul.extend_from_slice(tok.as_bytes());
    fs::write(r.join("blob.dat"), with_nul).unwrap();
    write(&r, "huge.txt", &format!("{tok}{}", "x".repeat(1024 * 1024 + 10)));
    write(&r, "cert.p12", "not read, only named\n");
    write(&r, "clean.txt", "nothing here\n");
    let s = scan(&r);
    let k = kinds(&s);
    assert_eq!(k, [("cert.p12".to_string(), SecretKind::PrivateKeyFile, Source::Name, false)], "{k:?}");
    assert_eq!(s.content_skipped, 6 + 1 + 1 + 1); // media/archive/pdf/dmg, nul, huge, p12
}

#[test]
fn symlinks_are_not_followed() {
    let (_t, root) = canonical_tmp();
    let r = root.join("r");
    repo_with_commit(&r);
    let outside = root.join("outside.txt");
    fs::write(&outside, format!("{}\n", aws())).unwrap();
    symlink(&outside, r.join("link.txt")).unwrap();
    assert!(scan(&r).findings.is_empty());
}

#[test]
fn exclusions_resolve_findings() {
    let (_t, root) = canonical_tmp();
    let r = root.join("r");
    repo_with_commit(&r);
    write(&r, ".env", "x\n");
    write(&r, "secrets/a.txt", &format!("{}\n", aws()));
    write(&r, "other/b.txt", &format!("{}\n", github()));
    let s = scan(&r);
    assert_eq!(unresolved(&s.findings, &[]).len(), 3);
    let left = unresolved(&s.findings, &[".env".into(), "secrets".into()]);
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].path, "other/b.txt");
}

#[test]
fn scanning_changes_nothing_and_honours_cancel() {
    let (_t, root) = canonical_tmp();
    let r = root.join("r");
    repo_with_commit(&r);
    write(&r, ".env", "x\n");
    let before = (git(&r, &["status", "--porcelain=v2"]), fs::read(r.join(".git/index")).unwrap());
    let _ = scan(&r);
    let after = (git(&r, &["status", "--porcelain=v2"]), fs::read(r.join(".git/index")).unwrap());
    assert_eq!(before, after);
    let cancelled = scan_repo(&Git::new(&RealRunner), &r, &AtomicBool::new(true)).unwrap();
    assert_eq!(cancelled.files_checked, 0);
}

#[test]
fn a_directory_that_is_not_a_repo_is_an_error_not_a_crash() {
    let (_t, root) = canonical_tmp();
    fs::create_dir_all(root.join("plain")).unwrap();
    assert!(scan_repo(&Git::new(&RealRunner), &root.join("plain"), &AtomicBool::new(false)).is_err());
}
