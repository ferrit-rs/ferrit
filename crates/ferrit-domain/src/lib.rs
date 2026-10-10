//! Ferrit's Git domain and application ports.
//!
//! This crate has no terminal, `git2`, or concrete subprocess adapter. The
//! application depends on these types and traits; `ferrit::git::repo` provides
//! the real adapter and `ferrit::git::fake` provides the test adapter.

#![warn(missing_docs)]

pub mod apply;
pub mod commit;
pub mod config;
pub mod diff;
pub mod error;
pub mod host;
pub mod identity;
pub mod model;
pub mod port;
pub mod rebase;
pub mod refs;
pub mod remote;
pub mod staging;
pub mod stats;

/// How long a network command may run before it is given up on.
pub const REMOTE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(300);

use model::{BranchEntry, CommitEntry, FileEntry, RemoteEntry, StashEntry, StatusHeader};

/// Everything the wired panes need from one refresh.
#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    /// The one-glance header: branch, upstream, ahead and behind.
    pub header: StatusHeader,
    /// Every path with a change.
    pub files: Vec<FileEntry>,
    /// Local branches, newest tip first.
    pub branches: Vec<BranchEntry>,
    /// The newest commits on the current branch.
    pub commits: Vec<CommitEntry>,
    /// Stash entries, newest first.
    pub stashes: Vec<StashEntry>,
    /// Feeds the Branches pane's Remotes tab. `docs/PLAN_9_REMOTE.md`.
    pub remotes: Vec<RemoteEntry>,
    /// A merge, rebase, cherry-pick or revert stopped mid-way, if any.
    pub operation: Option<model::Operation>,
}
