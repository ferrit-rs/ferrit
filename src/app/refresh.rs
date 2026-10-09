//! The result of a repository refresh, as it comes back from its worker.

use super::authorship::Authorship;
use super::error::AppError;
use super::tree::commit_drill_files;
use crate::git;
use crate::git::diff::DiffOpts;
use crate::git::error::GitError;
use crate::git::port::GitPort;
use crate::git::profile::Profile;
use color_eyre::Result;
use std::sync::Arc;

/// Snapshot plus any active drill-down data loaded in the same worker.
/// A result whose error is shared between the Status line, the toast and the
/// refresh bookkeeping, none of which can own it alone.
pub(super) type Shared<T> = Result<T, Arc<AppError>>;

#[doc(hidden)]
#[derive(Debug)]
pub struct RefreshCompletion {
    pub(crate) snapshot: Shared<git::Snapshot>,
    pub(crate) profile: Option<Profile>,
    pub(crate) branch_log: Option<(String, Shared<Vec<git::model::CommitEntry>>)>,
    pub(crate) commit_files: Option<(String, Shared<Vec<git::model::FileEntry>>)>,
}

impl RefreshCompletion {
    /// Read the snapshot, and the drilled branch log and commit files when the
    /// user is inside them, in one go (the worker's whole job).
    pub(super) fn load(
        repo: &mut dyn GitPort,
        branch: Option<String>,
        commit: Option<String>,
        opts: DiffOpts,
    ) -> Self {
        let snapshot = repo.snapshot().map_err(|error| Arc::new(error.into()));
        let branch_log = branch.map(|name| {
            let result = repo
                .branch_log(&name)
                .map_err(|error| Arc::new(error.into()));
            (name, result)
        });
        let commit_files = commit.map(|hash| {
            let result = repo
                .commit_diff(&hash, opts)
                .map(|diff| commit_drill_files(&diff))
                .map_err(|error| Arc::new(error.into()));
            (hash, result)
        });
        Self {
            snapshot,
            profile: Some(Authorship::profile_of(repo)),
            branch_log,
            commit_files,
        }
    }

    /// The repository could not be opened: every part of the refresh fails
    /// with that error.
    pub(super) fn failed(error: GitError, branch: Option<String>, commit: Option<String>) -> Self {
        let error = Arc::new(AppError::from(error));
        Self {
            snapshot: Err(Arc::clone(&error)),
            profile: None,
            branch_log: branch.map(|name| (name, Err(Arc::clone(&error)))),
            commit_files: commit.map(|hash| (hash, Err(error))),
        }
    }
}
