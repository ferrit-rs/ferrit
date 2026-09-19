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

use std::path::Path;
use std::process::{Command, Output};

use git2::Repository;

use crate::git::diff::workdir;
use crate::git::error::{GitError, GitResult};

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
fn run_git(workdir: &Path, args: &[String], err: impl Fn(String) -> GitError) -> GitResult<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(workdir)
        .args(args)
        .output()
        .map_err(|e| err(format!("cannot run git: {e}")))?;
    let combined = combined_output(&out);
    if out.status.success() {
        Ok(combined)
    } else {
        Err(err(combined))
    }
}

/// `git fetch <remote>`, or plain `git fetch` (every remote, git's own
/// default) when `remote` is `None`.
pub(super) fn fetch(repo: &Repository, remote: Option<&str>) -> GitResult<String> {
    let workdir = workdir(repo)?;
    let mut args = vec!["fetch".to_owned()];
    if let Some(name) = remote {
        args.push(name.to_owned());
    }
    run_git(workdir, &args, GitError::FetchFailed)
}

/// `git pull`. No flags: `pull.rebase` / `pull.ff` decide the shape, same
/// as any other `git config` this backend already defers to.
pub(super) fn pull(repo: &Repository) -> GitResult<String> {
    let workdir = workdir(repo)?;
    run_git(workdir, &["pull".to_owned()], GitError::PullFailed)
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
    let workdir = workdir(repo)?;
    let mut args = vec!["push".to_owned()];
    if let Some(remote) = set_upstream {
        let branch = current_branch_name(repo)?;
        args.push("-u".to_owned());
        args.push(remote.to_owned());
        args.push(branch);
    }

    let out = Command::new("git")
        .arg("-C")
        .arg(workdir)
        .args(&args)
        .output()
        .map_err(|e| GitError::PushFailed(format!("cannot run git: {e}")))?;
    let combined = combined_output(&out);
    if out.status.success() {
        return Ok(combined);
    }
    if set_upstream.is_none() && combined.contains("has no upstream branch") {
        return Err(GitError::NoUpstream);
    }
    Err(GitError::PushFailed(combined))
}
