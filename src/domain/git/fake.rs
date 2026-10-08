//! An in-memory git for tests. It answers the port from a `Snapshot` it holds,
//! applies the few changes the app logic tests need (stage, unstage, discard,
//! commit, checkout, create and delete a branch), records every call, and can
//! be told to fail the next call of a method. Everything else reports that it
//! is not implemented. `tests/fake_git_contract.rs` runs the same scenarios
//! against `Repo` and `FakeGit` so the two cannot drift apart.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use super::Snapshot;
use super::apply::{ApplyDir, ApplyTarget};
use super::blob::Rev;
use super::branch::MergeOutcome;
use super::commit::{CommitKind, CommitOpts};
use super::config::{ConfigView, ValueKind, WriteScope};
use super::diff::{Diff, DiffOpts, DiffSide};
use super::error::{GitError, GitResult};
use super::host::{CreateRequest, CreatedRepo, GhProgram};
use super::model::{BranchEntry, Change, CommitEntry, FileEntry, PushState, RemoteEntry};
use super::operation::{OperationOutcome, Step};
use super::port::{
    GitBranches, GitConfig, GitHistory, GitIndex, GitPort, GitRead, GitRemote, GitStash,
};
use super::rebase::RebaseEdit;
use super::stash::StashOutcome;
use super::stats::{RepoStats, StatsOptions, Window};
use crate::domain::profile::settings::{Identity, IdentitySource};

/// An in-memory repository. Clones share one state, which is what
/// `GitPort::reopen` hands a worker thread.
#[derive(Debug, Clone)]
pub struct FakeGit {
    path: PathBuf,
    state: Arc<Mutex<State>>,
}

#[derive(Debug, Default)]
struct State {
    name: String,
    snapshot: Snapshot,
    calls: Vec<&'static str>,
    failures: Vec<(&'static str, GitError)>,
    commits_made: u64,
}

fn unsupported(method: &str) -> GitError {
    GitError::OperationFailed(format!("FakeGit does not implement {method}"))
}

impl FakeGit {
    /// An empty repository called `name`, on branch `main`.
    pub fn new(name: &str) -> Self {
        let mut state = State {
            name: name.to_owned(),
            ..State::default()
        };
        "main".clone_into(&mut state.snapshot.header.branch);
        state.snapshot.branches.push(BranchEntry {
            name: "main".to_owned(),
            is_head: true,
            upstream: None,
            ahead: 0,
            behind: 0,
            tip_time: 0,
        });
        Self {
            path: PathBuf::from(format!("/fake/{name}")),
            state: Arc::new(Mutex::new(state)),
        }
    }

    /// Add a changed file.
    #[must_use]
    pub fn with_file(self, path: &str, staged: Change, worktree: Change) -> Self {
        self.lock().snapshot.files.push(FileEntry {
            path: PathBuf::from(path),
            staged,
            worktree,
            binary: false,
        });
        self
    }

    /// Add a local branch that is not checked out.
    #[must_use]
    pub fn with_branch(self, name: &str) -> Self {
        self.lock().snapshot.branches.push(BranchEntry {
            name: name.to_owned(),
            is_head: false,
            upstream: None,
            ahead: 0,
            behind: 0,
            tip_time: 0,
        });
        self
    }

    /// Add a commit at the top of the history.
    #[must_use]
    pub fn with_commit(self, summary: &str) -> Self {
        self.lock().push_commit(summary);
        self
    }

    /// Make the next call of `method` fail with `error`.
    #[must_use]
    pub fn fail_next(self, method: &'static str, error: GitError) -> Self {
        self.lock().failures.push((method, error));
        self
    }

    /// The methods called so far, in order.
    pub fn calls(&self) -> Vec<&'static str> {
        self.lock().calls.clone()
    }

    /// What a refresh would show right now.
    pub fn current(&self) -> Snapshot {
        self.lock().snapshot.clone()
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Record a call, and fail it when a failure was queued for it.
    fn enter(&self, method: &'static str) -> GitResult<()> {
        let mut state = self.lock();
        state.calls.push(method);
        match state.failures.iter().position(|(name, _)| *name == method) {
            Some(index) => Err(state.failures.remove(index).1),
            None => Ok(()),
        }
    }
}

impl State {
    fn push_commit(&mut self, summary: &str) -> String {
        self.commits_made += 1;
        let full_hash = format!("{:040x}", self.commits_made);
        self.snapshot.commits.insert(
            0,
            CommitEntry {
                short_hash: full_hash.chars().take(7).collect(),
                full_hash: full_hash.clone(),
                author: "Fake User".to_owned(),
                author_email: "fake@example.com".to_owned(),
                summary: summary.to_owned(),
                body: String::new(),
                time: 0,
                refs: Vec::new(),
                push_state: PushState::Unpushed,
            },
        );
        full_hash
    }

    /// Stage (`Forward`) or unstage (`Reverse`) one path, or every file.
    fn move_between_sides(&mut self, only: Option<&Path>, dir: ApplyDir) -> GitResult<()> {
        let mut found = only.is_none();
        for file in &mut self.snapshot.files {
            if only.is_some_and(|path| file.path != path) {
                continue;
            }
            found = true;
            match dir {
                ApplyDir::Forward if file.worktree != Change::None => {
                    file.staged = if file.worktree == Change::Untracked {
                        Change::Added
                    } else {
                        file.worktree
                    };
                    file.worktree = Change::None;
                },
                ApplyDir::Reverse if file.staged != Change::None => {
                    file.worktree = if file.staged == Change::Added {
                        Change::Untracked
                    } else {
                        file.staged
                    };
                    file.staged = Change::None;
                },
                _ => {},
            }
        }
        if found {
            Ok(())
        } else {
            Err(GitError::ApplyFailed("no such file".to_owned()))
        }
    }

    fn commit(&mut self, kind: &CommitKind, message: &str) -> GitResult<String> {
        if !matches!(kind, CommitKind::Normal) {
            return Err(unsupported("commit (amend, reword, fixup, squash)"));
        }
        if !self.snapshot.files.iter().any(FileEntry::has_staged) {
            return Err(GitError::NothingStaged);
        }
        self.snapshot.files.retain_mut(|file| {
            file.staged = Change::None;
            file.worktree != Change::None
        });
        Ok(self.push_commit(message))
    }

    fn checkout(&mut self, name: &str) -> GitResult<()> {
        if !self
            .snapshot
            .branches
            .iter()
            .any(|branch| branch.name == name)
        {
            return Err(GitError::CheckoutFailed(format!("no such branch: {name}")));
        }
        for branch in &mut self.snapshot.branches {
            branch.is_head = branch.name == name;
        }
        name.clone_into(&mut self.snapshot.header.branch);
        Ok(())
    }

    fn create_branch(&mut self, name: &str) -> GitResult<()> {
        if self
            .snapshot
            .branches
            .iter()
            .any(|branch| branch.name == name)
        {
            return Err(GitError::BranchFailed(format!(
                "a branch named '{name}' already exists"
            )));
        }
        for branch in &mut self.snapshot.branches {
            branch.is_head = false;
        }
        self.snapshot.branches.push(BranchEntry {
            name: name.to_owned(),
            is_head: true,
            upstream: None,
            ahead: 0,
            behind: 0,
            tip_time: 0,
        });
        name.clone_into(&mut self.snapshot.header.branch);
        Ok(())
    }

    fn delete_branch(&mut self, name: &str) -> GitResult<()> {
        match self
            .snapshot
            .branches
            .iter()
            .position(|branch| branch.name == name)
        {
            Some(index) if self.snapshot.branches.get(index).is_some_and(|b| b.is_head) => Err(
                GitError::BranchFailed(format!("cannot delete branch '{name}' used by worktree")),
            ),
            Some(index) => {
                self.snapshot.branches.remove(index);
                Ok(())
            },
            None => Err(GitError::BranchFailed(format!("branch '{name}' not found"))),
        }
    }
}

impl GitPort for FakeGit {
    fn reopen(&self) -> GitResult<Box<dyn GitPort>> {
        Ok(Box::new(self.clone()))
    }
}

impl GitRead for FakeGit {
    fn path(&self) -> &Path {
        &self.path
    }
    fn name(&self) -> String {
        self.lock().name.clone()
    }
    fn user_name(&self) -> Option<String> {
        Some("Fake User".to_owned())
    }
    fn identity_settings(
        &self,
    ) -> (
        Vec<Identity>,
        Option<Identity>,
        Option<Identity>,
        IdentitySource,
    ) {
        (Vec::new(), None, None, IdentitySource::Unset)
    }
    fn stats_with(
        &self,
        window: Window,
        opts: &StatsOptions,
        cancel: &AtomicBool,
    ) -> GitResult<RepoStats> {
        let _ = (window, opts, cancel);
        self.enter("stats_with")?;
        Err(unsupported("stats_with"))
    }
    fn snapshot(&mut self) -> GitResult<Snapshot> {
        self.enter("snapshot")?;
        Ok(self.lock().snapshot.clone())
    }
    fn workdir(&self) -> Option<&Path> {
        None
    }
    fn blob_bytes(&self, path: &Path, rev: Rev) -> GitResult<Vec<u8>> {
        let _ = (path, rev);
        self.enter("blob_bytes")?;
        Err(unsupported("blob_bytes"))
    }
    fn file_diff(&self, path: &Path, side: DiffSide, opts: DiffOpts) -> GitResult<Diff> {
        let _ = (path, side, opts);
        self.enter("file_diff")?;
        Err(unsupported("file_diff"))
    }
    fn commit_diff(&self, hash: &str, opts: DiffOpts) -> GitResult<Diff> {
        let _ = (hash, opts);
        self.enter("commit_diff")?;
        Err(unsupported("commit_diff"))
    }
    fn stash_diff(&self, oid: &str, header: &str, opts: DiffOpts) -> GitResult<Diff> {
        let _ = (oid, header, opts);
        self.enter("stash_diff")?;
        Err(unsupported("stash_diff"))
    }
    fn branch_log(&self, branch: &str) -> GitResult<Vec<CommitEntry>> {
        let _ = (branch,);
        self.enter("branch_log")?;
        Err(unsupported("branch_log"))
    }
    fn commit_message(&self, hash: &str) -> GitResult<String> {
        let _ = (hash,);
        self.enter("commit_message")?;
        Err(unsupported("commit_message"))
    }
    fn head_message(&self) -> GitResult<Option<String>> {
        self.enter("head_message")?;
        Ok(self
            .lock()
            .snapshot
            .commits
            .first()
            .map(|commit| commit.summary.clone()))
    }
    fn has_commits(&self) -> bool {
        !self.lock().snapshot.commits.is_empty()
    }
    fn commit_template(&self) -> Option<String> {
        None
    }
    fn has_conflict_markers(&self, path: &Path) -> GitResult<bool> {
        let _ = (path,);
        self.enter("has_conflict_markers")?;
        Err(unsupported("has_conflict_markers"))
    }
    fn remotes(&self) -> GitResult<Vec<RemoteEntry>> {
        self.enter("remotes")?;
        Ok(self.lock().snapshot.remotes.clone())
    }
}

impl GitIndex for FakeGit {
    fn stage_file(&self, path: &Path, dir: ApplyDir) -> GitResult<()> {
        self.enter("stage_file")?;
        self.lock().move_between_sides(Some(path), dir)
    }
    fn stage_all(&self, dir: ApplyDir) -> GitResult<()> {
        self.enter("stage_all")?;
        self.lock().move_between_sides(None, dir)
    }
    fn stage_all_except(&self, excluded: &[PathBuf]) -> GitResult<()> {
        let _ = (excluded,);
        self.enter("stage_all_except")?;
        Err(unsupported("stage_all_except"))
    }
    fn discard_file(&self, path: &Path, untracked: bool) -> GitResult<()> {
        self.enter("discard_file")?;
        let _ = untracked;
        self.lock().snapshot.files.retain(|file| file.path != path);
        Ok(())
    }
    fn apply_hunk(&self, patch: &str, dir: ApplyDir, target: ApplyTarget) -> GitResult<()> {
        let _ = (patch, dir, target);
        self.enter("apply_hunk")?;
        Err(unsupported("apply_hunk"))
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
        let _ = (file_header, hunk_header, hunk_body, lines, dir, target);
        self.enter("apply_lines")?;
        Err(unsupported("apply_lines"))
    }
    fn take_side(&self, path: &Path, ours: bool) -> GitResult<()> {
        let _ = (path, ours);
        self.enter("take_side")?;
        Err(unsupported("take_side"))
    }
}

impl GitHistory for FakeGit {
    fn commit(&self, kind: &CommitKind, message: &str, opts: CommitOpts) -> GitResult<String> {
        self.enter("commit")?;
        let _ = (message, opts);
        self.lock().commit(kind, message)
    }
    fn initial_commit(&self, author: Option<String>) -> GitResult<bool> {
        let _ = (author,);
        self.enter("initial_commit")?;
        Err(unsupported("initial_commit"))
    }
    fn rebase_edit(&self, hash: &str, edit: &RebaseEdit) -> GitResult<OperationOutcome> {
        let _ = (hash, edit);
        self.enter("rebase_edit")?;
        Err(unsupported("rebase_edit"))
    }
    fn autosquash(&self, hash: &str) -> GitResult<OperationOutcome> {
        let _ = (hash,);
        self.enter("autosquash")?;
        Err(unsupported("autosquash"))
    }
    fn operation_step(&self, step: Step) -> GitResult<OperationOutcome> {
        let _ = (step,);
        self.enter("operation_step")?;
        Err(unsupported("operation_step"))
    }
}

impl GitBranches for FakeGit {
    fn checkout(&self, name: &str) -> GitResult<()> {
        self.enter("checkout")?;
        self.lock().checkout(name)
    }
    fn create_branch(&self, name: &str) -> GitResult<()> {
        self.enter("create_branch")?;
        self.lock().create_branch(name)
    }
    fn create_branch_at(&self, name: &str, hash: &str) -> GitResult<()> {
        let _ = (name, hash);
        self.enter("create_branch_at")?;
        Err(unsupported("create_branch_at"))
    }
    fn delete_branch(&self, name: &str, force: bool) -> GitResult<()> {
        self.enter("delete_branch")?;
        let _ = force;
        self.lock().delete_branch(name)
    }
    fn rename_branch(&self, old: &str, new: &str) -> GitResult<()> {
        let _ = (old, new);
        self.enter("rename_branch")?;
        Err(unsupported("rename_branch"))
    }
    fn fast_forward(&self, name: &str) -> GitResult<()> {
        let _ = (name,);
        self.enter("fast_forward")?;
        Err(unsupported("fast_forward"))
    }
    fn merge_branch(&self, name: &str) -> GitResult<MergeOutcome> {
        let _ = (name,);
        self.enter("merge_branch")?;
        Err(unsupported("merge_branch"))
    }
    fn merge_branch_no_ff(&self, name: &str) -> GitResult<MergeOutcome> {
        let _ = (name,);
        self.enter("merge_branch_no_ff")?;
        Err(unsupported("merge_branch_no_ff"))
    }
    fn merge_squash(&self, name: &str, commit: bool) -> GitResult<()> {
        let _ = (name, commit);
        self.enter("merge_squash")?;
        Err(unsupported("merge_squash"))
    }
}

impl GitStash for FakeGit {
    fn stash_push(&self, message: &str) -> GitResult<()> {
        let _ = (message,);
        self.enter("stash_push")?;
        Err(unsupported("stash_push"))
    }
    fn stash_push_keeping_index(&self, message: &str) -> GitResult<()> {
        let _ = (message,);
        self.enter("stash_push_keeping_index")?;
        Err(unsupported("stash_push_keeping_index"))
    }
    fn stash_rename(&mut self, oid: &str, message: &str) -> GitResult<()> {
        let _ = (oid, message);
        self.enter("stash_rename")?;
        Err(unsupported("stash_rename"))
    }
    fn stash_apply(&mut self, oid: &str) -> GitResult<StashOutcome> {
        let _ = (oid,);
        self.enter("stash_apply")?;
        Err(unsupported("stash_apply"))
    }
    fn stash_pop(&mut self, oid: &str) -> GitResult<StashOutcome> {
        let _ = (oid,);
        self.enter("stash_pop")?;
        Err(unsupported("stash_pop"))
    }
    fn stash_drop(&mut self, oid: &str) -> GitResult<()> {
        let _ = (oid,);
        self.enter("stash_drop")?;
        Err(unsupported("stash_drop"))
    }
}

impl GitRemote for FakeGit {
    fn set_remote_url(&self, name: &str, url: &str) -> GitResult<()> {
        let _ = (name, url);
        self.enter("set_remote_url")?;
        Err(unsupported("set_remote_url"))
    }
    fn fetch_cancellable(&self, remote: Option<&str>, cancel: &AtomicBool) -> GitResult<String> {
        let _ = (remote, cancel);
        self.enter("fetch_cancellable")?;
        Err(unsupported("fetch_cancellable"))
    }
    fn pull_cancellable(&self, cancel: &AtomicBool) -> GitResult<String> {
        let _ = (cancel,);
        self.enter("pull_cancellable")?;
        Err(unsupported("pull_cancellable"))
    }
    fn push_cancellable(
        &self,
        set_upstream: Option<&str>,
        upstream_branch: Option<&str>,
        force_with_lease: bool,
        set_upstream_current: bool,
        cancel: &AtomicBool,
    ) -> GitResult<String> {
        let _ = (
            set_upstream,
            upstream_branch,
            force_with_lease,
            set_upstream_current,
            cancel,
        );
        self.enter("push_cancellable")?;
        Err(unsupported("push_cancellable"))
    }
    fn push_default_current(&self) -> bool {
        false
    }
    fn create_repo(
        &self,
        gh: &GhProgram,
        req: &CreateRequest,
        cancel: &AtomicBool,
    ) -> GitResult<CreatedRepo> {
        let _ = (gh, req, cancel);
        self.enter("create_repo")?;
        Err(unsupported("create_repo"))
    }
}

impl GitConfig for FakeGit {
    fn config(&self) -> GitResult<ConfigView> {
        self.enter("config")?;
        Err(unsupported("config"))
    }
    fn config_set(
        &self,
        scope: WriteScope,
        key: &str,
        value: &str,
        kind: ValueKind,
    ) -> GitResult<()> {
        let _ = (scope, key, value, kind);
        self.enter("config_set")?;
        Err(unsupported("config_set"))
    }
    fn config_add(
        &self,
        scope: WriteScope,
        key: &str,
        value: &str,
        kind: ValueKind,
    ) -> GitResult<()> {
        let _ = (scope, key, value, kind);
        self.enter("config_add")?;
        Err(unsupported("config_add"))
    }
    fn config_replace_value(
        &self,
        scope: WriteScope,
        key: &str,
        value: &str,
        old: &str,
        kind: ValueKind,
    ) -> GitResult<()> {
        let _ = (scope, key, value, old, kind);
        self.enter("config_replace_value")?;
        Err(unsupported("config_replace_value"))
    }
    fn config_unset_value(&self, scope: WriteScope, key: &str, old: &str) -> GitResult<()> {
        let _ = (scope, key, old);
        self.enter("config_unset_value")?;
        Err(unsupported("config_unset_value"))
    }
    fn config_unset(&self, scope: WriteScope, key: &str) -> GitResult<()> {
        let _ = (scope, key);
        self.enter("config_unset")?;
        Err(unsupported("config_unset"))
    }
    fn isolate_config(&mut self, global: &Path) {
        let _ = global;
    }
}
