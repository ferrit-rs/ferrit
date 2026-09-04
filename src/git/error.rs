//! One error type for the backend. Converts into `color_eyre::Report` at the
//! edge (it implements `std::error::Error`), so `main` and `App` can use `?`
//! or turn it into a message without knowing about `git2`.

use std::fmt;
use std::path::PathBuf;

#[derive(Debug)]
pub enum GitError {
    /// No git repository at or above the given path.
    NotARepository(PathBuf),
    /// `git2` failed while opening the repository.
    Open(git2::Error),
    /// `git2` failed while reading (status, refs, log, ...).
    Read(git2::Error),
}

impl fmt::Display for GitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GitError::NotARepository(path) => {
                write!(f, "not a git repository: {}", path.display())
            }
            GitError::Open(e) => write!(f, "cannot open repository: {}", e.message()),
            GitError::Read(e) => write!(f, "git read failed: {}", e.message()),
        }
    }
}

impl std::error::Error for GitError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            GitError::NotARepository(_) => None,
            GitError::Open(e) | GitError::Read(e) => Some(e),
        }
    }
}

pub type GitResult<T> = Result<T, GitError>;
