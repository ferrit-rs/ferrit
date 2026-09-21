//! Headless git backend. Nothing under `git::` imports `ratatui`.
//!
//! See `docs/PLAN_2_GIT_BACKEND.md`: Status, Files, Branches, Commits, Stash
//! and blob reads are all wired to real `git2` reads (G0..G6). Since
//! `docs/PLAN_6_STAGING.md`, `docs/PLAN_7_COMMIT.md` and
//! `docs/PLAN_8_BRANCHES.md`, `apply`, `commit` and `branch` also write the
//! index, worktree and `HEAD` (stage / unstage / discard, commit / amend /
//! reword / fixup / squash, checkout / create / delete / fast-forward /
//! merge); those are the three submodules that are not read-only, and all
//! three shell out to `git` rather than writing objects directly.

pub mod activity;
pub mod apply;
pub mod blob;
pub mod branch;
pub mod commit;
pub mod diff;
pub mod error;
pub mod log;
pub mod model;
pub mod refs;
pub mod remote;
pub mod stash;
pub mod status;

use std::path::Path;
use std::sync::atomic::AtomicBool;

use git2::Repository;

use self::apply::{ApplyDir, ApplyTarget};
use self::blob::Rev;
use self::branch::MergeOutcome;
use self::commit::{CommitKind, CommitOpts};
use self::diff::{Diff, DiffOpts, DiffSide};
use self::error::{GitError, GitResult};
use self::model::{BranchEntry, CommitEntry, StashEntry, UserIdentity};
use self::remote::RemoteEntry;
use self::status::{FileEntry, StatusHeader};

/// How many commits `Repo::snapshot()` reads for the Commits pane. Plain
/// constant until the pane grows real scrolling/paging.
const COMMITS_LIMIT: usize = 200;

fn config_values(config: &git2::Config, key: &str) -> Vec<String> {
    let Ok(mut entries) = config.multivar(key, None) else {
        return Vec::new();
    };
    let mut values = Vec::new();
    while let Some(Ok(entry)) = entries.next() {
        if let Ok(value) = entry.value() {
            values.push(value.to_owned());
        }
    }
    values
}

/// An open repository. Wraps `git2::Repository` and hands out owned snapshots.
pub struct Repo {
    inner: Repository,
    /// Path used to discover this repository. Lets the TUI reopen an owned
    /// handle inside a refresh worker; `git2::Repository` itself stays local.
    reopen_path: std::path::PathBuf,
}

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
}

/// Abbreviated hash, the 7 hex chars `git` shows by default. Shared by
/// `status` (upstream not needed there, but commits/refs both want it).
pub(crate) fn short_hash(oid: &git2::Oid) -> String {
    oid.to_string().chars().take(7).collect()
}

impl Repo {
    /// Open the repository at or above `path`. Walks up like `git` does.
    pub fn open(path: &Path) -> GitResult<Self> {
        let reopen_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let inner = Repository::discover(path).map_err(|e| {
            if e.code() == git2::ErrorCode::NotFound {
                GitError::NotARepository(path.to_path_buf())
            } else {
                GitError::Open(e)
            }
        })?;
        Ok(Self { inner, reopen_path })
    }

    /// Path for opening a fresh handle in a background worker.
    pub fn reopen_path(&self) -> &Path {
        &self.reopen_path
    }

    /// The repository's directory name, e.g. `ferrit`. Used in the status
    /// header (`ferrit -> main`). Falls back to `"repo"` for odd layouts.
    pub fn name(&self) -> String {
        self.inner
            .workdir()
            .and_then(|w| w.file_name())
            .or_else(|| self.inner.path().parent().and_then(|p| p.file_name()))
            .map_or_else(|| "repo".to_owned(), |n| n.to_string_lossy().into_owned())
    }

    /// Configured Git author name (`user.name`), respecting repo, global,
    /// and system config precedence.
    pub fn user_name(&self) -> Option<String> {
        self.inner.config().ok()?.get_string("user.name").ok()
    }

    /// All configured author names paired with email values in config order.
    pub fn user_identities(&self) -> Vec<UserIdentity> {
        let Ok(config) = self.inner.config() else {
            return Vec::new();
        };
        let names = config_values(&config, "user.name");
        let emails = config_values(&config, "user.email");
        names
            .into_iter()
            .enumerate()
            .map(|(index, name)| UserIdentity {
                name,
                email: emails.get(index).cloned(),
            })
            .collect()
    }

    /// Commit timestamps on HEAD, newest first. Activity view aggregates by day.
    pub fn activity(&self) -> GitResult<Vec<i64>> {
        activity::timestamps(&self.inner)
    }

    /// Timestamps of successful pushes performed through Ferrit for this repo.
    pub fn push_activity(&self) -> Vec<i64> {
        activity::push_timestamps(&self.inner)
    }

    /// Repository-local and user-global author config, kept separate for Settings.
    pub fn identity_settings(&self) -> (Option<UserIdentity>, Option<UserIdentity>) {
        fn identity(config: &git2::Config) -> Option<UserIdentity> {
            let name = config.get_string("user.name").ok()?;
            let email = config.get_string("user.email").ok();
            Some(UserIdentity { name, email })
        }
        let global = git2::Config::open_default()
            .ok()
            .and_then(|config| identity(&config));
        let local_path = self.inner.path().join("config");
        let local = git2::Config::open(&local_path)
            .ok()
            .and_then(|config| identity(&config));
        (global, local)
    }

    /// Re-read every wired pane in one go. Partial failure fails the whole call.
    ///
    /// `&mut self`: reading the stash list needs `&mut git2::Repository`.
    pub fn snapshot(&mut self) -> GitResult<Snapshot> {
        Ok(Snapshot {
            header: status::header(&self.inner)?,
            files: status::files(&self.inner)?,
            branches: refs::branches(&self.inner)?,
            commits: log::commits(&self.inner, COMMITS_LIMIT)?,
            stashes: stash::stashes(&mut self.inner)?,
            remotes: remote::remotes(&self.inner)?,
        })
    }

    /// The worktree root, or `None` for a bare repo. The filesystem watcher
    /// recurses from here; `.git` lives under it in the normal layout.
    pub fn workdir(&self) -> Option<&Path> {
        self.inner.workdir()
    }

    /// Raw bytes of `path` at `rev`. Feeds the right-pane image preview.
    pub fn blob_bytes(&self, path: &Path, rev: Rev) -> GitResult<Vec<u8>> {
        blob::blob_bytes(&self.inner, path, rev)
    }

    /// One file's diff (`git diff [--cached] -- <path>`), parsed. Untracked
    /// files come back via `--no-index` as all-additions.
    pub fn file_diff(&self, path: &Path, side: DiffSide, opts: DiffOpts) -> GitResult<Diff> {
        diff::file_diff(&self.inner, path, side, opts)
    }

    /// One commit's diff against its first parent (`git show <hash>`), parsed.
    pub fn commit_diff(&self, hash: &str, opts: DiffOpts) -> GitResult<Diff> {
        diff::commit_diff(&self.inner, hash, opts)
    }

    /// One local branch's own commit history, newest first, bounded by
    /// `COMMITS_LIMIT`. Feeds the Branches pane's Enter-to-drill-down log
    /// (`docs/PLAN_2_GIT_BACKEND.md`, G7).
    pub fn branch_log(&self, branch: &str) -> GitResult<Vec<CommitEntry>> {
        log::commits_for(&self.inner, branch, COMMITS_LIMIT)
    }

    /// Stage or unstage a whole file. No patch: `git add` / `git restore
    /// --staged`. See `docs/PLAN_6_STAGING.md`.
    pub fn stage_file(&self, path: &Path, dir: ApplyDir) -> GitResult<()> {
        apply::stage_file(&self.inner, path, dir)
    }

    /// Stage or unstage every changed file (`a`): `git add -A` / `git
    /// restore --staged .`.
    pub fn stage_all(&self, dir: ApplyDir) -> GitResult<()> {
        apply::stage_all(&self.inner, dir)
    }

    /// Discard a whole file's worktree change, never the index. `untracked`
    /// picks `git clean` (nothing to restore *to*) over `git restore
    /// --worktree`.
    pub fn discard_file(&self, path: &Path, untracked: bool) -> GitResult<()> {
        apply::discard_file(&self.inner, path, untracked)
    }

    /// Stage / unstage / discard one hunk. `patch` is `file.header.start
    /// .. hunk.body.end` over a `Diff::text`; the caller slices it so this
    /// module never re-runs the diff.
    pub fn apply_hunk(&self, patch: &str, dir: ApplyDir, target: ApplyTarget) -> GitResult<()> {
        apply::apply_hunk(&self.inner, patch, dir, target)
    }

    /// Stage / unstage / discard a set of body lines within one hunk.
    /// `lines` are 0-based indices into `hunk_body`'s own lines.
    pub fn apply_lines(
        &self,
        file_header: &str,
        hunk_header: &str,
        hunk_body: &str,
        lines: &[usize],
        dir: ApplyDir,
        target: ApplyTarget,
    ) -> GitResult<()> {
        apply::apply_lines(
            &self.inner,
            file_header,
            hunk_header,
            hunk_body,
            lines,
            dir,
            target,
        )
    }

    /// Run `git commit`. See `docs/PLAN_7_COMMIT.md`.
    pub fn commit(&self, kind: &CommitKind, message: &str, opts: CommitOpts) -> GitResult<String> {
        commit::commit(&self.inner, kind, message, opts)
    }

    /// `HEAD`'s current message, for pre-filling the Amend / Reword popup.
    pub fn head_message(&self) -> GitResult<Option<String>> {
        commit::head_message(&self.inner)
    }

    /// Count of paths staged relative to `HEAD`; the commit popup's
    /// precondition.
    pub fn staged_count(&self) -> GitResult<usize> {
        commit::staged_count(&self.inner)
    }

    /// `git checkout <name>`. See `docs/PLAN_8_BRANCHES.md`.
    pub fn checkout(&self, name: &str) -> GitResult<()> {
        branch::checkout(&self.inner, name)
    }

    /// `git checkout -b <name>` from the current `HEAD`.
    pub fn create_branch(&self, name: &str) -> GitResult<()> {
        branch::create_branch(&self.inner, name)
    }

    /// `git branch -d <name>` (`-D` when `force`).
    pub fn delete_branch(&self, name: &str, force: bool) -> GitResult<()> {
        branch::delete_branch(&self.inner, name, force)
    }

    /// Fast-forward `name` to its upstream, checked out or not.
    pub fn fast_forward(&self, name: &str) -> GitResult<()> {
        branch::fast_forward(&self.inner, name)
    }

    /// `git merge <name>` into the current branch.
    pub fn merge_branch(&self, name: &str) -> GitResult<MergeOutcome> {
        branch::merge_branch(&self.inner, name)
    }

    /// Configured remotes, alphabetical. See `docs/PLAN_9_REMOTE.md`.
    pub fn remotes(&self) -> GitResult<Vec<RemoteEntry>> {
        remote::remotes(&self.inner)
    }

    /// `git fetch <remote>`, or every remote when `remote` is `None`. Slow:
    /// run off the main thread, see `docs/PLAN_9_REMOTE.md` "Approach part 2".
    pub fn fetch(&self, remote: Option<&str>) -> GitResult<String> {
        remote::fetch(&self.inner, remote)
    }

    pub(crate) fn fetch_cancellable(
        &self,
        remote: Option<&str>,
        cancel: &AtomicBool,
    ) -> GitResult<String> {
        remote::fetch_cancellable(&self.inner, remote, cancel)
    }

    /// `git pull`, honouring the user's `pull.rebase`/`pull.ff` config. Slow,
    /// same as `fetch`.
    pub fn pull(&self) -> GitResult<String> {
        remote::pull(&self.inner)
    }

    pub(crate) fn pull_cancellable(&self, cancel: &AtomicBool) -> GitResult<String> {
        remote::pull_cancellable(&self.inner, cancel)
    }

    /// `git push`, or `git push -u <remote> <branch>` when `set_upstream` is
    /// `Some`. Slow, same as `fetch`.
    pub fn push(&self, set_upstream: Option<&str>) -> GitResult<String> {
        let result = remote::push(&self.inner, set_upstream);
        if result.is_ok() {
            activity::record_push(&self.inner);
        }
        result
    }

    pub(crate) fn push_cancellable(
        &self,
        set_upstream: Option<&str>,
        upstream_branch: Option<&str>,
        force_with_lease: bool,
        set_upstream_current: bool,
        cancel: &AtomicBool,
    ) -> GitResult<String> {
        let result = remote::push_cancellable(
            &self.inner,
            set_upstream,
            upstream_branch,
            force_with_lease,
            set_upstream_current,
            cancel,
        );
        if result.is_ok() {
            activity::record_push(&self.inner);
        }
        result
    }

    pub fn push_default_current(&self) -> bool {
        self.inner
            .config()
            .ok()
            .and_then(|config| config.get_string("push.default").ok())
            .is_some_and(|value| value == "current")
    }
}
