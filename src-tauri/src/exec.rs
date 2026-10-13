//! Process execution behind a trait, so modules that call system tools (`git`, `tmutil`, `brew`)
//! can be tested with canned output. Programs are always absolute paths with fixed argument lists;
//! nothing here builds a shell command and nothing is ever taken from UI text as a program name.

use std::collections::VecDeque;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::error::{AppError, AppResult};

/// Per-stream capture limit.
const MAX_OUTPUT: u64 = 8 * 1024 * 1024;

/// Variables copied from the app's environment. Everything else is dropped.
const PASSTHROUGH: &[&str] = &["HOME", "USER", "LOGNAME", "TMPDIR", "SSH_AUTH_SOCK"];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CmdOutput {
    /// `None` when the process was killed (timeout) or ended by a signal.
    pub status: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub timed_out: bool,
}

impl CmdOutput {
    pub fn success(&self) -> bool {
        self.status == Some(0) && !self.timed_out
    }

    pub fn stdout_str(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }

    pub fn stderr_str(&self) -> String {
        String::from_utf8_lossy(&self.stderr).into_owned()
    }
}

/// One invocation: `program` (absolute), `args`, extra environment and an optional working directory.
#[derive(Debug, Clone)]
pub struct Cmd<'a> {
    pub program: &'a str,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub cwd: Option<&'a Path>,
    pub timeout: Duration,
}

impl<'a> Cmd<'a> {
    pub fn new(program: &'a str, args: &[&str]) -> Self {
        Cmd {
            program,
            args: args.iter().map(|a| a.to_string()).collect(),
            env: Vec::new(),
            cwd: None,
            timeout: Duration::from_secs(30),
        }
    }

    pub fn env(mut self, key: &str, value: &str) -> Self {
        self.env.push((key.to_string(), value.to_string()));
        self
    }

    pub fn cwd(mut self, dir: &'a Path) -> Self {
        self.cwd = Some(dir);
        self
    }

    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }
}

pub trait CommandRunner: Send + Sync {
    fn run(&self, cmd: &Cmd) -> AppResult<CmdOutput>;
}

/// Runs the real process with a cleared environment, `LC_ALL=C`, a fixed `PATH`, and a hard timeout.
pub struct RealRunner;

impl CommandRunner for RealRunner {
    fn run(&self, cmd: &Cmd) -> AppResult<CmdOutput> {
        if !Path::new(cmd.program).is_absolute() {
            return Err(AppError::Invalid(format!("program must be an absolute path: {}", cmd.program)));
        }
        let mut c = Command::new(cmd.program);
        c.args(&cmd.args).env_clear().stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
        c.env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin").env("LC_ALL", "C");
        for key in PASSTHROUGH {
            if let Ok(v) = std::env::var(key) {
                c.env(key, v);
            }
        }
        for (k, v) in &cmd.env {
            c.env(k, v);
        }
        if let Some(dir) = cmd.cwd {
            c.current_dir(dir);
        }
        let mut child = c.spawn()?;
        let mut stdout = child.stdout.take().expect("piped");
        let mut stderr = child.stderr.take().expect("piped");
        let out = std::thread::spawn(move || read_capped(&mut stdout));
        let err = std::thread::spawn(move || read_capped(&mut stderr));

        let deadline = Instant::now() + cmd.timeout;
        let (status, timed_out) = loop {
            match child.try_wait()? {
                Some(s) => break (s.code(), false),
                None if Instant::now() >= deadline => {
                    let _ = child.kill();
                    let _ = child.wait();
                    break (None, true);
                }
                None => std::thread::sleep(Duration::from_millis(10)),
            }
        };
        Ok(CmdOutput {
            status,
            stdout: out.join().unwrap_or_default(),
            stderr: err.join().unwrap_or_default(),
            timed_out,
        })
    }
}

fn read_capped(r: &mut impl Read) -> Vec<u8> {
    let mut buf = Vec::new();
    let _ = r.take(MAX_OUTPUT).read_to_end(&mut buf);
    // Keep draining so the child never blocks on a full pipe.
    let _ = std::io::copy(r, &mut std::io::sink());
    buf
}

/// Test double: records every invocation and answers either from routes (matched by a text the call
/// contains, reusable, order does not matter) or, failing that, from queued replies in order.
#[derive(Default)]
pub struct FakeRunner {
    replies: Mutex<VecDeque<AppResult<CmdOutput>>>,
    routes: Mutex<Vec<(String, CmdOutput)>>,
    calls: Mutex<Vec<String>>,
    envs: Mutex<Vec<Vec<(String, String)>>>,
}

impl FakeRunner {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reply(self, stdout: &str) -> Self {
        self.push(Ok(CmdOutput { status: Some(0), stdout: stdout.as_bytes().to_vec(), ..Default::default() }))
    }

    pub fn reply_failure(self, code: i32, stderr: &str) -> Self {
        self.push(Ok(CmdOutput { status: Some(code), stderr: stderr.as_bytes().to_vec(), ..Default::default() }))
    }

    /// Any call whose text (`"program arg1 arg2"`) contains `needle` gets this successful output.
    pub fn route(self, needle: &str, stdout: &str) -> Self {
        self.routes.lock().unwrap().push((needle.to_string(), CmdOutput { status: Some(0), stdout: stdout.as_bytes().to_vec(), ..Default::default() }));
        self
    }

    /// Same, for a failing command.
    pub fn route_failure(self, needle: &str, code: i32, stderr: &str) -> Self {
        self.routes.lock().unwrap().push((needle.to_string(), CmdOutput { status: Some(code), stderr: stderr.as_bytes().to_vec(), ..Default::default() }));
        self
    }

    pub fn reply_error(self, err: AppError) -> Self {
        self.push(Err(err))
    }

    fn push(self, r: AppResult<CmdOutput>) -> Self {
        self.replies.lock().unwrap().push_back(r);
        self
    }

    /// Each call as `"program arg1 arg2"`.
    pub fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }

    /// The extra environment each call asked for, in the same order as `calls`.
    pub fn envs(&self) -> Vec<Vec<(String, String)>> {
        self.envs.lock().unwrap().clone()
    }
}

impl CommandRunner for FakeRunner {
    fn run(&self, cmd: &Cmd) -> AppResult<CmdOutput> {
        let text = std::iter::once(cmd.program.to_string()).chain(cmd.args.clone()).collect::<Vec<_>>().join(" ");
        self.calls.lock().unwrap().push(text.clone());
        self.envs.lock().unwrap().push(cmd.env.clone());
        if let Some((_, out)) = self.routes.lock().unwrap().iter().find(|(needle, _)| text.contains(needle.as_str())) {
            return Ok(out.clone());
        }
        self.replies
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| Err(AppError::Other("FakeRunner: no reply queued".into())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn captures_output_and_status() {
        let out = RealRunner.run(&Cmd::new("/bin/echo", &["olá"])).unwrap();
        assert!(out.success());
        assert_eq!(out.stdout_str(), "olá\n");
        let fail = RealRunner.run(&Cmd::new("/usr/bin/false", &[])).unwrap();
        assert_eq!(fail.status, Some(1));
        assert!(!fail.success());
    }

    #[test]
    fn rejects_relative_program() {
        assert!(matches!(RealRunner.run(&Cmd::new("git", &[])), Err(AppError::Invalid(_))));
    }

    #[test]
    fn environment_is_cleared_but_explicit_vars_pass() {
        std::env::set_var("OMM_SECRET_TEST", "leak");
        let out = RealRunner
            .run(&Cmd::new("/usr/bin/env", &[]).env("OMM_EXPLICIT", "yes"))
            .unwrap()
            .stdout_str();
        assert!(!out.contains("OMM_SECRET_TEST"));
        assert!(out.contains("OMM_EXPLICIT=yes"));
        assert!(out.contains("LC_ALL=C"));
    }

    #[test]
    fn timeout_kills_the_process() {
        let started = Instant::now();
        let out = RealRunner
            .run(&Cmd::new("/bin/sleep", &["5"]).timeout(Duration::from_millis(150)))
            .unwrap();
        assert!(out.timed_out);
        assert_eq!(out.status, None);
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn fake_runner_replays_and_records() {
        let fake = FakeRunner::new().reply("ok").reply_failure(2, "boom");
        assert_eq!(fake.run(&Cmd::new("/usr/bin/tmutil", &["status"])).unwrap().stdout_str(), "ok");
        assert_eq!(fake.run(&Cmd::new("/usr/bin/tmutil", &["latestbackup"])).unwrap().status, Some(2));
        assert!(fake.run(&Cmd::new("/usr/bin/tmutil", &[])).is_err());
        assert_eq!(fake.calls(), vec!["/usr/bin/tmutil status", "/usr/bin/tmutil latestbackup", "/usr/bin/tmutil"]);
    }
}
