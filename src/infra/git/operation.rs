//! The `git2` and subprocess half of `crate::domain::git::operation`: the types are there.

use std::fs;

use git2::{Repository, RepositoryState};

use super::diff::workdir;
use crate::domain::git::error::{GitError, GitResult};
use crate::domain::git::exec;
use crate::domain::git::model::Operation;

use crate::domain::git::operation::{OperationOutcome, Step, flag};

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

/// Run `git <operation> --continue|--skip|--abort` for whatever operation is
/// in progress, with the editor neutralised (`GIT_EDITOR=true`: ferrit owns the
/// terminal, an editor would hang).
///
/// The outcome comes from the repository afterwards, not the exit code: a
/// `--continue` that reaches the next conflicting commit exits non-zero and
/// is `Stopped`, while git refusing to continue over an unresolved file also
/// exits non-zero and is an error. The two are told apart by git's
/// `CONFLICT (` report, which only a fresh conflict prints.
pub(super) fn step(repo: &Repository, step: Step) -> GitResult<OperationOutcome> {
    let Some(operation) = current(repo) else {
        return Err(GitError::OperationFailed(
            "no operation in progress".to_owned(),
        ));
    };
    let (command, flag) = match (operation, step) {
        (Operation::Merge, Step::Skip) => {
            return Err(GitError::OperationFailed(
                "a merge cannot be skipped".to_owned(),
            ));
        },
        (Operation::Merge, _) => ("merge", flag(step)),
        (Operation::Rebase { .. }, _) => ("rebase", flag(step)),
        (Operation::CherryPick, _) => ("cherry-pick", flag(step)),
        (Operation::Revert, _) => ("revert", flag(step)),
    };
    let mut cmd = exec::git(workdir(repo)?);
    cmd.env("GIT_EDITOR", "true").arg(command).arg(flag);
    let out = exec::output(&mut cmd)
        .map_err(|e| GitError::OperationFailed(format!("cannot run git: {e}")))?;

    settle(repo, &out, GitError::OperationFailed)
}

/// Where the repository stands after a subprocess that may have started or
/// advanced an operation, or the refusal text. Shared by `step` and by
/// `rebase`, so both read the same signals.
///
/// A failure is `Stopped` only when git reported a fresh `CONFLICT (` and an
/// operation is in progress. Any other failure is the refusal text, even if an
/// operation is still in progress (a rejecting hook, a `--continue` over an
/// unresolved file): the caller shows it, and the Status badge and `m` menu
/// are how the user leaves that state. `refuse` wraps that text in the error
/// variant of the operation that ran.
pub(super) fn settle(
    repo: &Repository,
    out: &std::process::Output,
    refuse: fn(String) -> GitError,
) -> GitResult<OperationOutcome> {
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let conflicted = repo.index().is_ok_and(|mut index| {
        // The subprocess rewrote the index; drop the cached copy.
        index.read(true).is_ok() && index.has_conflicts()
    });
    if out.status.success() {
        return Ok(match current(repo) {
            None => OperationOutcome::Done,
            Some(_) => OperationOutcome::Stopped { conflicted },
        });
    }
    if text.contains("CONFLICT (") && current(repo).is_some() {
        return Ok(OperationOutcome::Stopped { conflicted: true });
    }
    Err(refuse(text.trim().to_owned()))
}
