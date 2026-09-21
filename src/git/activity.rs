//! Commit dates used by the profile activity heatmap.

use git2::{Repository, Sort};
use std::io::Write as _;

use crate::git::error::{GitError, GitResult};

/// Read reachable HEAD commit timestamps. Stop once history is older than two years.
pub(super) fn timestamps(repo: &Repository) -> GitResult<Vec<i64>> {
    let mut walk = repo.revwalk().map_err(GitError::Read)?;
    if walk.push_head().is_err() {
        return Ok(Vec::new());
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
        result.push(time);
    }
    Ok(result)
}

/// Read push events recorded by successful pushes through Ferrit.
pub(super) fn push_timestamps(repo: &Repository) -> Vec<i64> {
    std::fs::read_to_string(repo.path().join("ferrit-pushes.log")).map_or_else(
        |_| Vec::new(),
        |contents| {
            contents
                .lines()
                .filter_map(|line| line.parse::<i64>().ok())
                .collect()
        },
    )
}

/// Append a local event after a successful Ferrit push. Never changes push result.
pub(super) fn record_push(repo: &Repository) {
    let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(repo.path().join("ferrit-pushes.log"))
    else {
        return;
    };
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0_i64, |duration| {
            i64::try_from(duration.as_secs()).unwrap_or(i64::MAX)
        });
    let _ = writeln!(file, "{timestamp}");
}

fn now_days() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| {
            i64::try_from(duration.as_secs() / 86_400).unwrap_or(i64::MAX)
        })
}
