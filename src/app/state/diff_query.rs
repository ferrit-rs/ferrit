//! Owned request/result types and Git reads for selected right-pane previews.

use std::path::PathBuf;

use crate::app::error::AppError;

use crate::git;
use crate::git::diff::{DiffOpts, DiffSide};
use crate::git::error::GitError;
use crate::git::port::GitPort;

#[derive(Default)]
pub(crate) struct DiffQueryState {
    pub(crate) in_flight: bool,
    pub(crate) pending: Option<(RightKey, u64)>,
    pub(crate) generation: u64,
    pub(crate) refresh_requested: bool,
}

/// Identity of selected right-pane content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RightKey {
    File { path: PathBuf },
    Commit { full_hash: String },
    BranchLog { branch: String },
    Stash { oid: String, header: String },
}

#[derive(Debug)]
pub(crate) enum DiffQueryResult {
    File {
        unstaged: git::diff::Diff,
        staged: git::diff::Diff,
    },
    Commit(git::diff::Diff),
    BranchLog(Vec<git::model::CommitEntry>),
    Stash(git::diff::Diff),
}

/// Hidden event payload for a selected-diff worker completion.
#[doc(hidden)]
#[derive(Debug)]
pub struct DiffCompletion {
    pub(crate) key: RightKey,
    pub(crate) generation: u64,
    pub(crate) result: Result<DiffQueryResult, AppError>,
}

pub(crate) fn load(
    repo: &dyn GitPort,
    key: &RightKey,
    opts: DiffOpts,
) -> Result<DiffQueryResult, GitError> {
    match key {
        RightKey::File { path } => {
            // The root directory row has an empty path: `git diff -- .` is every file.
            let path = if path.as_os_str().is_empty() {
                std::path::Path::new(".")
            } else {
                path.as_path()
            };
            Ok(DiffQueryResult::File {
                unstaged: repo.file_diff(path, DiffSide::Worktree, opts)?,
                staged: repo.file_diff(path, DiffSide::Staged, opts)?,
            })
        },
        RightKey::Commit { full_hash } => repo
            .commit_diff(full_hash, opts)
            .map(DiffQueryResult::Commit),
        RightKey::BranchLog { branch } => repo.branch_log(branch).map(DiffQueryResult::BranchLog),
        RightKey::Stash { oid, header } => repo
            .stash_diff(oid, header, opts)
            .map(DiffQueryResult::Stash),
    }
}
