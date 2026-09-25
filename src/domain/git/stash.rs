//! Stash entries, stack order (`stash@{0}` = most recent).
//!
//! All types here are plain owned values. No `git2` type escapes this module.
//!
//! Writes shell out to `git` (hooks and git's own safety messages apply),
//! resolving an entry by its stable oid to `stash@{n}` right before the
//! command. See `docs/PLAN_10_STASH.md`.

use std::process::Output;

use git2::Repository;

use crate::domain::git::diff::{stderr, workdir};
use crate::domain::git::error::{GitError, GitResult};
use crate::domain::git::exec;
use crate::domain::git::model::StashEntry;

/// Read the stash list. `git2::Repository::stash_foreach` needs `&mut`, so
/// this is the one read in `Repo::snapshot()` that borrows mutably.
pub(super) fn stashes(repo: &mut Repository) -> GitResult<Vec<StashEntry>> {
    let mut out = Vec::new();
    repo.stash_foreach(|index, message, oid| {
        out.push(StashEntry {
            index,
            oid: oid.to_string(),
            message: message.to_owned(),
        });
        true
    })
    .map_err(GitError::Read)?;
    Ok(out)
}

/// What an apply or pop actually did. Not a plain `()`: "it worked" and
/// "it left conflicts" are both ordinary git behaviour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StashOutcome {
    /// Exit 0.
    Done,
    /// Exit non-zero and the index has conflicts. The stash is kept (pop
    /// does not drop on conflict); Files shows `Change::Conflicted`.
    Conflicted,
}

fn git(repo: &Repository, args: &[&str]) -> GitResult<Output> {
    exec::output(exec::git(workdir(repo)?).args(args))
        .map_err(|e| GitError::StashFailed(format!("cannot run git: {e}")))
}

/// `stash@{n}` for the entry with this oid. `git stash drop` refuses a bare
/// oid, and an index read earlier may have shifted since.
fn resolve(repo: &mut Repository, oid: &str) -> GitResult<String> {
    stashes(repo)?
        .into_iter()
        .find(|entry| entry.oid == oid)
        .map(|entry| format!("stash@{{{}}}", entry.index))
        .ok_or_else(|| GitError::StashFailed("stash entry no longer exists".to_owned()))
}

/// `git stash push --include-untracked [-m <message>]`. An empty message
/// lets git write its own `WIP on <branch>: ...`.
pub(super) fn push(repo: &Repository, message: &str, keep_index: bool) -> GitResult<()> {
    let mut args = vec!["stash", "push", "--include-untracked"];
    if keep_index {
        args.push("--keep-index");
    }
    if !message.is_empty() {
        args.extend(["-m", message]);
    }
    let out = git(repo, &args)?;
    if !out.status.success() {
        return Err(GitError::StashFailed(stderr(&out)));
    }
    // A clean tree is exit 0 with this stable line on stdout.
    if String::from_utf8_lossy(&out.stdout).contains("No local changes to save") {
        return Err(GitError::NothingToStash);
    }
    Ok(())
}

/// `git stash apply|pop <stash@{n}>`. A failure that left conflicts in the
/// index is an outcome, not an error.
fn restore(repo: &mut Repository, oid: &str, verb: &str) -> GitResult<StashOutcome> {
    let reference = resolve(repo, oid)?;
    let out = git(repo, &["stash", verb, &reference])?;
    if out.status.success() {
        return Ok(StashOutcome::Done);
    }
    let mut index = repo.index().map_err(GitError::Read)?;
    index.read(true).map_err(GitError::Read)?;
    if index.has_conflicts() {
        return Ok(StashOutcome::Conflicted);
    }
    Err(GitError::StashFailed(stderr(&out)))
}

/// `git stash apply`: the entry stays.
pub(super) fn apply(repo: &mut Repository, oid: &str) -> GitResult<StashOutcome> {
    restore(repo, oid, "apply")
}

/// `git stash pop`: the entry is removed only after a clean apply.
pub(super) fn pop(repo: &mut Repository, oid: &str) -> GitResult<StashOutcome> {
    restore(repo, oid, "pop")
}

/// Give an entry a new message: `git stash store -m <message> <oid>` adds the
/// same commit again at the top, then the old copy, now one further down, is
/// dropped. The renamed entry ends up as `stash@{0}`; git has no in-place rename.
/// Storing the commit that is already on top changes nothing (git sees the same
/// value and writes no reflog entry), so that entry is dropped first; its commit
/// stays in the object database for the `store`.
pub(super) fn rename(repo: &mut Repository, oid: &str, message: &str) -> GitResult<()> {
    let old = stashes(repo)?
        .into_iter()
        .find(|entry| entry.oid == oid)
        .map(|entry| entry.index)
        .ok_or_else(|| GitError::StashFailed("stash entry no longer exists".to_owned()))?;
    if old == 0 {
        run(repo, &["stash", "drop", "stash@{0}"])?;
        run(repo, &["stash", "store", "-m", message, oid])
    } else {
        run(repo, &["stash", "store", "-m", message, oid])?;
        run(repo, &["stash", "drop", &format!("stash@{{{}}}", old + 1)])
    }
}

/// Run one `git stash` command, a non-zero exit being a `StashFailed`.
fn run(repo: &Repository, args: &[&str]) -> GitResult<()> {
    let out = git(repo, args)?;
    if out.status.success() {
        Ok(())
    } else {
        Err(GitError::StashFailed(stderr(&out)))
    }
}

/// `git stash drop`.
pub(super) fn drop_entry(repo: &mut Repository, oid: &str) -> GitResult<()> {
    let reference = resolve(repo, oid)?;
    run(repo, &["stash", "drop", &reference])
}
