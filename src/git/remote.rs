//! Fetch, pull and push by shelling out to `git`, so credentials (SSH
//! agent, credential helpers, askpass), hooks (`pre-push`), and
//! `pull.rebase`-style config all work the way they do for the user's own
//! `git`. See `docs/PLAN_9_REMOTE.md`. Slow, network-crossing calls: run
//! off the main thread — that plan's "Approach part 2" — this module only
//! provides the blocking calls, threading is `App`'s concern.
//!
//! `git2`'s remote callbacks would need SSH-agent lookup, credential
//! helpers and interactive prompts implemented by hand; the user's own
//! `git` already has all of that solved. Reading which remotes exist is
//! the one part of this module that stays a `git2` read (no credentials
//! or hooks involved), same split the rest of `git::` already makes.

use std::io::{self, Read};
use std::path::Path;
use std::process::{Child, Command, Output, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use git2::Repository;

use crate::git::diff::workdir;
use crate::git::error::{GitError, GitResult};

const REMOTE_TIMEOUT: Duration = Duration::from_secs(300);
const TERMINATE_GRACE: Duration = Duration::from_secs(2);
const POLL_INTERVAL: Duration = Duration::from_millis(40);

/// One configured remote, `git remote -v`'s own model (fetch and push URLs
/// can differ; usually do not).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteEntry {
    pub name: String,
    pub fetch_url: String,
    pub push_url: String,
}

/// Configured remotes, alphabetical.
pub(super) fn remotes(repo: &Repository) -> GitResult<Vec<RemoteEntry>> {
    let mut names: Vec<String> = repo
        .remotes()
        .map_err(GitError::Read)?
        .iter()
        .filter_map(|res| res.ok().flatten())
        .map(str::to_owned)
        .collect();
    names.sort();

    names
        .into_iter()
        .map(|name| {
            let remote = repo.find_remote(&name).map_err(GitError::Read)?;
            let fetch_url = remote.url().unwrap_or_default().to_owned();
            let push_url = remote
                .pushurl()
                .ok()
                .flatten()
                .unwrap_or(fetch_url.as_str())
                .to_owned();
            Ok(RemoteEntry {
                name,
                fetch_url,
                push_url,
            })
        })
        .collect()
}

/// `stdout` + `stderr`, each trimmed, joined by a newline when both are
/// non-empty. Unlike `apply.rs`/`commit.rs`/`branch.rs`'s error-only
/// `stderr()`, a fetch/pull/push's *success* line is worth showing too
/// (git puts progress and the human summary on stderr, machine-parseable
/// bits, when there are any, on stdout); ferrit does not parse either,
/// just shows them.
fn combined_output(out: &Output) -> String {
    let out_text = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    let err_text = String::from_utf8_lossy(&out.stderr).trim().to_owned();
    match (out_text.is_empty(), err_text.is_empty()) {
        (true, _) => err_text,
        (false, true) => out_text,
        (false, false) => format!("{out_text}\n{err_text}"),
    }
}

/// `git -C <workdir> <...args>`, run to completion. `Ok`/`Err` both carry
/// `combined_output`; the caller's `err` only decides which `GitError`
/// variant wraps a non-zero exit.
fn run_git(
    workdir: &Path,
    args: &[String],
    cancel: Option<&AtomicBool>,
    err: impl Fn(String) -> GitError,
) -> GitResult<String> {
    let out = run_command(workdir, args, cancel, &err)?;
    let combined = combined_output(&out);
    if out.status.success() {
        Ok(combined)
    } else {
        Err(err(combined))
    }
}

fn run_command(
    workdir: &Path,
    args: &[String],
    cancel: Option<&AtomicBool>,
    err: &impl Fn(String) -> GitError,
) -> GitResult<Output> {
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(workdir)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command
        .spawn()
        .map_err(|e| err(format!("cannot run git: {e}")))?;
    let pid = child.id();
    let stdout = child.stdout.take().ok_or_else(|| {
        stop_process_group(&mut child, pid);
        err("cannot capture git stdout".to_owned())
    })?;
    let stderr = child.stderr.take().ok_or_else(|| {
        stop_process_group(&mut child, pid);
        err("cannot capture git stderr".to_owned())
    })?;
    let stdout_reader = thread::spawn(move || read_all(stdout));
    let stderr_reader = thread::spawn(move || read_all(stderr));
    let deadline = Instant::now() + REMOTE_TIMEOUT;

    let status = loop {
        if cancel.is_some_and(|flag| flag.load(Ordering::Acquire)) {
            stop_process_group(&mut child, pid);
            let output = collect_output(stdout_reader, stderr_reader);
            return Err(err(with_diagnostics("cancelled during shutdown", &output)));
        }
        if Instant::now() >= deadline {
            stop_process_group(&mut child, pid);
            let output = collect_output(stdout_reader, stderr_reader);
            let reason = format!("timed out after {} seconds", REMOTE_TIMEOUT.as_secs());
            return Err(err(with_diagnostics(&reason, &output)));
        }
        match child.try_wait() {
            Err(e) => {
                stop_process_group(&mut child, pid);
                return Err(err(format!("cannot wait for git: {e}")));
            },
            Ok(Some(status)) => break status,
            Ok(None) => thread::sleep(POLL_INTERVAL),
        }
    };
    let stdout = stdout_reader
        .join()
        .map_err(|_| err("git stdout reader panicked".to_owned()))?
        .map_err(|e| err(format!("cannot read git stdout: {e}")))?;
    let stderr = stderr_reader
        .join()
        .map_err(|_| err("git stderr reader panicked".to_owned()))?
        .map_err(|e| err(format!("cannot read git stderr: {e}")))?;
    let out = Output {
        status,
        stdout,
        stderr,
    };
    Ok(out)
}

fn read_all(mut reader: impl Read) -> io::Result<Vec<u8>> {
    let mut output = Vec::new();
    reader.read_to_end(&mut output)?;
    Ok(output)
}

fn collect_output(
    stdout_reader: JoinHandle<io::Result<Vec<u8>>>,
    stderr_reader: JoinHandle<io::Result<Vec<u8>>>,
) -> String {
    let stdout = read_output(stdout_reader, "stdout");
    let stderr = read_output(stderr_reader, "stderr");
    [stdout, stderr]
        .into_iter()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn read_output(reader: JoinHandle<io::Result<Vec<u8>>>, stream: &str) -> String {
    match reader.join() {
        Ok(Ok(bytes)) => String::from_utf8_lossy(&bytes).trim().to_owned(),
        Ok(Err(error)) => format!("cannot read git {stream}: {error}"),
        Err(_) => format!("git {stream} reader panicked"),
    }
}

fn with_diagnostics(reason: &str, output: &str) -> String {
    if output.is_empty() {
        reason.to_owned()
    } else {
        format!("{reason}\n{output}")
    }
}

fn stop_process_group(child: &mut Child, pid: u32) {
    signal_process_group(pid, false);
    let deadline = Instant::now() + TERMINATE_GRACE;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => return,
            Ok(None) if Instant::now() < deadline => thread::sleep(POLL_INTERVAL),
            _ => break,
        }
    }
    signal_process_group(pid, true);
    #[cfg(not(unix))]
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(unix)]
fn signal_process_group(pid: u32, force: bool) {
    let signal = if force { "-KILL" } else { "-TERM" };
    // Use the system utility: this crate forbids unsafe code. The negative
    // pid targets the process group created for the Git command.
    let _ = Command::new("/bin/kill")
        .arg(signal)
        .arg(format!("-{pid}"))
        .status();
}

#[cfg(not(unix))]
fn signal_process_group(_pid: u32, _force: bool) {}

/// `git fetch <remote>`, or plain `git fetch` (every remote, git's own
/// default) when `remote` is `None`.
pub(super) fn fetch(repo: &Repository, remote: Option<&str>) -> GitResult<String> {
    let workdir = workdir(repo)?;
    let mut args = vec!["fetch".to_owned()];
    if let Some(name) = remote {
        args.push(name.to_owned());
    }
    run_git(workdir, &args, None, GitError::FetchFailed)
}

pub(crate) fn fetch_cancellable(
    repo: &Repository,
    remote: Option<&str>,
    cancel: &AtomicBool,
) -> GitResult<String> {
    let workdir = workdir(repo)?;
    let mut args = vec!["fetch".to_owned()];
    if let Some(name) = remote {
        args.push(name.to_owned());
    }
    run_git(workdir, &args, Some(cancel), GitError::FetchFailed)
}

/// `git pull`. No flags: `pull.rebase` / `pull.ff` decide the shape, same
/// as any other `git config` this backend already defers to.
pub(super) fn pull(repo: &Repository) -> GitResult<String> {
    let workdir = workdir(repo)?;
    run_git(workdir, &["pull".to_owned()], None, GitError::PullFailed)
}

pub(crate) fn pull_cancellable(repo: &Repository, cancel: &AtomicBool) -> GitResult<String> {
    let workdir = workdir(repo)?;
    run_git(
        workdir,
        &["pull".to_owned()],
        Some(cancel),
        GitError::PullFailed,
    )
}

/// `HEAD`'s branch name, needed to spell out `git push -u <remote>
/// <branch>` (git does not infer the branch from `-u` alone the first
/// time an upstream is set).
fn current_branch_name(repo: &Repository) -> GitResult<String> {
    let head = repo.head().map_err(GitError::Read)?;
    head.shorthand()
        .map(str::to_owned)
        .map_err(|_| GitError::PushFailed("HEAD is not on a branch".to_owned()))
}

/// `git push`, or `git push -u <remote> <branch>` when `set_upstream` is
/// `Some(remote)` — `<remote>` is chosen by `App`, not here. A plain push
/// with no upstream configured fails with git's own stable "has no
/// upstream branch" message; detected by substring, same technique
/// `branch.rs`'s unmerged-delete detection already uses, and reported as
/// `GitError::NoUpstream` rather than a generic `PushFailed` so `App` can
/// act on it (offer `-u`) instead of just displaying it.
pub(super) fn push(repo: &Repository, set_upstream: Option<&str>) -> GitResult<String> {
    push_with_lease(repo, set_upstream, false)
}

fn push_with_lease(
    repo: &Repository,
    set_upstream: Option<&str>,
    force_with_lease: bool,
) -> GitResult<String> {
    let workdir = workdir(repo)?;
    let mut args = vec!["push".to_owned()];
    if force_with_lease {
        args.push("--force-with-lease".to_owned());
    }
    if let Some(remote) = set_upstream {
        let branch = current_branch_name(repo)?;
        args.push("-u".to_owned());
        args.push(remote.to_owned());
        args.push(branch);
    }

    run_push(workdir, &args, None, set_upstream)
}

pub(crate) fn push_cancellable(
    repo: &Repository,
    set_upstream: Option<&str>,
    force_with_lease: bool,
    cancel: &AtomicBool,
) -> GitResult<String> {
    let workdir = workdir(repo)?;
    let mut args = vec!["push".to_owned()];
    if force_with_lease {
        args.push("--force-with-lease".to_owned());
    }
    if let Some(remote) = set_upstream {
        args.push("-u".to_owned());
        args.push(remote.to_owned());
        args.push(current_branch_name(repo)?);
    }
    run_push(workdir, &args, Some(cancel), set_upstream)
}

fn run_push(
    workdir: &Path,
    args: &[String],
    cancel: Option<&AtomicBool>,
    set_upstream: Option<&str>,
) -> GitResult<String> {
    let out = run_command(workdir, args, cancel, &GitError::PushFailed)?;
    let combined = combined_output(&out);
    if out.status.success() {
        return Ok(combined);
    }
    if set_upstream.is_none() && combined.contains("has no upstream branch") {
        return Err(GitError::NoUpstream);
    }
    Err(GitError::PushFailed(combined))
}
