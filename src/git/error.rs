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
    /// `commit_diff` was handed a hash `git` does not know.
    #[error("no such commit: {0}")]
    NoSuchCommit(String),
    /// A `git diff` / `git show` subprocess exited non-zero. Holds stderr.
    #[error("git diff failed: {0}")]
    DiffFailed(String),
    /// A `git apply` / `add` / `restore` / `clean` subprocess exited
    /// non-zero while staging, unstaging or discarding. Holds stderr. `git
    /// apply` is atomic per invocation, so this always leaves the index and
    /// worktree exactly as they were. See `docs/PLAN_6_STAGING.md`.
    #[error("git apply failed: {0}")]
    ApplyFailed(String),
    /// `git commit` had nothing staged to commit. Distinct from
    /// `CommitFailed` because git's own message is stable and worth a
    /// dedicated Status-pane line rather than a raw stderr dump.
    #[error("nothing staged to commit")]
    NothingStaged,
    /// A `git commit` subprocess exited non-zero for any other reason,
    /// including a rejecting hook (`pre-commit` / `commit-msg`): git gives
    /// no stable way to tell "a hook said no" apart from any other failure
    /// in the general case, so both surface the same way. Holds stderr.
    /// See `docs/PLAN_7_COMMIT.md`.
    #[error("git commit failed: {0}")]
    CommitFailed(String),
}

pub type GitResult<T> = Result<T, GitError>;
