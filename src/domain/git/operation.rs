//! Which multi-step operation, if any, git is stopped in the middle of.
//!
//! Read-only: `git2::Repository::state` plus, for a rebase, the progress
//! files git writes itself. See `docs/PLAN_11_REBASE.md`.

use std::fs;

use git2::{Repository, RepositoryState};

use crate::domain::git::model::Operation;

/// The operation in progress, or `None` for a clean repository. Bisect and
/// mailbox (`git am`) states map to `None`: ferrit has no flow for them, and
/// `ApplyMailboxOrRebase` cannot be told apart from a plain `am`.
pub(super) fn current(repo: &Repository) -> Option<Operation> {
    match repo.state() {
        RepositoryState::Clean
        | RepositoryState::Bisect
        | RepositoryState::ApplyMailbox
        | RepositoryState::ApplyMailboxOrRebase => None,
        RepositoryState::Merge => Some(Operation::Merge),
        RepositoryState::Revert | RepositoryState::RevertSequence => Some(Operation::Revert),
        RepositoryState::CherryPick | RepositoryState::CherryPickSequence => {
            Some(Operation::CherryPick)
        },
        // The `--apply` backend.
        RepositoryState::Rebase => Some(rebase_progress(repo, "rebase-apply", "next", "last")),
        // The default (merge) backend, interactive or not.
        RepositoryState::RebaseInteractive | RepositoryState::RebaseMerge => {
            Some(rebase_progress(repo, "rebase-merge", "msgnum", "end"))
        },
    }
}

/// `step` and `total` from the files git keeps in `<git dir>/<dir>/`.
fn rebase_progress(repo: &Repository, dir: &str, step_file: &str, total_file: &str) -> Operation {
    let read = |name: &str| {
        fs::read_to_string(repo.path().join(dir).join(name))
            .ok()
            .and_then(|text| text.trim().parse().ok())
            .unwrap_or(0)
    };
    Operation::Rebase {
        step: read(step_file),
        total: read(total_file),
    }
}
