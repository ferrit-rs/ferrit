//! Headless, read-only git backend. Nothing under `git::` imports `ratatui`.
//!
//! Phase 2 wires Status, Files and Branches (see
//! `docs/PLAN_2_GIT_BACKEND.md`); commits, stash and blob reads follow in
//! later milestones.

mod blob;
mod error;
mod model;
mod refs;
mod status;

use std::path::Path;

use git2::Repository;

pub use blob::Rev;
pub use error::{GitError, GitResult};
pub use model::{BranchEntry, CommitEntry, StashEntry};
pub use status::{Change, FileEntry, StatusHeader};

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
    pub fn snapshot(&self) -> GitResult<Snapshot> {
        Ok(Snapshot {
            header: status::header(&self.inner)?,
            files: status::files(&self.inner)?,
            branches: refs::branches(&self.inner)?,
        })
    }

    /// Raw bytes of `path` at `rev`. Feeds the right-pane image preview.
    pub fn blob_bytes(&self, path: &Path, rev: Rev) -> GitResult<Vec<u8>> {
        blob::blob_bytes(&self.inner, path, rev)
    }
}
