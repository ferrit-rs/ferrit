//! Recent commits on HEAD or on an arbitrary branch tip, newest first,
//! bounded to a max count.
//!
//! All types here are plain owned values. No `git2` type escapes this module.

use git2::{BranchType, Repository, Revwalk, Sort};

use crate::git::error::{GitError, GitResult};
use crate::git::model::CommitEntry;

/// Walk HEAD's history, newest first, up to `max` entries. An unborn branch
/// (fresh repo, no commits) comes back as an empty list, not an error.
pub(super) fn commits(repo: &Repository, max: usize) -> GitResult<Vec<CommitEntry>> {
    let mut revwalk = repo.revwalk().map_err(GitError::Read)?;
    if revwalk.push_head().is_err() {
        return Ok(Vec::new());
    }
    walk(repo, revwalk, max)
}

/// Walk one local branch's history, newest first, up to `max` entries. A
/// branch that no longer exists, or one with no commits, comes back as an
/// empty list, not an error — the caller (`App`) treats that as "drop the
/// scope" rather than surfacing it.
pub(super) fn commits_for(
    repo: &Repository,
    branch: &str,
    max: usize,
) -> GitResult<Vec<CommitEntry>> {
    let Ok(branch_ref) = repo.find_branch(branch, BranchType::Local) else {
        return Ok(Vec::new());
    };
    let Some(oid) = branch_ref.get().target() else {
        return Ok(Vec::new());
    };
    let mut revwalk = repo.revwalk().map_err(GitError::Read)?;
    revwalk.push(oid).map_err(GitError::Read)?;
    walk(repo, revwalk, max)
}

/// Shared revwalk drain: TOPOLOGICAL sorting breaks ties between commits made
/// in the same second (which TIME alone leaves in an arbitrary order) by
/// parent-before-child.
fn walk(repo: &Repository, mut revwalk: Revwalk<'_>, max: usize) -> GitResult<Vec<CommitEntry>> {
    revwalk
        .set_sorting(Sort::TIME | Sort::TOPOLOGICAL)
        .map_err(GitError::Read)?;

    revwalk
        .take(max)
        .map(|oid| {
            let oid = oid.map_err(GitError::Read)?;
            let commit = repo.find_commit(oid).map_err(GitError::Read)?;
            let full_hash = oid.to_string();
            Ok(CommitEntry {
                short_hash: full_hash.chars().take(7).collect(),
                full_hash,
                author: commit.author().name().unwrap_or("unknown").to_owned(),
                summary: commit.summary().ok().flatten().unwrap_or("").to_owned(),
                time: commit.time().seconds(),
            })
        })
        .collect()
}
