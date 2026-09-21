use crate::domain::git::error::GitError;

/// Errors surfaced by app actions. Git failures retain their typed source;
/// app-level validation and background failures keep distinct categories.
#[derive(Debug, thiserror::Error)]
pub(crate) enum AppError {
    #[error(transparent)]
    Git(#[from] GitError),
    #[error("nothing staged to commit")]
    NothingStaged,
    #[error("no commit yet to amend")]
    NoCommitToAmend,
    #[error("commit message cannot be empty")]
    EmptyCommitMessage,
    #[error("repository refresh failed: {0}")]
    Refresh(String),
    #[error("background operation failed: {0}")]
    Background(String),
    #[error("{0}")]
    Operation(String),
}

impl From<String> for AppError {
    fn from(message: String) -> Self {
        Self::Operation(message)
    }
}

impl From<&str> for AppError {
    fn from(message: &str) -> Self {
        Self::Operation(message.to_owned())
    }
}
