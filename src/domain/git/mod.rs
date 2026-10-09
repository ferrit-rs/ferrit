//! Headless Git adapter. Nothing under `domain::git` imports `ratatui`.
//!
//! See `docs/PLAN_2_GIT_BACKEND.md`: Status, Files, Branches, Commits, Stash
//! and blob reads are all wired to real `git2` reads (G0..G6). Since
//! `docs/PLAN_6_STAGING.md`, `docs/PLAN_7_COMMIT.md` and
//! `docs/PLAN_8_BRANCHES.md`, `apply`, `commit` and `branch` also write the
//! index, worktree and `HEAD` (stage / unstage / discard, commit / amend /
//! reword / fixup / squash, checkout / create / delete / fast-forward /
//! merge); those are the three submodules that are not read-only, and all
//! three shell out to `git` rather than writing objects directly.

pub mod apply;
pub mod askpass;
pub mod blob;
pub mod branch;
pub mod command_log;
pub mod commit;
pub mod config;
pub mod config_keys;
pub mod diff;
pub mod error;
pub(crate) mod exec;
#[cfg(feature = "test-util")]
pub mod fake;
pub mod host;
pub mod model;
pub mod operation;
pub mod port;
pub mod process;
pub mod rebase;
pub mod ssh_config;
pub mod stash;
pub mod stats;

use self::model::{BranchEntry, CommitEntry, FileEntry, RemoteEntry, StashEntry, StatusHeader};

/// Everything the wired panes need from one refresh.
#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub header: StatusHeader,
    pub files: Vec<FileEntry>,
    pub branches: Vec<BranchEntry>,
    pub commits: Vec<CommitEntry>,
    pub stashes: Vec<StashEntry>,
    /// Feeds the Branches pane's Remotes tab. `docs/PLAN_9_REMOTE.md`.
    pub remotes: Vec<RemoteEntry>,
    /// A merge, rebase, cherry-pick or revert stopped mid-way, if any.
    pub operation: Option<model::Operation>,
}
