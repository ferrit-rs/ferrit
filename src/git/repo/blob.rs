//! Blob reads for the `git2` repository adapter.

use crate::git::diff::Rev;
use crate::git::error::{GitError, GitResult};
use crate::git::repo::{Repo, read_error};
use git2::Repository;
use std::path::Path;

fn read_err(path: &Path, message: impl std::fmt::Display) -> GitError {
    read_error(git2::Error::from_str(&format!(
        "{}: {message}",
        path.display()
    )))
}

/// Read `path` at `rev`. Missing files, bare repos, and non-blob entries
/// come back as `GitError::Read`, never a panic.
pub(crate) fn blob_bytes(repo: &Repository, path: &Path, rev: Rev) -> GitResult<Vec<u8>> {
    match rev {
        Rev::Workdir => {
            let root = repo
                .workdir()
                .ok_or_else(|| read_err(path, "bare repository has no working directory"))?;
            std::fs::read(root.join(path)).map_err(|e| read_err(path, e))
        },
        Rev::Head => {
            let tree = repo
                .head()
                .and_then(|h| h.peel_to_tree())
                .map_err(read_error)?;
            let entry = tree.get_path(path).map_err(read_error)?;
            let object = entry.to_object(repo).map_err(read_error)?;
            let blob = object
                .as_blob()
                .ok_or_else(|| read_err(path, "not a blob at HEAD"))?;
            Ok(blob.content().to_vec())
        },
    }
}

#[allow(
    clippy::same_name_method,
    reason = "the `GitPort` blob role forwards to this method under the same name"
)]
impl Repo {
    /// Raw bytes of `path` at `rev`. Feeds the right-pane image preview.
    pub fn blob_bytes(&self, path: &Path, rev: Rev) -> GitResult<Vec<u8>> {
        blob_bytes(&self.inner, path, rev)
    }
}
