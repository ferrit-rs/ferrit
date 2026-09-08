//! Headless, read-only git backend. Nothing under `git::` imports `ratatui`.
//!
//! See `docs/PLAN_2_GIT_BACKEND.md`: Status, Files, Branches, Commits, Stash
//! and blob reads are all wired to real `git2` reads (G0..G6).

mod blob;
mod diff;
mod error;
mod log;
mod model;
mod refs;
mod stash;
mod status;

use std::path::Path;

use git2::Repository;

pub use blob::Rev;
pub use diff::{Diff, DiffOpts, DiffSide, FileMeta, FileStatus, HunkMeta, parse_diff};
pub use error::{GitError, GitResult};
pub use model::{BranchEntry, CommitEntry, StashEntry};
pub use status::{Change, FileEntry, StatusHeader};

/// How many commits `Repo::snapshot()` reads for the Commits pane. Plain
/// constant until the pane grows real scrolling/paging.
const COMMITS_LIMIT: usize = 200;

/// An open repository. Wraps `git2::Repository` and hands out owned snapshots.
pub struct Repo {
    inner: Repository,
}

/// Everything the wired panes need from one refresh.
#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub header: StatusHeader,
    pub files: Vec<FileEntry>,
    pub branches: Vec<BranchEntry>,
    pub commits: Vec<CommitEntry>,
    pub stashes: Vec<StashEntry>,
}

/// Abbreviated hash, the 7 hex chars `git` shows by default. Shared by
/// `status` (upstream not needed there, but commits/refs both want it).
pub(crate) fn short_hash(oid: &git2::Oid) -> String {
    oid.to_string().chars().take(7).collect()
}

impl Repo {
    /// Open the repository at or above `path`. Walks up like `git` does.
    pub fn open(path: &Path) -> GitResult<Repo> {
        let inner = Repository::discover(path).map_err(|e| {
            if e.code() == git2::ErrorCode::NotFound {
                GitError::NotARepository(path.to_path_buf())
            } else {
                GitError::Open(e)
            }
        })?;
        Ok(Repo { inner })
    }

    /// The repository's directory name, e.g. `ferrit`. Used in the status
    /// header (`ferrit -> main`). Falls back to `"repo"` for odd layouts.
    pub fn name(&self) -> String {
        self.inner
            .workdir()
            .and_then(|w| w.file_name())
            .or_else(|| self.inner.path().parent().and_then(|p| p.file_name()))
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "repo".to_string())
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
    pub fn file_diff(&self, path: &Path, side: DiffSide, opts: &DiffOpts) -> GitResult<Diff> {
        diff::file_diff(&self.inner, path, side, opts)
    }

    /// One commit's diff against its first parent (`git show <hash>`), parsed.
    pub fn commit_diff(&self, hash: &str, opts: &DiffOpts) -> GitResult<Diff> {
        diff::commit_diff(&self.inner, hash, opts)
    }
}
