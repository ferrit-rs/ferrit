//! Owned request/result types and Git reads for selected right-pane previews.

use std::path::PathBuf;

use crate::git::{self, DiffOpts, DiffSide};

/// Identity of selected right-pane content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RightKey {
    File { path: PathBuf },
    Commit { full_hash: String },
    BranchLog { branch: String },
}

#[derive(Debug)]
pub(crate) enum DiffQueryResult {
    File {
        unstaged: git::Diff,
        staged: git::Diff,
    },
    Commit(git::Diff),
    BranchLog(Vec<git::CommitEntry>),
}

/// Hidden event payload for a selected-diff worker completion.
#[doc(hidden)]
#[derive(Debug)]
pub struct DiffCompletion {
    pub(crate) key: RightKey,
    pub(crate) generation: u64,
    pub(crate) result: Result<DiffQueryResult, String>,
}

pub(crate) fn load(repo: &git::Repo, key: &RightKey) -> Result<DiffQueryResult, String> {
    let opts = DiffOpts::default();
    match key {
        RightKey::File { path } => Ok(DiffQueryResult::File {
            unstaged: repo
                .file_diff(path, DiffSide::Worktree, opts)
                .map_err(|error| error.to_string())?,
            staged: repo
                .file_diff(path, DiffSide::Staged, opts)
                .map_err(|error| error.to_string())?,
        }),
        RightKey::Commit { full_hash } => repo
            .commit_diff(full_hash, opts)
            .map(DiffQueryResult::Commit)
            .map_err(|error| error.to_string()),
        RightKey::BranchLog { branch } => repo
            .branch_log(branch)
            .map(DiffQueryResult::BranchLog)
            .map_err(|error| error.to_string()),
    }
}
