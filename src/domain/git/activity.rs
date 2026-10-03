//! The walk over every local and fetched remote branch, for the dashboard's statistics.

use git2::{Repository, Revwalk};

use crate::domain::git::error::{GitError, GitResult};

/// A revwalk primed with the tip of every local and fetched remote branch
/// (`refs/heads`, `refs/remotes`, remote `HEAD` aliases left out). The caller sets the sorting.
pub(super) fn branch_walk(repo: &Repository) -> GitResult<Revwalk<'_>> {
    let mut walk = repo.revwalk().map_err(GitError::Read)?;
    let references = repo.references().map_err(GitError::Read)?;
    for reference in references {
        let reference = reference.map_err(GitError::Read)?;
        let Ok(name) = reference.name() else {
            continue;
        };
        if !(name.starts_with("refs/heads/") || name.starts_with("refs/remotes/"))
            || name.ends_with("/HEAD")
        {
            continue;
        }
        if let Ok(commit) = reference.peel_to_commit() {
            walk.push(commit.id()).map_err(GitError::Read)?;
        }
    }
    Ok(walk)
}
