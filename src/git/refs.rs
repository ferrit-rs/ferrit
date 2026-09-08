//! Local branches: which one is HEAD, its upstream, ahead/behind.
//!
//! All types here are plain owned values. No `git2` type escapes this module.

use git2::{BranchType, Repository};

use crate::git::error::{GitError, GitResult};
use crate::git::model::BranchEntry;

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

            Ok(BranchEntry {
                name,
                is_head,
                upstream,
                ahead,
                behind,
            })
        })
        .collect::<GitResult<Vec<_>>>()?;

    out.sort_by(|a, b| match (a.is_head, b.is_head) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.cmp(&b.name),
    });
    Ok(out)
}
