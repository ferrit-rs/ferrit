//! Branches and stashes: the refs a user moves around. Merge, and restore a
//! stash entry, are pure decisions over `GitPort`; the code that reads with
//! `git2` or runs `git` is `crate::git::repo::{branches, stashes}`.
//!
//! Branch writes shell out to `git` so hooks and git's own safety messages
//! apply (`docs/PLAN_8_BRANCHES.md`); stash entries are plain owned values
//! resolved by oid to `stash@{n}` right before the command
//! (`docs/PLAN_10_STASH.md`).

use crate::git::error::GitResult;
use crate::git::port::GitPort;

/// What a merge actually did. Not a plain `()`: "it worked" has two shapes
/// ferrit's UI treats differently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeOutcome {
    /// Exit 0: a merge commit (or a fast-forward git decided to do anyway)
    /// landed clean.
    Merged,
    /// Exit non-zero, but `repo.state()` shows a merge in progress
    /// (`MERGE_HEAD` written, conflict markers in the worktree) rather
    /// than some other failure. Ordinary, expected git behaviour ferrit
    /// currently has no UI for finishing, so this is not a `GitError`.
    Conflicted,
}

/// How a merge is done: the choices of the `M` menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeKind {
    /// Fast-forward when history allows, else a merge commit.
    Regular,
    /// Always a merge commit.
    NoFf,
    /// Stage the branch's changes without committing.
    Squash,
    /// Squash the branch's changes into one new commit.
    SquashCommit,
}

/// Merge `name` into the current branch the way `kind` says.
pub fn merge(repo: &dyn GitPort, name: &str, kind: MergeKind) -> GitResult<MergeOutcome> {
    match kind {
        MergeKind::Regular => repo.merge_branch(name),
        MergeKind::NoFf => repo.merge_branch_no_ff(name),
        MergeKind::Squash => repo
            .merge_squash(name, false)
            .map(|()| MergeOutcome::Merged),
        MergeKind::SquashCommit => repo.merge_squash(name, true).map(|()| MergeOutcome::Merged),
    }
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

/// Apply or pop the stash entry `oid`.
pub fn restore(repo: &mut dyn GitPort, oid: &str, pop: bool) -> GitResult<StashOutcome> {
    if pop {
        repo.stash_pop(oid)
    } else {
        repo.stash_apply(oid)
    }
}
