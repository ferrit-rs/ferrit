//! The git types and pure logic (model, diff parsing, statistics, hosting rules)
//! and the `GitPort` traits. Nothing under `domain::git` imports `ratatui` or
//! `git2`; the code that reads with `git2` or runs `git` for each module here
//! is the module of the same name in `crate::git::repo`.
//!
//! See `docs/PLAN_2_GIT_BACKEND.md`: Status, Files, Branches, Commits, Stash
//! and blob reads are all wired to real `git2` reads (G0..G6). Since
//! `docs/PLAN_6_STAGING.md`, `docs/PLAN_7_COMMIT.md` and
//! `docs/PLAN_8_BRANCHES.md`, `apply`, `commit` and `branch` also write the
//! index, worktree and `HEAD` (stage / unstage / discard, commit / amend /
//! reword / fixup / squash, checkout / create / delete / fast-forward /
//! merge); those are the three submodules that are not read-only, and all
//! three shell out to `git` rather than writing objects directly.

#![warn(missing_docs)]

pub mod apply;
pub mod askpass;
pub(crate) mod authorship;
pub mod blob;
pub mod branch;
pub mod command_log;
pub mod commit;
pub mod config;
pub mod config_edit;
pub mod config_keys;
pub mod create_remote;
pub mod diff;
pub mod error;
pub(crate) mod exec;
#[cfg(feature = "test-util")]
pub mod fake;
pub mod host;
pub mod identity;
pub mod image;
pub mod model;
pub mod operation;
pub mod port;
pub(crate) mod process;
pub mod profile;
pub mod rebase;
pub mod remote;
pub mod repo;
pub mod ssh_config;
pub mod staging;
pub mod stash;
pub mod stats;

use self::model::{BranchEntry, CommitEntry, FileEntry, RemoteEntry, StashEntry, StatusHeader};

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
