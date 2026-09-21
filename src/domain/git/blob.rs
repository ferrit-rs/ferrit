//! Raw bytes of a path, either from the working directory or from HEAD.
//!
//! The right pane's image preview (see `src/domain/image/preview.rs`) is the first consumer;
//! diffs in phase 3 will be the second. No `git2` type escapes this module.

use std::path::Path;

use git2::Repository;

use crate::domain::git::error::{GitError, GitResult};

/// Which version of a path's bytes to read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rev {
    /// The file as it currently sits in the working directory.
    Workdir,
    /// The blob recorded in HEAD's tree.
    Head,
}

fn read_err(path: &Path, msg: impl std::fmt::Display) -> GitError {
    GitError::Read(git2::Error::from_str(&format!("{}: {msg}", path.display())))
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
                .map_err(GitError::Read)?;
            let entry = tree.get_path(path).map_err(GitError::Read)?;
            let object = entry.to_object(repo).map_err(GitError::Read)?;
            let blob = object
                .as_blob()
                .ok_or_else(|| read_err(path, "not a blob at HEAD"))?;
            Ok(blob.content().to_vec())
        },
    }
}
