//! Create commits by shelling out to `git commit`, so hooks
//! (`pre-commit`/`commit-msg`/`post-commit`), GPG/SSH signing, and
//! `commit.*` config all apply the way they do for the user's own `git`.
//! See `docs/PLAN_7_COMMIT.md`.
//!
//! No `ratatui` import, same rule as the rest of `git::`.
//!
//! This file holds the types and the pure functions. The code that reads with
//! `git2` or runs `git` is `crate::git::repo::history`.

use super::error::GitResult;
use super::port::GitPort;

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
