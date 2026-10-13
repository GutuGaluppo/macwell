//! Read-only view of a repository through `/usr/bin/git`. Nothing here talks to the network: ahead
//! and behind come from what is already local, and no command can modify the repository.

use std::path::Path;
use std::time::Duration;

use serde::Serialize;

use crate::error::{AppError, AppResult};
use crate::exec::{Cmd, CmdOutput, CommandRunner};

pub const GIT: &str = "/usr/bin/git";
const XCODE_SELECT: &str = "/usr/bin/xcode-select";
const GIT_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StatusInfo {
    /// `None` when HEAD is detached.
    pub branch: Option<String>,
    pub detached: bool,
    /// The repository has no commits yet.
    pub no_commits: bool,
    pub upstream: Option<String>,
    /// `None` when there is no upstream (or it is gone).
    pub ahead: Option<u32>,
    pub behind: Option<u32>,
    /// Tracked files with staged or unstaged changes.
    pub changed: u32,
    pub untracked: u32,
    pub conflicted: u32,
}

impl StatusInfo {
    /// Everything that is not committed.
    pub fn uncommitted(&self) -> u32 {
        self.changed + self.untracked + self.conflicted
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Remote {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RepoState {
    #[serde(flatten)]
    pub status: StatusInfo,
    pub remotes: Vec<Remote>,
    /// Gitlinks declared in `.gitmodules`.
    pub submodules: Vec<String>,
    /// Gitlinks with no `.gitmodules` entry: a repository was added inside this one by mistake.
    pub embedded: Vec<String>,
}

/// `git status --porcelain=v2 --branch -z`.
pub fn parse_status(raw: &[u8]) -> StatusInfo {
    let text = String::from_utf8_lossy(raw);
    let mut info = StatusInfo::default();
    let mut tokens = text.split('\0');
    while let Some(tok) = tokens.next() {
        if let Some(h) = tok.strip_prefix("# ") {
            let (key, value) = h.split_once(' ').unwrap_or((h, ""));
            match key {
                "branch.oid" => info.no_commits = value == "(initial)",
                "branch.head" => {
                    info.detached = value == "(detached)";
                    info.branch = (!info.detached).then(|| value.to_string());
                }
                "branch.upstream" => info.upstream = Some(value.to_string()),
                "branch.ab" => {
                    let mut it = value.split(' ');
                    info.ahead = it.next().and_then(|a| a.trim_start_matches('+').parse().ok());
                    info.behind = it.next().and_then(|b| b.trim_start_matches('-').parse().ok());
                }
                _ => {}
            }
            continue;
        }
        match tok.as_bytes().first() {
            Some(b'1') => info.changed += 1,
            Some(b'2') => {
                info.changed += 1;
                tokens.next(); // the original path of a rename or copy follows as its own token
            }
            Some(b'u') => info.conflicted += 1,
            Some(b'?') => info.untracked += 1,
            _ => {} // '!' ignored files, empty trailing token
        }
    }
    info
}

/// `git remote -v`: one entry per remote, using the fetch URL.
pub fn parse_remotes(raw: &str) -> Vec<Remote> {
    let mut out: Vec<Remote> = Vec::new();
    for line in raw.lines() {
        let Some((name, rest)) = line.split_once('\t') else { continue };
        if !rest.ends_with("(fetch)") {
            continue;
        }
        let url = rest.trim_end_matches("(fetch)").trim().to_string();
        if !out.iter().any(|r| r.name == name) {
            out.push(Remote { name: name.to_string(), url });
        }
    }
    out
}

/// Paths of gitlinks (mode 160000) from `git ls-files --stage -z`.
pub fn parse_gitlinks(raw: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(raw)
        .split('\0')
        .filter_map(|e| {
            let (meta, path) = e.split_once('\t')?;
            meta.starts_with("160000 ").then(|| path.to_string())
        })
        .collect()
}

/// Paths declared by `git config -f .gitmodules --null --get-regexp ^submodule\..*\.path$`.
pub fn parse_declared_submodules(raw: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(raw)
        .split('\0')
        .filter_map(|e| e.split_once('\n').map(|(_, path)| path.to_string()))
        .collect()
}

fn split_nul(raw: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(raw).split('\0').filter(|p| !p.is_empty()).map(String::from).collect()
}

pub struct Git<'a> {
    runner: &'a dyn CommandRunner,
    timeout: Duration,
}

impl<'a> Git<'a> {
    pub fn new(runner: &'a dyn CommandRunner) -> Self {
        Git { runner, timeout: GIT_TIMEOUT }
    }

    /// The same runner with a different time limit per command (a push can take minutes).
    pub fn with_timeout(&self, timeout: Duration) -> Git<'a> {
        Git { runner: self.runner, timeout }
    }

    /// `/usr/bin/git` is a shim that offers to install the Command Line Tools when they are missing,
    /// so check first and never invoke it blindly.
    pub fn available(&self) -> AppResult<()> {
        let out = self.runner.run(&Cmd::new(XCODE_SELECT, &["-p"]).timeout(Duration::from_secs(10)))?;
        if out.success() {
            Ok(())
        } else {
            Err(AppError::Other("git não está disponível: instale as Command Line Tools (xcode-select --install)".into()))
        }
    }

    fn run(&self, repo: &Path, args: &[&str]) -> AppResult<CmdOutput> {
        // Paths are data, not patterns. `check-ignore` rejects pathspec magic, so it is the exception.
        self.exec(repo, args, &[], args.first() != Some(&"check-ignore"))
    }

    fn run_ok(&self, repo: &Path, args: &[&str]) -> AppResult<CmdOutput> {
        self.exec_ok(repo, args, &[], args.first() != Some(&"check-ignore"))
    }

    /// Runs one git command in `repo` with the fixed safety configuration (no hooks, no fsmonitor,
    /// no prompts, no optional locks). `literal_pathspecs` turns pathspecs into plain paths; callers
    /// that need pathspec magic such as `:(exclude)` pass `false`.
    pub fn exec(&self, repo: &Path, args: &[&str], env: &[(&str, &str)], literal_pathspecs: bool) -> AppResult<CmdOutput> {
        let repo = repo.to_str().ok_or_else(|| AppError::Invalid("caminho do repositório não é UTF-8".into()))?;
        let mut full = vec!["-C", repo, "-c", "core.fsmonitor=false", "-c", "core.hooksPath=/dev/null", "-c", "core.quotePath=false"];
        full.extend_from_slice(args);
        let mut cmd = Cmd::new(GIT, &full).env("GIT_OPTIONAL_LOCKS", "0").env("GIT_TERMINAL_PROMPT", "0").timeout(self.timeout);
        if literal_pathspecs {
            cmd = cmd.env("GIT_LITERAL_PATHSPECS", "1");
        }
        for (k, v) in env {
            cmd = cmd.env(k, v);
        }
        let out = self.runner.run(&cmd)?;
        if out.timed_out {
            return Err(AppError::Other(format!("git demorou demais em {repo}")));
        }
        Ok(out)
    }

    /// Like [`exec`](Self::exec), turning a non-zero exit into an error with a readable message.
    pub fn exec_ok(&self, repo: &Path, args: &[&str], env: &[(&str, &str)], literal_pathspecs: bool) -> AppResult<CmdOutput> {
        let out = self.exec(repo, args, env, literal_pathspecs)?;
        if out.success() {
            return Ok(out);
        }
        let err = out.stderr_str();
        Err(AppError::Other(if err.contains("license") {
            "git não pôde rodar: aceite a licença do Xcode (sudo xcodebuild -license)".to_string()
        } else {
            format!("git {}: {}", args.first().unwrap_or(&""), err.trim())
        }))
    }

    /// The object name `rev` resolves to, or `None` when it does not exist (no commits, unknown ref).
    pub fn rev_parse(&self, repo: &Path, rev: &str) -> Option<String> {
        let out = self.run(repo, &["rev-parse", "--verify", "-q", rev]).ok()?;
        out.success().then(|| out.stdout_str().trim().to_string()).filter(|s| !s.is_empty())
    }

    /// Author and committer for a commit made without touching the user's configuration: whatever
    /// is configured wins, and only missing parts get a neutral fallback.
    pub fn identity_env(&self, repo: &Path) -> Vec<(String, String)> {
        let get = |key: &str| {
            self.run(repo, &["config", "--get", key]).ok().filter(|o| o.success()).map(|o| o.stdout_str().trim().to_string()).filter(|v| !v.is_empty())
        };
        let mut env = Vec::new();
        if get("user.name").is_none() {
            env.push(("GIT_AUTHOR_NAME".to_string(), "OrganizeMyMac".to_string()));
            env.push(("GIT_COMMITTER_NAME".to_string(), "OrganizeMyMac".to_string()));
        }
        if get("user.email").is_none() {
            env.push(("GIT_AUTHOR_EMAIL".to_string(), "organizemymac@localhost".to_string()));
            env.push(("GIT_COMMITTER_EMAIL".to_string(), "organizemymac@localhost".to_string()));
        }
        env
    }

    /// Whether `git add` could run a clean filter (for example Git LFS) in this repository: a filter
    /// is configured *and* some attributes file assigns one. Filters execute external programs, so a
    /// snapshot refuses to run when this is true.
    pub fn filters_in_use(&self, repo: &Path) -> AppResult<bool> {
        let configured = self.run(repo, &["config", "--get-regexp", r"^filter\..*\.(clean|process)$"])?;
        if configured.status != Some(0) {
            return Ok(false);
        }
        let mut files: Vec<std::path::PathBuf> = Vec::new();
        let listed = self.exec_ok(repo, &["ls-files", "-z", "--cached", "--others", "--exclude-standard", "--", ":(glob)**/.gitattributes"], &[], false)?;
        files.extend(split_nul(&listed.stdout).into_iter().map(|p| repo.join(p)));
        if let Some(info) = self.run(repo, &["rev-parse", "--git-path", "info/attributes"]).ok().filter(|o| o.success()) {
            files.push(repo.join(info.stdout_str().trim()));
        }
        Ok(files.iter().any(|f| {
            std::fs::read_to_string(f)
                .map(|t| t.lines().any(|l| !l.trim_start().starts_with('#') && l.contains("filter=")))
                .unwrap_or(false)
        }))
    }

    /// The URL a push to `remote` would use (`pushurl` and `insteadOf` applied). May contain credentials.
    pub fn push_url(&self, repo: &Path, remote: &str) -> AppResult<String> {
        Ok(self.exec_ok(repo, &["remote", "get-url", "--push", remote], &[], true)?.stdout_str().trim().to_string())
    }

    /// Files a snapshot would contain: `(tracked, untracked but not ignored)`, repository-relative.
    pub fn snapshot_files(&self, repo: &Path) -> AppResult<(Vec<String>, Vec<String>)> {
        let tracked = split_nul(&self.run_ok(repo, &["ls-files", "-z"])?.stdout);
        let untracked = split_nul(&self.run_ok(repo, &["ls-files", "-z", "--others", "--exclude-standard"])?.stdout);
        Ok((tracked, untracked))
    }

    /// Unix time in milliseconds of the latest commit; `None` for a repository with no commits.
    pub fn last_commit_ms(&self, repo: &Path) -> Option<i64> {
        let out = self.run(repo, &["log", "-1", "--format=%ct"]).ok()?;
        out.success().then(|| out.stdout_str().trim().parse::<i64>().ok()).flatten().map(|s| s * 1000)
    }

    /// Tracked files under any of `rels` (repository-relative paths, taken literally).
    pub fn tracked_under(&self, repo: &Path, rels: &[String]) -> AppResult<Vec<String>> {
        let mut found = Vec::new();
        for chunk in rels.chunks(100) {
            let mut args = vec!["ls-files", "-z", "--"];
            args.extend(chunk.iter().map(String::as_str));
            let out = self.run_ok(repo, &args)?;
            found.extend(split_nul(&out.stdout));
        }
        Ok(found)
    }

    /// Which of `rels` git ignores. Exit code 1 only means that none is ignored.
    pub fn ignored_among(&self, repo: &Path, rels: &[String]) -> AppResult<Vec<String>> {
        let mut found = Vec::new();
        for chunk in rels.chunks(100) {
            let mut args = vec!["check-ignore", "--"];
            args.extend(chunk.iter().map(String::as_str));
            let out = self.run(repo, &args)?;
            match out.status {
                Some(0) | Some(1) => found.extend(out.stdout_str().lines().map(String::from)),
                _ => return Err(AppError::Other(format!("git check-ignore: {}", out.stderr_str().trim()))),
            }
        }
        Ok(found)
    }

    pub fn state(&self, repo: &Path) -> AppResult<RepoState> {
        let status = parse_status(&self.run_ok(repo, &["status", "--porcelain=v2", "--branch", "-z", "--untracked-files=normal"])?.stdout);
        let remotes = parse_remotes(&self.run_ok(repo, &["remote", "-v"])?.stdout_str());
        let gitlinks = parse_gitlinks(&self.run_ok(repo, &["ls-files", "--stage", "-z"])?.stdout);

        // Exit code 1 means there is no .gitmodules (or no entries): that is not an error.
        let declared = if gitlinks.is_empty() {
            Vec::new()
        } else {
            let out = self.run(repo, &["config", "-f", ".gitmodules", "--null", "--get-regexp", r"^submodule\..*\.path$"])?;
            parse_declared_submodules(&out.stdout)
        };
        let (submodules, embedded) = gitlinks.into_iter().partition(|p| declared.contains(p));
        Ok(RepoState { status, remotes, submodules, embedded })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exec::FakeRunner;

    #[test]
    fn parses_branch_headers_and_entries() {
        let raw = b"# branch.oid abc123\0# branch.head main\0# branch.upstream origin/main\0# branch.ab +2 -1\0\
            1 .M N... 100644 100644 100644 aa bb src/lib.rs\0\
            1 M. N... 100644 100644 100644 aa bb README.md\0\
            2 R. N... 100644 100644 100644 aa bb R100 new.txt\0old.txt\0\
            u UU N... 100644 100644 100644 100644 aa bb cc conflict.txt\0\
            ? notes.txt\0? dir/other.txt\0! ignored.log\0";
        let s = parse_status(raw);
        assert_eq!(s.branch.as_deref(), Some("main"));
        assert_eq!((s.ahead, s.behind), (Some(2), Some(1)));
        assert_eq!(s.upstream.as_deref(), Some("origin/main"));
        assert_eq!((s.changed, s.untracked, s.conflicted), (3, 2, 1));
        assert_eq!(s.uncommitted(), 6);
        assert!(!s.detached && !s.no_commits);
    }

    #[test]
    fn rename_origin_is_not_counted_as_untracked() {
        let s = parse_status(b"2 R. N... 100644 100644 100644 aa bb R100 new\0? old-looking\0");
        assert_eq!((s.changed, s.untracked), (1, 0));
        // The token after a rename is its original path, even if it starts with '?'.
        let s = parse_status(b"2 R. N... 100644 100644 100644 aa bb R100 new\0?weird\0");
        assert_eq!((s.changed, s.untracked), (1, 0));
    }

    #[test]
    fn detached_initial_and_no_upstream() {
        let s = parse_status(b"# branch.oid (initial)\0# branch.head (detached)\0");
        assert!(s.detached && s.no_commits);
        assert_eq!(s.branch, None);
        assert_eq!((s.ahead, s.behind, s.upstream), (None, None, None));
    }

    #[test]
    fn remotes_use_fetch_urls_once() {
        let r = parse_remotes("origin\tgit@github.com:me/x.git (fetch)\norigin\tgit@github.com:me/x.git (push)\nfork\thttps://h/f.git (fetch)\nfork\thttps://h/f.git (push)\n");
        assert_eq!(r.len(), 2);
        assert_eq!((r[0].name.as_str(), r[0].url.as_str()), ("origin", "git@github.com:me/x.git"));
        assert!(parse_remotes("").is_empty());
    }

    #[test]
    fn gitlinks_and_declared_submodules() {
        let stage = b"100644 aa 0\tREADME.md\0160000 bb 0\tvendor/lib\0160000 cc 0\tstray\0";
        assert_eq!(parse_gitlinks(stage), ["vendor/lib", "stray"]);
        let cfg = b"submodule.lib.path\nvendor/lib\0";
        assert_eq!(parse_declared_submodules(cfg), ["vendor/lib"]);
    }

    #[test]
    fn state_assembles_four_reads_with_a_fixed_environment() {
        let fake = FakeRunner::new()
            .reply("# branch.head main\0")
            .reply("origin\tu (fetch)\norigin\tu (push)\n")
            .reply("") // ls-files: no gitlinks, so .gitmodules is not read
            ;
        let s = Git::new(&fake).state(Path::new("/tmp/r")).unwrap();
        assert_eq!(s.status.branch.as_deref(), Some("main"));
        assert_eq!(s.remotes.len(), 1);
        let calls = fake.calls();
        assert_eq!(calls.len(), 3);
        assert!(calls[0].starts_with("/usr/bin/git -C /tmp/r -c core.fsmonitor=false -c core.hooksPath=/dev/null -c core.quotePath=false status"));
    }

    #[test]
    fn identity_falls_back_only_for_what_is_missing() {
        // name configured, email not: only the email gets a neutral value.
        let fake = FakeRunner::new().reply("Ada Dev\n").reply_failure(1, "");
        let env = Git::new(&fake).identity_env(Path::new("/tmp/r"));
        let keys: Vec<_> = env.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(keys, ["GIT_AUTHOR_EMAIL", "GIT_COMMITTER_EMAIL"]);
        // nothing configured: both.
        let fake = FakeRunner::new().reply_failure(1, "").reply_failure(1, "");
        assert_eq!(Git::new(&fake).identity_env(Path::new("/tmp/r")).len(), 4);
        // everything configured: the user's identity is used untouched.
        let fake = FakeRunner::new().reply("Ada\n").reply("ada@x\n");
        assert!(Git::new(&fake).identity_env(Path::new("/tmp/r")).is_empty());
    }

    #[test]
    fn license_and_missing_tools_give_clear_errors() {
        let fake = FakeRunner::new().reply_failure(69, "You have not agreed to the Xcode license agreements.");
        let err = Git::new(&fake).state(Path::new("/tmp/r")).unwrap_err().to_string();
        assert!(err.contains("licença"), "{err}");
        let fake = FakeRunner::new().reply_failure(2, "");
        assert!(Git::new(&fake).available().is_err());
        let fake = FakeRunner::new().reply("/Library/Developer/CommandLineTools\n");
        assert!(Git::new(&fake).available().is_ok());
    }
}
