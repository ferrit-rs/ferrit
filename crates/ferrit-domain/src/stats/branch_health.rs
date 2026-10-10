//! Branch health against the main branch, and the newest tag reachable from
//! `HEAD`. All types are plain owned values; no `git2` type escapes.
//!
//! This file holds the types and the pure functions. The code that reads with
//! `git2` or runs `git` is `crate::git::repo::statistics::branches`.

/// A local branch whose tip is older than this (and is not checked out) is stale.
pub const STALE_SECONDS: i64 = 60 * 86_400;

/// How a branch stands against the main branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VsMain {
    /// Commits the branch has that the main branch does not.
    pub ahead: usize,
    /// Commits the main branch has that the branch does not.
    pub behind: usize,
    /// The tip is already in the main branch (never true for the main branch
    /// itself, and true for a branch that has not moved off it).
    pub merged: bool,
}

/// One local branch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchHealth {
    /// The branch's name.
    pub name: String,
    /// It is the checked-out branch.
    pub current: bool,
    /// Committer time of the tip, unix seconds.
    pub tip_time: i64,
    /// `None` when there is no main branch to compare with.
    pub vs_main: Option<VsMain>,
    /// Not the current branch and the tip is older than 60 days. Always
    /// `false` without a main branch: the screen omits stale then.
    pub stale: bool,
}

/// The newest tag reachable from `HEAD`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagSince {
    /// The tag's name.
    pub name: String,
    /// Commits in `HEAD` that the tag does not have.
    pub commits: usize,
}
