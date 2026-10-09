//! Stash entries, stack order (`stash@{0}` = most recent).
//!
//! All types here are plain owned values. No `git2` type escapes this module.
//!
//! Writes shell out to `git` (hooks and git's own safety messages apply),
//! resolving an entry by its stable oid to `stash@{n}` right before the
//! command. See `docs/PLAN_10_STASH.md`.
//!
//! This file holds the types and the pure functions. The code that reads with
//! `git2` or runs `git` is `crate::infra::git::stash`.

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
