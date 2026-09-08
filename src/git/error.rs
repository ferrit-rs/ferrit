//! One error type for the backend. `thiserror` derives `Display` and
//! `std::error::Error` (including `source`), so `main` and `App` can `?` it
//! into a `color_eyre::Report` or turn it into a message without knowing about
//! `git2`.

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum GitError {
    /// No git repository at or above the given path.
    #[error("not a git repository: {0}")]
    NotARepository(PathBuf),
    /// `git2` failed while opening the repository.
    #[error("cannot open repository: {}", .0.message())]
    Open(#[source] git2::Error),
    /// `git2` failed while reading (status, refs, log, ...).
    #[error("git read failed: {}", .0.message())]
    Read(#[source] git2::Error),
}

pub type GitResult<T> = Result<T, GitError>;
