//! `Repo` answers the `GitPort` traits by forwarding to its own methods
//! (`crate::git::repo`).

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use crate::git::Snapshot;
use crate::git::apply::{ApplyDir, ApplyTarget};
use crate::git::blob::Rev;
use crate::git::branch::MergeOutcome;
use crate::git::commit::{CommitKind, CommitOpts};
use crate::git::config::{ConfigView, ValueKind, WriteScope};
use crate::git::diff::{Diff, DiffOpts, DiffSide};
use crate::git::error::GitResult;
use crate::git::host::{CreateRequest, CreatedRepo, GhProgram};
use crate::git::identity::{Identity, IdentitySource};
use crate::git::model::{CommitEntry, RemoteEntry};
use crate::git::operation::{OperationOutcome, Step};
use crate::git::rebase::RebaseEdit;
use crate::git::stash::StashOutcome;
use crate::git::stats::{RepoStats, StatsOptions, Window};

use super::Repo;
use crate::git::port::{
    GitBranches, GitConfig, GitHistory, GitIndex, GitPort, GitRead, GitRemote, GitStash,
};

impl GitPort for Repo {
    fn reopen(&self) -> GitResult<Box<dyn GitPort>> {
        Ok(Box::new(Self::open(GitRead::path(self))?))
    }
}

impl GitRead for Repo {
    fn path(&self) -> &Path {
        Self::reopen_path(self)
    }
    fn name(&self) -> String {
        Self::name(self)
    }
    fn user_name(&self) -> Option<String> {
        Self::user_name(self)
    }
    fn identity_settings(
        &self,
    ) -> (
        Vec<Identity>,
        Option<Identity>,
        Option<Identity>,
        IdentitySource,
    ) {
        Self::identity_settings(self)
    }
    fn stats_with(
        &self,
        window: Window,
        opts: &StatsOptions,
        cancel: &AtomicBool,
    ) -> GitResult<RepoStats> {
        Self::stats_with(self, window, opts, cancel)
    }
    fn snapshot(&mut self) -> GitResult<Snapshot> {
        Self::snapshot(self)
    }
    fn workdir(&self) -> Option<&Path> {
        Self::workdir(self)
    }
    fn blob_bytes(&self, path: &Path, rev: Rev) -> GitResult<Vec<u8>> {
        Self::blob_bytes(self, path, rev)
    }
    fn file_diff(&self, path: &Path, side: DiffSide, opts: DiffOpts) -> GitResult<Diff> {
        Self::file_diff(self, path, side, opts)
    }
    fn commit_diff(&self, hash: &str, opts: DiffOpts) -> GitResult<Diff> {
        Self::commit_diff(self, hash, opts)
    }
    fn stash_diff(&self, oid: &str, header: &str, opts: DiffOpts) -> GitResult<Diff> {
        Self::stash_diff(self, oid, header, opts)
    }
    fn branch_log(&self, branch: &str) -> GitResult<Vec<CommitEntry>> {
        Self::branch_log(self, branch)
    }
    fn commit_message(&self, hash: &str) -> GitResult<String> {
        Self::commit_message(self, hash)
    }
    fn head_message(&self) -> GitResult<Option<String>> {
        Self::head_message(self)
    }
    fn has_commits(&self) -> bool {
        Self::has_commits(self)
    }
    fn commit_template(&self) -> Option<String> {
        Self::commit_template(self)
    }
    fn has_conflict_markers(&self, path: &Path) -> GitResult<bool> {
        Self::has_conflict_markers(self, path)
    }
    fn remotes(&self) -> GitResult<Vec<RemoteEntry>> {
        Self::remotes(self)
    }
}

impl GitIndex for Repo {
    fn stage_file(&self, path: &Path, dir: ApplyDir) -> GitResult<()> {
        Self::stage_file(self, path, dir)
    }
    fn stage_all(&self, dir: ApplyDir) -> GitResult<()> {
        Self::stage_all(self, dir)
    }
    fn stage_all_except(&self, excluded: &[PathBuf]) -> GitResult<()> {
        Self::stage_all_except(self, excluded)
    }
    fn discard_file(&self, path: &Path, untracked: bool) -> GitResult<()> {
        Self::discard_file(self, path, untracked)
    }
    fn apply_hunk(&self, patch: &str, dir: ApplyDir, target: ApplyTarget) -> GitResult<()> {
        Self::apply_hunk(self, patch, dir, target)
    }
    fn apply_lines(
        &self,
        file_header: &str,
        hunk_header: &str,
        hunk_body: &str,
        lines: &[usize],
        dir: ApplyDir,
        target: ApplyTarget,
    ) -> GitResult<()> {
        Self::apply_lines(
            self,
            file_header,
            hunk_header,
            hunk_body,
            lines,
            dir,
            target,
        )
    }
    fn take_side(&self, path: &Path, ours: bool) -> GitResult<()> {
        Self::take_side(self, path, ours)
    }
}

impl GitHistory for Repo {
    fn commit(&self, kind: &CommitKind, message: &str, opts: CommitOpts) -> GitResult<String> {
        Self::commit(self, kind, message, opts)
    }
    fn initial_commit(&self, author: Option<String>) -> GitResult<bool> {
        Self::initial_commit(self, author)
    }
    fn rebase_edit(&self, hash: &str, edit: &RebaseEdit) -> GitResult<OperationOutcome> {
        Self::rebase_edit(self, hash, edit)
    }
    fn autosquash(&self, hash: &str) -> GitResult<OperationOutcome> {
        Self::autosquash(self, hash)
    }
    fn operation_step(&self, step: Step) -> GitResult<OperationOutcome> {
        Self::operation_step(self, step)
    }
}

impl GitBranches for Repo {
    fn checkout(&self, name: &str) -> GitResult<()> {
        Self::checkout(self, name)
    }
    fn create_branch(&self, name: &str) -> GitResult<()> {
        Self::create_branch(self, name)
    }
    fn create_branch_at(&self, name: &str, hash: &str) -> GitResult<()> {
        Self::create_branch_at(self, name, hash)
    }
    fn delete_branch(&self, name: &str, force: bool) -> GitResult<()> {
        Self::delete_branch(self, name, force)
    }
    fn rename_branch(&self, old: &str, new: &str) -> GitResult<()> {
        Self::rename_branch(self, old, new)
    }
    fn fast_forward(&self, name: &str) -> GitResult<()> {
        Self::fast_forward(self, name)
    }
    fn merge_branch(&self, name: &str) -> GitResult<MergeOutcome> {
        Self::merge_branch(self, name)
    }
    fn merge_branch_no_ff(&self, name: &str) -> GitResult<MergeOutcome> {
        Self::merge_branch_no_ff(self, name)
    }
    fn merge_squash(&self, name: &str, commit: bool) -> GitResult<()> {
        Self::merge_squash(self, name, commit)
    }
}

impl GitStash for Repo {
    fn stash_push(&self, message: &str) -> GitResult<()> {
        Self::stash_push(self, message)
    }
    fn stash_push_keeping_index(&self, message: &str) -> GitResult<()> {
        Self::stash_push_keeping_index(self, message)
    }
    fn stash_rename(&mut self, oid: &str, message: &str) -> GitResult<()> {
        Self::stash_rename(self, oid, message)
    }
    fn stash_apply(&mut self, oid: &str) -> GitResult<StashOutcome> {
        Self::stash_apply(self, oid)
    }
    fn stash_pop(&mut self, oid: &str) -> GitResult<StashOutcome> {
        Self::stash_pop(self, oid)
    }
    fn stash_drop(&mut self, oid: &str) -> GitResult<()> {
        Self::stash_drop(self, oid)
    }
}

impl GitRemote for Repo {
    fn set_remote_url(&self, name: &str, url: &str) -> GitResult<()> {
        Self::set_remote_url(self, name, url)
    }
    fn fetch_cancellable(&self, remote: Option<&str>, cancel: &AtomicBool) -> GitResult<String> {
        Self::fetch_cancellable(self, remote, cancel)
    }
    fn pull_cancellable(&self, cancel: &AtomicBool) -> GitResult<String> {
        Self::pull_cancellable(self, cancel)
    }
    fn push_cancellable(
        &self,
        set_upstream: Option<&str>,
        upstream_branch: Option<&str>,
        force_with_lease: bool,
        set_upstream_current: bool,
        cancel: &AtomicBool,
    ) -> GitResult<String> {
        Self::push_cancellable(
            self,
            set_upstream,
            upstream_branch,
            force_with_lease,
            set_upstream_current,
            cancel,
        )
    }
    fn push_default_current(&self) -> bool {
        Self::push_default_current(self)
    }
    fn create_repo(
        &self,
        gh: &GhProgram,
        req: &CreateRequest,
        cancel: &AtomicBool,
    ) -> GitResult<CreatedRepo> {
        Self::create_repo(self, gh, req, cancel)
    }
}

impl GitConfig for Repo {
    fn config(&self) -> GitResult<ConfigView> {
        Self::config(self)
    }
    fn config_set(
        &self,
        scope: WriteScope,
        key: &str,
        value: &str,
        kind: ValueKind,
    ) -> GitResult<()> {
        Self::config_set(self, scope, key, value, kind)
    }
    fn config_add(
        &self,
        scope: WriteScope,
        key: &str,
        value: &str,
        kind: ValueKind,
    ) -> GitResult<()> {
        Self::config_add(self, scope, key, value, kind)
    }
    fn config_replace_value(
        &self,
        scope: WriteScope,
        key: &str,
        value: &str,
        old: &str,
        kind: ValueKind,
    ) -> GitResult<()> {
        Self::config_replace_value(self, scope, key, value, old, kind)
    }
    fn config_unset_value(&self, scope: WriteScope, key: &str, old: &str) -> GitResult<()> {
        Self::config_unset_value(self, scope, key, old)
    }
    fn config_unset(&self, scope: WriteScope, key: &str) -> GitResult<()> {
        Self::config_unset(self, scope, key)
    }
    fn isolate_config(&mut self, global: &Path) {
        Self::isolate_config(self, global);
    }
}
