//! One error type for the backend. `thiserror` derives `Display` and
//! `std::error::Error` (including `source`), so `main` and `App` can `?` it
//! into a `color_eyre::Report` or turn it into a message without knowing about
//! `git2`: the adapter boxes its own error into `Open` and `Read`.

use std::path::PathBuf;

/// Everything that can go wrong talking to git, one variant per kind of operation. Most hold the
/// stderr git printed; the adapter's own errors are boxed in `Open` and `Read`.
#[derive(Debug, thiserror::Error)]
pub enum GitError {
    /// No git repository at or above the given path.
    #[error("not a git repository: {0}")]
    NotARepository(PathBuf),
    /// The repository could not be opened. The cause is the adapter's own error
    /// (`git2` for the real one), kept as the `source`.
    #[error("cannot open repository: {0}")]
    Open(#[source] Box<dyn std::error::Error + Send + Sync>),
    /// A read failed (status, refs, log, ...). Same shape as `Open`.
    #[error("git read failed: {0}")]
    Read(#[source] Box<dyn std::error::Error + Send + Sync>),
    /// `commit_diff` was handed a hash `git` does not know.
    #[error("no such commit: {0}")]
    NoSuchCommit(String),
    /// A `git diff` / `git show` subprocess exited non-zero. Holds stderr.
    #[error("git diff failed: {0}")]
    DiffFailed(String),
    /// A `git apply` / `add` / `restore` / `clean` subprocess exited
    /// non-zero while staging, unstaging or discarding. Holds stderr. `git
    /// apply` is atomic per invocation, so this always leaves the index and
    /// worktree exactly as they were. See `docs/PLAN_6_STAGING.md`.
    #[error("git apply failed: {0}")]
    ApplyFailed(String),
    /// `git commit` had nothing staged to commit. Distinct from
    /// `CommitFailed` because git's own message is stable and worth a
    /// dedicated Status-pane line rather than a raw stderr dump.
    #[error("nothing staged to commit")]
    NothingStaged,
    /// A `git commit` subprocess exited non-zero for any other reason,
    /// including a rejecting hook (`pre-commit` / `commit-msg`): git gives
    /// no stable way to tell "a hook said no" apart from any other failure
    /// in the general case, so both surface the same way. Holds stderr.
    /// See `docs/PLAN_7_COMMIT.md`.
    #[error("git commit failed: {0}")]
    CommitFailed(String),
    /// A `git checkout` subprocess exited non-zero: a dirty worktree the
    /// checkout would clobber, an invalid ref, or similar. Holds stderr.
    /// See `docs/PLAN_8_BRANCHES.md`.
    #[error("git checkout failed: {0}")]
    CheckoutFailed(String),
    /// A `git branch` / `git merge --ff-only` / local `git fetch .`
    /// subprocess exited non-zero while creating, deleting, or fast-
    /// forwarding a branch. Holds stderr; the one case ferrit acts on
    /// specially (delete refused for being unmerged) is detected from this
    /// by matching git's own stable substring, the same technique
    /// `commit.rs`'s `NothingStaged` already uses. See
    /// `docs/PLAN_8_BRANCHES.md`.
    #[error("git branch failed: {0}")]
    BranchFailed(String),
    /// `git branch -d` refused because the branch is not fully merged. The
    /// one `BranchFailed` case the UI acts on (it offers a forced delete), so
    /// it is told apart here, where git's stable wording is matched, and not
    /// by the caller. Holds stderr.
    #[error("git branch failed: {0}")]
    BranchNotMerged(String),
    /// A `git merge` subprocess exited non-zero for a reason other than an
    /// ordinary conflict (`MergeOutcome::Conflicted` is not an error, see
    /// `branch::MergeOutcome`). Holds stderr. See `docs/PLAN_8_BRANCHES.md`.
    #[error("git merge failed: {0}")]
    MergeFailed(String),
    /// A `git fetch` subprocess exited non-zero: no network, an unknown
    /// remote, an auth failure git's own credential handling could not
    /// resolve. Holds stdout+stderr, trimmed and joined. See
    /// `docs/PLAN_9_REMOTE.md`.
    #[error("git fetch failed: {0}")]
    FetchFailed(String),
    /// A `git pull` subprocess exited non-zero: the same causes as
    /// `FetchFailed`, plus a dirty worktree the merge/rebase it starts
    /// would clobber, or a conflict it leaves unresolved (see that plan's
    /// "Edge cases"). Holds stdout+stderr, trimmed and joined.
    #[error("git pull failed: {0}")]
    PullFailed(String),
    /// A `git push` subprocess exited non-zero for any reason other than
    /// `NoUpstream` (a rejected non-fast-forward, no network, auth). Holds
    /// stdout+stderr, trimmed and joined.
    #[error("git push failed: {0}")]
    PushFailed(String),
    /// The current branch has no upstream to push to. Distinct from
    /// `PushFailed` because git's message for this is stable and ferrit
    /// acts on it specifically (offers `-u <remote>`), the same shape as
    /// `NothingStaged` next to the generic `CommitFailed`.
    #[error("no upstream configured for the current branch")]
    NoUpstream,
    /// A history rewrite (`git rebase -i` with a todo ferrit generated) was
    /// refused before it started, or failed without leaving a conflict:
    /// a merge commit in the range, a dirty worktree, a `pre-rebase` hook, a
    /// commit that is not on the current branch. Holds git's own message
    /// where git said it. See `docs/PLAN_11_REBASE.md`.
    #[error("git rebase failed: {0}")]
    RebaseFailed(String),
    /// `git <operation> --continue|--skip|--abort` was refused, or asked of
    /// an operation that has no such step (a merge cannot be skipped). Holds
    /// git's own message. See `docs/PLAN_11_REBASE.md`.
    #[error("git operation failed: {0}")]
    OperationFailed(String),
    /// A `git stash` subprocess exited non-zero (apply onto a dirty
    /// worktree it would clobber, a stash entry that vanished). Holds
    /// stderr. See `docs/PLAN_10_STASH.md`.
    #[error("git stash failed: {0}")]
    StashFailed(String),
    /// `git stash push` had no local changes to save. Distinct from
    /// `StashFailed` because git exits 0 here and the message is stable.
    #[error("no local changes to save")]
    NothingToStash,
    /// A `git config` subprocess exited non-zero: a value git rejects, a
    /// locked file, an unreadable one. Holds stderr. See
    /// `docs/PLAN_14_GIT_CONFIG.md`.
    #[error("git config failed: {0}")]
    ConfigFailed(String),
    /// `gh repo create` was refused before it ran (a bad name, an `origin`
    /// that already exists) or exited non-zero, timed out or was cancelled.
    /// Holds what the user needs to read: ferrit's reason or `gh`'s own message.
    /// See `docs/PLAN_15_CREATE_REMOTE.md`.
    #[error("creating the repository failed: {0}")]
    HostFailed(String),
    /// `git init` failed: a folder that cannot be written or does not exist.
    /// Holds git's own message. See `docs/PLAN_16_START_WITHOUT_REPO.md`.
    #[error("git init failed: {0}")]
    InitFailed(String),
    /// A long read (`Repo::stats`) was stopped by its cancel flag. Not a
    /// failure: the caller asked for it and drops the result.
    #[error("cancelled")]
    Cancelled,
}

/// A result whose error is [`GitError`].
pub type GitResult<T> = Result<T, GitError>;
