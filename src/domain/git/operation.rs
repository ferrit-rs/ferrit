//! Which multi-step operation, if any, git is stopped in the middle of.
//!
//! Read-only: `git2::Repository::state` plus, for a rebase, the progress
//! files git writes itself. See `docs/PLAN_11_REBASE.md`.
//!
//! This file holds the types and the pure functions. The code that reads with
//! `git2` or runs `git` is `crate::infra::git::operation`.

/// What the user asks of a stopped operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Continue,
    /// Not available for a merge: git has no `merge --skip`.
    Skip,
    Abort,
}

/// Where the repository is after a step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationOutcome {
    /// No operation in progress any more.
    Done,
    /// Git stopped again and waits for the user: on a conflict, or, for a
    /// rebase, at an `edit` step.
    Stopped { conflicted: bool },
}

pub(crate) fn flag(step: Step) -> &'static str {
    match step {
        Step::Continue => "--continue",
        Step::Skip => "--skip",
        Step::Abort => "--abort",
    }
}
