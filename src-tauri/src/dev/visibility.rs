//! What kind of remote a repository has and, only when the user asks, whether it is public.
//!
//! The one network call here is an unauthenticated `GET https://api.github.com/repos/<owner>/<repo>`
//! for github.com remotes. It sends nothing but that URL and the app's user agent: no token, no file
//! names, no content. Any other host is "unknown", and a local path never needs a check.

use std::sync::OnceLock;
use std::time::Duration;

use regex::Regex;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RemoteKind {
    /// A path or `file://` URL: the data stays on this Mac (or a disk attached to it).
    Local,
    Github { owner: String, repo: String },
    Other { host: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Visibility {
    Public,
    /// GitHub answers 404 both for private and for missing repositories.
    PrivateOrMissing,
    Unknown,
}

impl Visibility {
    pub fn as_str(self) -> &'static str {
        match self {
            Visibility::Public => "public",
            Visibility::PrivateOrMissing => "privateOrMissing",
            Visibility::Unknown => "unknown",
        }
    }

    pub fn parse(s: &str) -> Visibility {
        match s {
            "public" => Visibility::Public,
            "privateOrMissing" => Visibility::PrivateOrMissing,
            _ => Visibility::Unknown,
        }
    }
}

fn valid_name(s: &str) -> bool {
    !s.is_empty() && s != "." && s != ".." && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
}

/// `(scheme, host, path)` of a remote URL, ignoring any credentials in it.
fn split(url: &str) -> Option<(Option<String>, String, String)> {
    if let Some((scheme, rest)) = url.split_once("://") {
        let (authority, path) = rest.split_once('/').unwrap_or((rest, ""));
        let host_port = authority.rsplit('@').next().unwrap_or(authority);
        let host = host_port.split(':').next().unwrap_or(host_port).to_lowercase();
        return Some((Some(scheme.to_lowercase()), host, path.to_string()));
    }
    // scp-like: [user@]host:path, where the part before ':' has no slash
    let (left, path) = url.split_once(':')?;
    if left.contains('/') || left.is_empty() {
        return None;
    }
    Some((None, left.rsplit('@').next().unwrap_or(left).to_lowercase(), path.to_string()))
}

pub fn classify(url: &str) -> RemoteKind {
    let url = url.trim();
    if url.starts_with('/') || url.starts_with("./") || url.starts_with("../") || url.starts_with('~') || url.starts_with("file://") {
        return RemoteKind::Local;
    }
    let Some((_, host, path)) = split(url) else { return RemoteKind::Local }; // a relative path
    if host == "github.com" {
        let path = path.trim_matches('/');
        let path = path.strip_suffix(".git").unwrap_or(path);
        let parts: Vec<&str> = path.split('/').collect();
        if let [owner, repo] = parts.as_slice() {
            if valid_name(owner) && valid_name(repo) {
                return RemoteKind::Github { owner: owner.to_string(), repo: repo.to_string() };
            }
        }
    }
    RemoteKind::Other { host }
}

fn credentials() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"(?i)(https?://)[^/\s@]+@").unwrap())
}

/// Removes credentials from http(s) URLs (`https://user:token@host`), wherever they appear in `text`.
/// ssh URLs keep `git@`: that is a user name, not a secret.
pub fn redact(text: &str) -> String {
    credentials().replace_all(text, "${1}***@").into_owned()
}

/// The question the user asked; answered only on request.
pub trait HttpProbe: Send + Sync {
    /// HTTP status of a GET, or an error text when there was no answer.
    fn status(&self, url: &str) -> Result<u16, String>;
}

pub struct UreqProbe;

impl HttpProbe for UreqProbe {
    fn status(&self, url: &str) -> Result<u16, String> {
        let agent = ureq::AgentBuilder::new()
            .timeout(Duration::from_secs(8))
            .https_only(true)
            .redirects(2)
            .user_agent(concat!("OrganizeMyMac/", env!("CARGO_PKG_VERSION"), " (repository visibility, on request)"))
            .build();
        match agent.get(url).set("Accept", "application/vnd.github+json").call() {
            Ok(r) => Ok(r.status()),
            Err(ureq::Error::Status(code, _)) => Ok(code),
            Err(e) => Err(e.to_string()),
        }
    }
}

/// Asks GitHub whether `owner/repo` is public. Rate limits, outages and anything unexpected are
/// "unknown", never a guess.
pub fn probe_github(owner: &str, repo: &str, http: &dyn HttpProbe) -> Visibility {
    if !valid_name(owner) || !valid_name(repo) {
        return Visibility::Unknown;
    }
    match http.status(&format!("https://api.github.com/repos/{owner}/{repo}")) {
        Ok(200) => Visibility::Public,
        Ok(404) => Visibility::PrivateOrMissing,
        _ => Visibility::Unknown,
    }
}

/// `None` when the remote cannot be checked (not GitHub, or local).
pub fn check(url: &str, http: &dyn HttpProbe) -> Option<Visibility> {
    match classify(url) {
        RemoteKind::Github { owner, repo } => Some(probe_github(&owner, &repo, http)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    struct Fake {
        reply: Result<u16, String>,
        asked: Mutex<Vec<String>>,
    }

    impl HttpProbe for Fake {
        fn status(&self, url: &str) -> Result<u16, String> {
            self.asked.lock().unwrap().push(url.to_string());
            self.reply.clone()
        }
    }

    fn fake(reply: Result<u16, String>) -> Fake {
        Fake { reply, asked: Mutex::new(Vec::new()) }
    }

    fn gh(o: &str, r: &str) -> RemoteKind {
        RemoteKind::Github { owner: o.into(), repo: r.into() }
    }

    #[test]
    fn classifies_every_common_url_shape() {
        for url in [
            "git@github.com:me/x.git",
            "https://github.com/me/x",
            "https://github.com/me/x.git",
            "https://github.com/me/x/",
            "ssh://git@github.com/me/x.git",
            "https://user:ghp_token@github.com/me/x.git",
            "https://TOKEN@GitHub.com/me/x.git",
            "git://github.com/me/x.git",
        ] {
            assert_eq!(classify(url), gh("me", "x"), "{url}");
        }
        for url in ["/Users/me/backup.git", "file:///Users/me/backup.git", "../sibling", "./x", "~/repos/x.git", "plain-relative"] {
            assert_eq!(classify(url), RemoteKind::Local, "{url}");
        }
        assert_eq!(classify("git@gitlab.com:me/x.git"), RemoteKind::Other { host: "gitlab.com".into() });
        assert_eq!(classify("https://bitbucket.org/me/x.git"), RemoteKind::Other { host: "bitbucket.org".into() });
        assert_eq!(classify("https://user:pw@example.com:8443/me/x.git"), RemoteKind::Other { host: "example.com".into() });
        // Look-alike hosts and odd paths are never treated as GitHub.
        for url in ["https://github.com.evil.test/me/x", "https://notgithub.com/me/x", "https://github.com/me", "https://github.com/me/x/y", "https://github.com/me/../x"] {
            assert!(matches!(classify(url), RemoteKind::Other { .. }), "{url}");
        }
    }

    #[test]
    fn credentials_are_redacted_in_urls_and_messages() {
        assert_eq!(redact("https://user:secret@example.com/x.git"), "https://***@example.com/x.git");
        assert_eq!(redact("https://TOKEN123@github.com/me/x.git"), "https://***@github.com/me/x.git");
        assert_eq!(redact("git@github.com:me/x.git"), "git@github.com:me/x.git");
        assert_eq!(redact("ssh://git@github.com/me/x.git"), "ssh://git@github.com/me/x.git");
        let msg = redact("fatal: unable to access 'https://u:pw@h.test/r.git/': Failed to connect; also https://k@h2.test/");
        assert!(!msg.contains("pw") && !msg.contains("k@"), "{msg}");
    }

    #[test]
    fn the_probe_sends_only_the_repository_url() {
        let http = fake(Ok(200));
        assert_eq!(check("git@github.com:me/x.git", &http), Some(Visibility::Public));
        assert_eq!(*http.asked.lock().unwrap(), ["https://api.github.com/repos/me/x"]);
    }

    #[test]
    fn answers_map_conservatively() {
        for (reply, expected) in [
            (Ok(200), Visibility::Public),
            (Ok(404), Visibility::PrivateOrMissing),
            (Ok(403), Visibility::Unknown),
            (Ok(429), Visibility::Unknown),
            (Ok(500), Visibility::Unknown),
            (Err("offline".to_string()), Visibility::Unknown),
        ] {
            assert_eq!(probe_github("me", "x", &fake(reply.clone())), expected, "{reply:?}");
        }
    }

    #[test]
    fn nothing_is_requested_for_other_hosts_or_local_paths_or_bad_names() {
        let http = fake(Ok(200));
        assert_eq!(check("git@gitlab.com:me/x.git", &http), None);
        assert_eq!(check("/Users/me/backup.git", &http), None);
        assert_eq!(probe_github("me", "x/../../admin", &http), Visibility::Unknown);
        assert_eq!(probe_github("", "x", &http), Visibility::Unknown);
        assert!(http.asked.lock().unwrap().is_empty());
    }

    #[test]
    fn visibility_round_trips_through_text() {
        for v in [Visibility::Public, Visibility::PrivateOrMissing, Visibility::Unknown] {
            assert_eq!(Visibility::parse(v.as_str()), v);
        }
        assert_eq!(Visibility::parse("garbage"), Visibility::Unknown);
    }
}
