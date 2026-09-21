//! Stash entries, stack order (`stash@{0}` = most recent).
//!
//! All types here are plain owned values. No `git2` type escapes this module.

use git2::Repository;

use crate::domain::git::error::{GitError, GitResult};
use crate::domain::repository::StashEntry;

/// Read the stash list. `git2::Repository::stash_foreach` needs `&mut`, so
/// this is the one read in `Repo::snapshot()` that borrows mutably.
pub(super) fn stashes(repo: &mut Repository) -> GitResult<Vec<StashEntry>> {
    let mut out = Vec::new();
    repo.stash_foreach(|index, message, oid| {
        out.push(StashEntry {
            index,
            oid: oid.to_string(),
            message: message.to_owned(),
        });
        true
    })
    .map_err(GitError::Read)?;
    Ok(out)
}
