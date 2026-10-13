//! Pushing a WIP snapshot branch. It is its own, explicit action: one ref that this app created,
//! one remote that is already configured, never `--force`, and a warning the user must accept when
//! the remote is public or its visibility is not known.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

use regex::Regex;
use serde::Serialize;

use crate::dev::git::Git;
use crate::dev::visibility::{classify, redact, RemoteKind, Visibility};
use crate::error::{AppError, AppResult};

const PUSH_TIMEOUT: Duration = Duration::from_secs(300);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PushWarning {
    /// The remote is public: everything in the snapshot becomes readable by anyone.
    Public,
    /// Nobody checked, so it is treated like a public one.
    Unknown,
}

/// A local path never leaves the Mac, and a remote that was found private or missing needs no
/// warning. Everything else — public, or not known — does.
pub fn warning_for(kind: &RemoteKind, visibility: Option<Visibility>) -> Option<PushWarning> {
    match (kind, visibility) {
        (RemoteKind::Local, _) => None,
        (_, Some(Visibility::PrivateOrMissing)) => None,
        (_, Some(Visibility::Public)) => Some(PushWarning::Public),
        (_, Some(Visibility::Unknown)) | (_, None) => Some(PushWarning::Unknown),
    }
}

/// Only branches this app creates can be pushed.
pub fn valid_snapshot_ref(name: &str) -> bool {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^wip/snapshot-\d{4}-\d{2}-\d{2}(-\d{4}(\d{2})?)?$").unwrap()).is_match(name)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PushPlan {
    pub repo: PathBuf,
    pub remote: String,
    /// The URL a push would really use (`pushurl` and `insteadOf` applied), credentials removed.
    pub url: String,
    pub kind: RemoteKind,
    pub ref_name: String,
    pub commit: String,
    pub visibility: Option<Visibility>,
    /// When the user last checked, milliseconds.
    pub visibility_checked_at: Option<i64>,
    pub warning: Option<PushWarning>,
    pub remotes: Vec<String>,
}

/// Read-only. `known` is the cached answer for this exact remote URL, if the user ever asked.
pub fn plan_push(git: &Git, repo: &Path, ref_name: &str, remote: &str, known: Option<(Visibility, i64)>) -> AppResult<PushPlan> {
    if !valid_snapshot_ref(ref_name) {
        return Err(AppError::Invalid(format!("só branches de snapshot podem ser enviadas: {ref_name}")));
    }
    let commit = git
        .rev_parse(repo, &format!("refs/heads/{ref_name}"))
        .ok_or_else(|| AppError::NotFound(format!("a branch {ref_name} não existe neste repositório")))?;
    let remotes: Vec<String> = git.exec_ok(repo, &["remote"], &[], true)?.stdout_str().lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect();
    if !remotes.iter().any(|r| r == remote) {
        return Err(AppError::Invalid(format!("remote desconhecido: {remote}")));
    }
    let url = git.push_url(repo, remote)?;
    let kind = classify(&url);
    Ok(PushPlan {
        repo: repo.to_path_buf(),
        remote: remote.to_string(),
        url: redact(&url),
        warning: warning_for(&kind, known.map(|k| k.0)),
        kind,
        ref_name: ref_name.to_string(),
        commit,
        visibility: known.map(|k| k.0),
        visibility_checked_at: known.map(|k| k.1),
        remotes,
    })
}

#[derive(Debug, Clone, Copy)]
pub struct PushConsent {
    /// The user pressed the confirm button for this exact push.
    pub confirmed: bool,
    /// The user read the public/unknown warning and still wants to send it.
    pub accept_exposure: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PushResult {
    pub remote: String,
    pub url: String,
    pub ref_name: String,
    pub commit: String,
}

pub fn push(git: &Git, repo: &Path, ref_name: &str, remote: &str, known: Option<(Visibility, i64)>, consent: PushConsent) -> AppResult<PushResult> {
    let plan = plan_push(git, repo, ref_name, remote, known)?;
    if !consent.confirmed {
        return Err(AppError::Invalid("o envio precisa ser confirmado".into()));
    }
    if plan.warning.is_some() && !consent.accept_exposure {
        return Err(AppError::Invalid("este remote é público ou de visibilidade desconhecida: aceite o aviso para enviar".into()));
    }
    let refspec = format!("refs/heads/{ref_name}:refs/heads/{ref_name}");
    // Never prompt: a missing credential must fail, not hang. A user-configured ssh command is respected.
    let mut env: Vec<(&str, &str)> = Vec::new();
    let has_ssh_command = git.exec(repo, &["config", "--get", "core.sshCommand"], &[], true).map(|o| o.success()).unwrap_or(false);
    if !has_ssh_command {
        env.push(("GIT_SSH_COMMAND", "ssh -o BatchMode=yes"));
    }
    let out = git.with_timeout(PUSH_TIMEOUT).exec(
        repo,
        &["push", "--recurse-submodules=no", "--no-verify", "--porcelain", remote, refspec.as_str()],
        &env,
        true,
    )?;
    if !out.success() {
        let err = redact(out.stderr_str().trim());
        let hint = if err.contains("rejected") || err.contains("non-fast-forward") { " (a branch já existe no remote com outro conteúdo; nada foi sobrescrito)" } else { "" };
        return Err(AppError::Other(format!("o envio falhou: {err}{hint}")));
    }
    Ok(PushResult { remote: plan.remote, url: plan.url, ref_name: plan.ref_name, commit: plan.commit })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exec::FakeRunner;

    #[test]
    fn only_snapshot_branches_are_pushable() {
        for ok in ["wip/snapshot-2024-05-06", "wip/snapshot-2024-05-06-0708", "wip/snapshot-2024-05-06-070809"] {
            assert!(valid_snapshot_ref(ok), "{ok}");
        }
        for bad in ["main", "wip/other", "wip/snapshot-2024-05-06/x", "wip/snapshot-2024-5-6", "refs/heads/wip/snapshot-2024-05-06", "+wip/snapshot-2024-05-06", "wip/snapshot-2024-05-06 ", "-wip/snapshot-2024-05-06", "wip/snapshot-2024-05-06:main", ""] {
            assert!(!valid_snapshot_ref(bad), "{bad:?}");
        }
    }

    #[test]
    fn warning_table() {
        let gh = RemoteKind::Github { owner: "me".into(), repo: "x".into() };
        let other = RemoteKind::Other { host: "example.com".into() };
        use Visibility::*;
        assert_eq!(warning_for(&RemoteKind::Local, None), None);
        assert_eq!(warning_for(&RemoteKind::Local, Some(Public)), None);
        assert_eq!(warning_for(&gh, Some(Public)), Some(PushWarning::Public));
        assert_eq!(warning_for(&gh, None), Some(PushWarning::Unknown));
        assert_eq!(warning_for(&gh, Some(Unknown)), Some(PushWarning::Unknown));
        assert_eq!(warning_for(&gh, Some(PrivateOrMissing)), None);
        assert_eq!(warning_for(&other, None), Some(PushWarning::Unknown));
    }

    // Replies for plan_push: rev-parse, `remote`, `remote get-url --push`.
    fn planned(url: &str) -> FakeRunner {
        FakeRunner::new().reply("abc123\n").reply("origin\nfork\n").reply(&format!("{url}\n"))
    }

    #[test]
    fn refusals_happen_before_any_push_command_runs() {
        let consent = |confirmed, accept_exposure| PushConsent { confirmed, accept_exposure };
        let r = Path::new("/tmp/r");
        let ref_ok = "wip/snapshot-2024-05-06";
        // Not confirmed.
        let fake = planned("/Users/me/backup.git");
        assert!(push(&Git::new(&fake), r, ref_ok, "origin", None, consent(false, true)).is_err());
        assert!(!fake.calls().iter().any(|c| c.contains(" push ")));
        // Public remote, warning not accepted.
        let fake = planned("git@github.com:me/x.git");
        let err = push(&Git::new(&fake), r, ref_ok, "origin", Some((Visibility::Public, 1)), consent(true, false)).unwrap_err().to_string();
        assert!(err.contains("aceite o aviso"), "{err}");
        assert!(!fake.calls().iter().any(|c| c.contains(" push ")));
        // Visibility never checked is treated the same way.
        let fake = planned("https://example.com/me/x.git");
        assert!(push(&Git::new(&fake), r, ref_ok, "origin", None, consent(true, false)).is_err());
        assert!(!fake.calls().iter().any(|c| c.contains(" push ")));
        // A branch that is not a snapshot never reaches git at all.
        let fake = FakeRunner::new();
        assert!(push(&Git::new(&fake), r, "main", "origin", None, consent(true, true)).is_err());
        assert!(fake.calls().is_empty());
        // A remote that is not configured (for example a URL typed in) is refused.
        let fake = FakeRunner::new().reply("abc123\n").reply("origin\n");
        assert!(push(&Git::new(&fake), r, ref_ok, "https://evil.test/x.git", None, consent(true, true)).is_err());
        assert!(!fake.calls().iter().any(|c| c.contains(" push ")));
    }

    #[test]
    fn the_plan_hides_credentials_and_uses_the_push_url() {
        let fake = planned("https://user:secret@example.com/me/x.git");
        let plan = plan_push(&Git::new(&fake), Path::new("/tmp/r"), "wip/snapshot-2024-05-06", "origin", None).unwrap();
        assert_eq!(plan.url, "https://***@example.com/me/x.git");
        assert!(fake.calls()[2].ends_with("remote get-url --push origin"));
        assert_eq!(plan.warning, Some(PushWarning::Unknown));
    }

    #[test]
    fn push_errors_are_redacted_even_if_git_prints_credentials() {
        // Replies: rev-parse, remote, get-url, config core.sshCommand (unset), then the failing push.
        let fake = planned("https://example.com/me/x.git")
            .reply_failure(1, "")
            .reply_failure(1, "fatal: unable to access 'https://someuser:s3cr3tpass@example.com/me/x.git/': denied\n ! [rejected] wip (non-fast-forward)");
        let consent = PushConsent { confirmed: true, accept_exposure: true };
        let err = push(&Git::new(&fake), Path::new("/tmp/r"), "wip/snapshot-2024-05-06", "origin", None, consent).unwrap_err().to_string();
        assert!(!err.contains("s3cr3tpass") && !err.contains("someuser"), "{err}");
        assert!(err.contains("nada foi sobrescrito"), "{err}");
        let calls = fake.calls();
        let push_call = calls.last().unwrap();
        assert!(push_call.contains("push --recurse-submodules=no --no-verify --porcelain origin refs/heads/wip/snapshot-2024-05-06:refs/heads/wip/snapshot-2024-05-06"), "{push_call}");
        assert!(!push_call.contains("--force") && !push_call.contains(":+") && !push_call.contains(" +"), "{push_call}");
    }

    #[test]
    fn a_missing_branch_is_not_found() {
        let fake = FakeRunner::new().reply_failure(1, "");
        assert!(matches!(plan_push(&Git::new(&fake), Path::new("/tmp/r"), "wip/snapshot-2024-05-06", "origin", None), Err(AppError::NotFound(_))));
    }
}
