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
//! `git2` or runs `git` is `crate::infra::git::branch`.

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
