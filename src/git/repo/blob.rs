//! The `git2` and subprocess half of `crate::git::blob`: the types are there.

use crate::git::repo::read_error;
use std::path::Path;

use git2::Repository;

use crate::git::error::{GitError, GitResult};

use crate::git::blob::Rev;

fn read_err(path: &Path, msg: impl std::fmt::Display) -> GitError {
    read_error(git2::Error::from_str(&format!("{}: {msg}", path.display())))
}

/// Read `path` at `rev`. Missing files, bare repos and non-blob entries all
/// come back as `GitError::Read`, never a panic.
pub(super) fn blob_bytes(repo: &Repository, path: &Path, rev: Rev) -> GitResult<Vec<u8>> {
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
