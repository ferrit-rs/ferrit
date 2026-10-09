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

use crate::git::repo::read_error;
use std::path::Path;
use std::sync::atomic::AtomicBool;

use git2::Repository;

use super::diff::workdir;
use crate::git::error::{GitError, GitResult};
use crate::git::model::RemoteEntry;

use crate::git::process::{combined_output, run_command, run_git};

/// Configured remotes, alphabetical.
pub(super) fn remotes(repo: &Repository) -> GitResult<Vec<RemoteEntry>> {
    let mut names: Vec<String> = repo
        .remotes()
        .map_err(read_error)?
        .iter()
        .filter_map(|res| res.ok().flatten())
        .map(str::to_owned)
        .collect();
    names.sort();

    names
        .into_iter()
        .map(|name| {
            let remote = repo.find_remote(&name).map_err(read_error)?;
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
    let head = repo.head().map_err(read_error)?;
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

    run_push(workdir, &args, None, set_upstream, false)
}

pub(crate) fn push_cancellable(
    repo: &Repository,
    set_upstream: Option<&str>,
    upstream_branch: Option<&str>,
    force_with_lease: bool,
    set_upstream_current: bool,
    cancel: &AtomicBool,
) -> GitResult<String> {
    let workdir = workdir(repo)?;
    let mut args = vec!["push".to_owned()];
    if force_with_lease {
        args.push("--force-with-lease".to_owned());
    }
    if set_upstream_current && set_upstream.is_none() {
        args.push("-u".to_owned());
    }
    if let Some(remote) = set_upstream {
        args.push("-u".to_owned());
        args.push(remote.to_owned());
        let local_branch = current_branch_name(repo)?;
        args.push(match upstream_branch {
            Some(branch) => format!("{local_branch}:{branch}"),
            None => local_branch,
        });
    }
    run_push(
        workdir,
        &args,
        Some(cancel),
        set_upstream,
        set_upstream_current,
    )
}

fn run_push(
    workdir: &Path,
    args: &[String],
    cancel: Option<&AtomicBool>,
    set_upstream: Option<&str>,
    set_upstream_current: bool,
) -> GitResult<String> {
    let out = run_command(workdir, args, cancel, &GitError::PushFailed)?;
    let combined = combined_output(&out);
    if out.status.success() {
        return Ok(combined);
    }
    if set_upstream.is_none()
        && !set_upstream_current
        && combined.contains("has no upstream branch")
    {
        return Err(GitError::NoUpstream);
    }
    Err(GitError::PushFailed(combined))
}
