//! Recent commits on HEAD, newest first, bounded to a max count.
//!
//! All types here are plain owned values. No `git2` type escapes this module.

use git2::{Repository, Sort};

use crate::git::error::{GitError, GitResult};
use crate::git::model::CommitEntry;
use crate::git::short_hash;

/// Walk HEAD's history, newest first, up to `max` entries. An unborn branch
/// (fresh repo, no commits) comes back as an empty list, not an error.
pub fn commits(repo: &Repository, max: usize) -> GitResult<Vec<CommitEntry>> {
    let mut revwalk = repo.revwalk().map_err(GitError::Read)?;
    if revwalk.push_head().is_err() {
        return Ok(Vec::new());
    }
    // TOPOLOGICAL breaks ties between commits made in the same second (which
    // TIME alone leaves in an arbitrary order) by parent-before-child.
    revwalk
        .set_sorting(Sort::TIME | Sort::TOPOLOGICAL)
        .map_err(GitError::Read)?;

    revwalk
        .take(max)
        .map(|oid| {
            let oid = oid.map_err(GitError::Read)?;
            let commit = repo.find_commit(oid).map_err(GitError::Read)?;
            Ok(CommitEntry {
                short_hash: short_hash(&oid),
                author: commit.author().name().unwrap_or("unknown").to_string(),
                summary: commit.summary().ok().flatten().unwrap_or("").to_string(),
                time: commit.time().seconds(),
            })
        })
        .collect()
}
