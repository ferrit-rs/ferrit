//! Shared repository command helpers and worktree access.

use crate::git::error::{GitError, GitResult};
use crate::git::repo::Repo;
use std::path::Path;
use std::process::Output;

/// Worktree root required by subprocess-backed Git operations.
pub(crate) fn workdir(repo: &git2::Repository) -> GitResult<&Path> {
    repo.workdir()
        .ok_or_else(|| GitError::DiffFailed("bare repository has no working tree".to_owned()))
}

/// Trimmed stderr shared by subprocess-backed Git operations.
pub(crate) fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).trim().to_owned()
}

#[allow(
    clippy::same_name_method,
    reason = "the repository exposes its worktree under the backend role name"
)]
impl Repo {
    /// The worktree root, or `None` for a bare repo.
    pub fn workdir(&self) -> Option<&Path> {
        self.inner.workdir()
    }
}
