//! Checkout, create, delete, fast-forward and merge branches by shelling
//! out to `git`, so hooks (`post-checkout`, `pre-merge-commit`,
//! `post-merge`) and git's own safety messaging (a dirty worktree an
//! checkout would clobber, an unmerged delete, a merge conflict) apply the
//! way they do for the user's own `git`. See `docs/PLAN_8_BRANCHES.md`.
//!
//! Same rule as the rest of `git::`: no `ratatui`, one subprocess per
//! action. Reads (is a branch checked out, what is its upstream) still go
//! through `git2`, the same split `apply.rs`/`commit.rs` already make.
//!
//! This file holds the types and the pure functions. The code that reads with
//! `git2` or runs `git` is `crate::git::repo::branch`.

use super::error::GitResult;
use super::port::GitPort;

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
