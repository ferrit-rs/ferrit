//! Local branches: which one is HEAD, its upstream, ahead/behind.
//!
//! All types here are plain owned values. No `git2` type escapes this module.

use git2::{BranchType, Repository};

use crate::domain::git::error::{GitError, GitResult};
use crate::domain::git::model::BranchEntry;

/// Read the local branches, HEAD first, then alphabetical by name.
pub(super) fn branches(repo: &Repository) -> GitResult<Vec<BranchEntry>> {
    let mut out: Vec<BranchEntry> = repo
        .branches(Some(BranchType::Local))
        .map_err(GitError::Read)?
        .map(|res| {
            let (branch, _) = res.map_err(GitError::Read)?;
            let is_head = branch.is_head();
            let name = branch
                .name()
                .map_err(GitError::Read)?
                .unwrap_or("(invalid utf-8)")
                .to_owned();

            let (upstream, ahead, behind) = match branch.upstream() {
                Ok(up) => {
                    let up_name = up.name().ok().flatten().map(str::to_owned);
                    let ahead_behind = match (branch.get().target(), up.get().target()) {
                        (Some(local), Some(remote)) => {
                            repo.graph_ahead_behind(local, remote).unwrap_or((0, 0))
                        },
                        _ => (0, 0),
                    };
                    (up_name, ahead_behind.0, ahead_behind.1)
                },
                Err(_) => (None, 0, 0),
            };

            let tip_time = branch
                .get()
                .target()
                .and_then(|oid| repo.find_commit(oid).ok())
                .map_or(0, |commit| commit.time().seconds());

            Ok(BranchEntry {
                name,
                is_head,
                upstream,
                ahead,
                behind,
                tip_time,
            })
        })
        .collect::<GitResult<Vec<_>>>()?;

    out.sort_by(|a, b| match (a.is_head, b.is_head) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        // The checked-out branch first, then the most recently committed to, as
        // lazygit orders them; a tie falls back to the name.
        _ => b
            .tip_time
            .cmp(&a.tip_time)
            .then_with(|| a.name.cmp(&b.name)),
    });
    Ok(out)
}
