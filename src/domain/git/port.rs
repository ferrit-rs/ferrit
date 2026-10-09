//! The port the application uses to reach git. `app` depends on these traits and
//! never on a concrete repository: `Repo` (`git2` plus the `git` subprocess, in `crate::infra::git`) is
//! the adapter, and tests can substitute a fake. See `docs/PLAN_21_GIT_PORT.md`.
//!
//! The port is split by what the app does with git; `GitPort` is the bundle a
//! handle must offer, and the only name `App` needs.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use super::Snapshot;
use crate::domain::git::apply::{ApplyDir, ApplyTarget};
use crate::domain::git::blob::Rev;
use crate::domain::git::branch::MergeOutcome;
use crate::domain::git::commit::{CommitKind, CommitOpts};
use crate::domain::git::config::{ConfigView, ValueKind, WriteScope};
use crate::domain::git::diff::{Diff, DiffOpts, DiffSide};
use crate::domain::git::error::GitResult;
use crate::domain::git::host::{CreateRequest, CreatedRepo, GhProgram};
use crate::domain::git::model::{CommitEntry, RemoteEntry};
use crate::domain::git::operation::{OperationOutcome, Step};
use crate::domain::git::rebase::RebaseEdit;
use crate::domain::git::stash::StashOutcome;
use crate::domain::git::stats::{RepoStats, StatsOptions, Window};
use crate::domain::profile::settings::{Identity, IdentitySource};

/// Reading the repository: snapshots, diffs, blobs, history.
pub trait GitRead {
    /// Where this repository was opened; a worker reopens its own handle from it.
    fn path(&self) -> &Path;
    /// The repository's directory name, e.g. `ferrit`.
    fn name(&self) -> String;
    /// `user.name` as git resolves it here.
    fn user_name(&self) -> Option<String>;
    /// The identities git knows: global ones, the repository's, the effective one and where it comes from.
    fn identity_settings(
        &self,
    ) -> (
        Vec<Identity>,
        Option<Identity>,
        Option<Identity>,
        IdentitySource,
    );
    /// Repository statistics for the dashboard; stops early when `cancel` is set.
    fn stats_with(
        &self,
        window: Window,
        opts: &StatsOptions,
        cancel: &AtomicBool,
    ) -> GitResult<RepoStats>;
    /// Everything the panes need from one refresh.
    fn snapshot(&mut self) -> GitResult<Snapshot>;
    /// The worktree root, `None` for a bare repository.
    fn workdir(&self) -> Option<&Path>;
    /// Raw bytes of `path` at `rev`.
    fn blob_bytes(&self, path: &Path, rev: Rev) -> GitResult<Vec<u8>>;
    /// The diff of one file, worktree or staged.
    fn file_diff(&self, path: &Path, side: DiffSide, opts: DiffOpts) -> GitResult<Diff>;
    /// The diff a commit introduced.
    fn commit_diff(&self, hash: &str, opts: DiffOpts) -> GitResult<Diff>;
    /// The patch a stash holds.
    fn stash_diff(&self, oid: &str, header: &str, opts: DiffOpts) -> GitResult<Diff>;
    /// The commits of one branch.
    fn branch_log(&self, branch: &str) -> GitResult<Vec<CommitEntry>>;
    /// The full message of a commit.
    fn commit_message(&self, hash: &str) -> GitResult<String>;
    /// The message of `HEAD`, `None` before the first commit.
    fn head_message(&self) -> GitResult<Option<String>>;
    /// Whether `HEAD` points at a commit.
    fn has_commits(&self) -> bool;
    /// The `commit.template` file's text, if one is set.
    fn commit_template(&self) -> Option<String>;
    /// Whether `path` still holds `<<<<<<<` markers.
    fn has_conflict_markers(&self, path: &Path) -> GitResult<bool>;
    /// The configured remotes.
    fn remotes(&self) -> GitResult<Vec<RemoteEntry>>;
}

/// Staging and discarding.
pub trait GitIndex {
    /// Stage or unstage one path.
    fn stage_file(&self, path: &Path, dir: ApplyDir) -> GitResult<()>;
    /// Stage or unstage everything.
    fn stage_all(&self, dir: ApplyDir) -> GitResult<()>;
    /// Stage everything except `excluded`.
    fn stage_all_except(&self, excluded: &[PathBuf]) -> GitResult<()>;
    /// Throw away a file's worktree change.
    fn discard_file(&self, path: &Path, untracked: bool) -> GitResult<()>;
    /// Apply a whole hunk to the index or the worktree.
    fn apply_hunk(&self, patch: &str, dir: ApplyDir, target: ApplyTarget) -> GitResult<()>;
    /// Apply a selection of a hunk's lines.
    fn apply_lines(
        &self,
        file_header: &str,
        hunk_header: &str,
        hunk_body: &str,
        lines: &[usize],
        dir: ApplyDir,
        target: ApplyTarget,
    ) -> GitResult<()>;
    /// Resolve a conflicted file with one side.
    fn take_side(&self, path: &Path, ours: bool) -> GitResult<()>;
}

/// Commits and the operations that rewrite them.
pub trait GitHistory {
    /// Create a commit, or amend, fixup or squash one.
    fn commit(&self, kind: &CommitKind, message: &str, opts: CommitOpts) -> GitResult<String>;
    /// The first, empty commit of a new repository; `false` when there already is one.
    fn initial_commit(&self, author: Option<String>) -> GitResult<bool>;
    /// Reword, drop, squash, fixup or edit one commit.
    fn rebase_edit(&self, hash: &str, edit: &RebaseEdit) -> GitResult<OperationOutcome>;
    /// Fold the `fixup!` and `squash!` commits above `hash`.
    fn autosquash(&self, hash: &str) -> GitResult<OperationOutcome>;
    /// Continue, skip or abort the operation in progress.
    fn operation_step(&self, step: Step) -> GitResult<OperationOutcome>;
}

/// Branches and merges.
pub trait GitBranches {
    /// Check out a branch.
    fn checkout(&self, name: &str) -> GitResult<()>;
    /// Create a branch at `HEAD` and check it out.
    fn create_branch(&self, name: &str) -> GitResult<()>;
    /// Create a branch at a commit.
    fn create_branch_at(&self, name: &str, hash: &str) -> GitResult<()>;
    /// Delete a branch; `GitError::BranchNotMerged` when it needs `force`.
    fn delete_branch(&self, name: &str, force: bool) -> GitResult<()>;
    /// Rename a branch.
    fn rename_branch(&self, old: &str, new: &str) -> GitResult<()>;
    /// Fast-forward a branch to its upstream.
    fn fast_forward(&self, name: &str) -> GitResult<()>;
    /// Merge, fast-forwarding when possible.
    fn merge_branch(&self, name: &str) -> GitResult<MergeOutcome>;
    /// Merge with a merge commit.
    fn merge_branch_no_ff(&self, name: &str) -> GitResult<MergeOutcome>;
    /// Squash-merge, committing or leaving the changes staged.
    fn merge_squash(&self, name: &str, commit: bool) -> GitResult<()>;
}

/// The stash.
pub trait GitStash {
    /// Stash the changes.
    fn stash_push(&self, message: &str) -> GitResult<()>;
    /// Stash the worktree changes and keep the index.
    fn stash_push_keeping_index(&self, message: &str) -> GitResult<()>;
    /// Change a stash's message.
    fn stash_rename(&mut self, oid: &str, message: &str) -> GitResult<()>;
    /// Apply a stash and keep it.
    fn stash_apply(&mut self, oid: &str) -> GitResult<StashOutcome>;
    /// Apply a stash and drop it.
    fn stash_pop(&mut self, oid: &str) -> GitResult<StashOutcome>;
    /// Drop a stash.
    fn stash_drop(&mut self, oid: &str) -> GitResult<()>;
}

/// Remotes and the network.
pub trait GitRemote {
    /// Point a remote at another URL.
    fn set_remote_url(&self, name: &str, url: &str) -> GitResult<()>;
    /// `git fetch`, stoppable through `cancel`.
    fn fetch_cancellable(&self, remote: Option<&str>, cancel: &AtomicBool) -> GitResult<String>;
    /// `git pull`, stoppable through `cancel`.
    fn pull_cancellable(&self, cancel: &AtomicBool) -> GitResult<String>;
    /// `git push`, stoppable through `cancel`.
    fn push_cancellable(
        &self,
        set_upstream: Option<&str>,
        upstream_branch: Option<&str>,
        force_with_lease: bool,
        set_upstream_current: bool,
        cancel: &AtomicBool,
    ) -> GitResult<String>;
    /// Whether a plain `git push` already knows where to go.
    fn push_default_current(&self) -> bool;
    /// Create the repository on GitHub through `gh` and wire `origin`.
    fn create_repo(
        &self,
        gh: &GhProgram,
        req: &CreateRequest,
        cancel: &AtomicBool,
    ) -> GitResult<CreatedRepo>;
}

/// Git configuration.
pub trait GitConfig {
    /// Every key with its scope and origin.
    fn config(&self) -> GitResult<ConfigView>;
    /// Set a key.
    fn config_set(
        &self,
        scope: WriteScope,
        key: &str,
        value: &str,
        kind: ValueKind,
    ) -> GitResult<()>;
    /// Add a value to a multi-valued key.
    fn config_add(
        &self,
        scope: WriteScope,
        key: &str,
        value: &str,
        kind: ValueKind,
    ) -> GitResult<()>;
    /// Replace one value of a multi-valued key.
    fn config_replace_value(
        &self,
        scope: WriteScope,
        key: &str,
        value: &str,
        old: &str,
        kind: ValueKind,
    ) -> GitResult<()>;
    /// Remove one value of a multi-valued key.
    fn config_unset_value(&self, scope: WriteScope, key: &str, old: &str) -> GitResult<()>;
    /// Remove a key.
    fn config_unset(&self, scope: WriteScope, key: &str) -> GitResult<()>;
    /// Read a throwaway global config instead of the user's own. A test seam.
    fn isolate_config(&mut self, global: &Path);
}

/// What `App` holds: every role above, a handle that can move to a worker
/// thread, and a way to open a second handle on the same repository for it.
pub trait GitPort:
    GitRead + GitIndex + GitHistory + GitBranches + GitStash + GitRemote + GitConfig + Send
{
    /// A fresh handle on the same repository, for a worker thread.
    fn reopen(&self) -> GitResult<Box<dyn GitPort>>;
}
