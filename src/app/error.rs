use std::path::PathBuf;
use std::sync::Arc;

use crate::app::WorkerError;
use crate::app::config::error::ConfigError;
use crate::app::image_query::ImageError;
use crate::domain::git::error::GitError;

/// Errors surfaced by app actions. Every variant says what went wrong; there is
/// no catch-all that takes a `String`, so a failure keeps its type from where it
/// happens to the one place that renders it (the Status pane and the toast).
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error(transparent)]
    Git(#[from] GitError),
    #[error(transparent)]
    Worker(#[from] WorkerError),
    #[error(transparent)]
    Image(#[from] ImageError),
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error("no commit yet to amend")]
    NoCommitToAmend,
    #[error("commit message cannot be empty")]
    EmptyCommitMessage,
    #[error("no repository: git config needs one")]
    NoRepository,
    #[error("upstream must be `<remote> <branch>`")]
    BadUpstream,
    /// A staging refused because `path` still holds conflict markers.
    #[error("{} still has conflict markers, resolve them before staging", .0.display())]
    ConflictMarkers(PathBuf),
    /// A bulk staging that left out the files still holding conflict markers.
    #[error("not staged, still has conflict markers: {}", join_paths(.0))]
    PartlyStaged(Vec<PathBuf>),
    /// The configuration file had problems; `location` is already formatted
    /// (` /path/to/config.toml` or empty).
    #[error("config{location}: {}", .issues.join("; "))]
    ConfigIssues {
        location: String,
        issues: Vec<String>,
    },
    /// The repository refresh failed in its worker.
    #[error("repository refresh failed: {0}")]
    Refresh(#[source] Arc<Self>),
    /// A fetch, pull, push or creation failed in the background.
    #[error("background operation failed: {0}")]
    Background(#[source] Arc<Self>),
    /// The push that follows a repository creation failed; the repository
    /// exists, so say how to retry.
    #[error("{source}\nThe repository {url} exists and origin is set: P retries the push.")]
    PushAfterCreation {
        #[source]
        source: Arc<Self>,
        url: String,
    },
    /// The filesystem watcher failed to start; ferrit polls instead.
    #[error("filesystem watcher unavailable; polling fallback active: {detail}")]
    WatcherUnavailable { detail: String },
    /// Something worth telling the user that is not a failure: a hint, a
    /// refused input, a terminal quirk.
    #[error("{0}")]
    Notice(String),
}

fn join_paths(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}
