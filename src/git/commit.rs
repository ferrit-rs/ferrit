//! Create commits by shelling out to `git commit`, so hooks
//! (`pre-commit`/`commit-msg`/`post-commit`), GPG/SSH signing, and
//! `commit.*` config all apply the way they do for the user's own `git`.
//! See `docs/PLAN_7_COMMIT.md`.
//!
//! No `ratatui` import, same rule as the rest of `git::`.
//!
//! This file holds the types and the pure functions. The code that reads with
//! `git2` or runs `git` is `crate::git::repo::commit`.

use crate::git::error::GitResult;
use crate::git::model::{Change, FileEntry};
use crate::git::port::GitPort;
use crate::git::rebase::OperationOutcome;
use crate::git::rebase::RebaseEdit;

/// What kind of commit to make.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommitKind {
    /// A new commit with the message given.
    Normal,
    /// Replace `HEAD`: its message and whatever is staged now.
    Amend,
    /// Amend the message only (`--amend --only`), ignoring whatever is
    /// currently staged.
    Reword,
    /// `git commit --fixup=<target>`. `target` is a full commit hash; git
    /// writes the `fixup! <subject>` message itself, so `commit`'s own
    /// `message` argument is ignored for this kind.
    Fixup {
        /// The full hash of the commit to fix up.
        target: String,
    },
    /// `git commit --squash=<target>`, with a caller-supplied message.
    Squash {
        /// The full hash of the commit to squash into.
        target: String,
    },
}

impl CommitKind {
    /// Popup title, `docs/PLAN_7_COMMIT.md`'s "Amend HEAD" / "Reword HEAD".
    pub fn title(&self) -> &'static str {
        match self {
            Self::Normal => "Commit",
            Self::Amend => "Amend HEAD",
            Self::Reword => "Reword HEAD",
            Self::Fixup { .. } => "Fixup",
            Self::Squash { .. } => "Squash",
        }
    }
}

/// Per-commit toggles, both visible in the popup footer
/// (`docs/PLAN_7_COMMIT.md` "Sign-off default": never a silent `-s`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CommitOpts {
    /// Add a `Signed-off-by` trailer (`-s`).
    pub sign_off: bool,
    /// Skip the `pre-commit` and `commit-msg` hooks (`--no-verify`).
    pub no_verify: bool,
    /// `Name <email>` for `--author`; `None` lets git use its own identity.
    pub author: Option<String>,
}

/// The file the first commit holds.
pub(crate) const INITIAL_FILE: &str = "README.md";

/// The message of the first commit: always this one, and it says who made the
/// commit and the remote repository, so the history is honest about it.
pub const INITIAL_MESSAGE: &str =
    "Initial commit\n\nThis initial commit and the remote repository were created by Ferrit.\n";

/// `git commit --fixup=<target>` with what is staged, for a later autosquash.
/// Nothing staged is an error, not an empty commit.
pub fn fixup(repo: &dyn GitPort, target: &str, author: Option<String>) -> GitResult<String> {
    let opts = CommitOpts {
        sign_off: false,
        no_verify: false,
        author,
    };
    let kind = CommitKind::Fixup {
        target: target.to_owned(),
    };
    repo.commit(&kind, "", opts)
}

impl CommitKind {
    /// Does this kind need a message of its own? A fixup does not: git writes
    /// the `fixup! <subject>` line itself.
    #[must_use]
    pub const fn needs_summary(&self) -> bool {
        !matches!(self, Self::Fixup { .. })
    }
}

/// What `c`, `A` or `w` comes to before an editor opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenPlan {
    /// A new commit, with nothing staged: ask whether to stage everything first.
    StageAllFirst,
    /// Amend or reword, with no commit to amend.
    NoCommit,
    /// Open the editor.
    Edit,
}

/// Decide what opening the editor for `kind` comes to.
#[must_use]
pub fn plan_open(kind: &CommitKind, files: &[FileEntry], commit_count: usize) -> OpenPlan {
    match kind {
        CommitKind::Normal if !files.iter().any(|file| file.staged != Change::None) => {
            OpenPlan::StageAllFirst
        },
        CommitKind::Amend | CommitKind::Reword if commit_count == 0 => OpenPlan::NoCommit,
        _ => OpenPlan::Edit,
    }
}

/// The text the editor starts from. Amend and reword start from `HEAD`'s
/// message. A new commit starts from the draft a cancelled editor kept
/// (`saved`, taken), else from the `commit.template` file, else from nothing.
/// Any other kind takes the saved draft.
pub fn prefill(
    kind: &CommitKind,
    repo: &dyn GitPort,
    saved: &mut Option<String>,
) -> Option<String> {
    match kind {
        CommitKind::Amend | CommitKind::Reword => repo.head_message().ok().flatten(),
        CommitKind::Normal => saved.take().or_else(|| repo.commit_template()),
        _ => saved.take(),
    }
}

/// What submitting the editor did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Submitted {
    /// A commit was made; this is its hash.
    Committed(String),
    /// An older commit was reworded with a rebase; where git stopped.
    Reworded(OperationOutcome),
}

/// Submit the editor: a rebase reword of the commit `reword` when there is one,
/// else `git commit` of `kind`.
pub fn submit(
    repo: &dyn GitPort,
    kind: &CommitKind,
    message: String,
    opts: CommitOpts,
    reword: Option<&str>,
) -> GitResult<Submitted> {
    match reword {
        Some(hash) => repo
            .rebase_edit(hash, &RebaseEdit::Reword(message))
            .map(Submitted::Reworded),
        None => repo.commit(kind, &message, opts).map(Submitted::Committed),
    }
}
