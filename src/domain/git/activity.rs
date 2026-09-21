//! Commit activity across local and fetched remote branches.

use git2::{Repository, Sort};

use crate::domain::git::error::{GitError, GitResult};
use crate::domain::git::model::CommitEntry;

/// Read recent commits across local and fetched remote branches, newest first.
/// This captures activity from every contributor, including unmerged branches.
pub(super) fn commits(repo: &Repository) -> GitResult<Vec<CommitEntry>> {
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
    walk.set_sorting(Sort::TIME | Sort::TOPOLOGICAL)
        .map_err(GitError::Read)?;
    let cutoff = now_days().saturating_sub(730).saturating_mul(86_400);
    let mut result = Vec::new();
    for oid in walk {
        let commit = repo
            .find_commit(oid.map_err(GitError::Read)?)
            .map_err(GitError::Read)?;
        let time = commit.time().seconds();
        if time < cutoff {
            break;
        }
        let full_hash = commit.id().to_string();
        result.push(CommitEntry {
            short_hash: full_hash.chars().take(7).collect(),
            full_hash,
            author: commit.author().name().unwrap_or("unknown").to_owned(),
            author_email: commit.author().email().unwrap_or_default().to_owned(),
            summary: commit.summary().ok().flatten().unwrap_or("").to_owned(),
            time,
        });
    }
    Ok(result)
}

fn now_days() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| {
            i64::try_from(duration.as_secs() / 86_400).unwrap_or(i64::MAX)
        })
}
