//! Create commits by shelling out to `git commit`, so hooks
//! (`pre-commit`/`commit-msg`/`post-commit`), GPG/SSH signing, and
//! `commit.*` config all apply the way they do for the user's own `git`.
//! See `docs/PLAN_7_COMMIT.md`.
//!
//! No `ratatui` import, same rule as the rest of `git::`.
//!
//! This file holds the types and the pure functions. The code that reads with
//! `git2` or runs `git` is `crate::infra::git::commit`.

/// What kind of commit to make.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommitKind {
    Normal,
    Amend,
    /// Amend the message only (`--amend --only`), ignoring whatever is
    /// currently staged.
    Reword,
    /// `git commit --fixup=<target>`. `target` is a full commit hash; git
    /// writes the `fixup! <subject>` message itself, so `commit`'s own
    /// `message` argument is ignored for this kind.
    Fixup {
        target: String,
    },
    /// `git commit --squash=<target>`, with a caller-supplied message.
    Squash {
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
    pub sign_off: bool,
    pub no_verify: bool,
    pub author: Option<String>,
}

/// The file the first commit holds.
pub(crate) const INITIAL_FILE: &str = "README.md";

/// The message of the first commit: always this one, and it says who made the
/// commit and the remote repository, so the history is honest about it.
pub const INITIAL_MESSAGE: &str =
    "Initial commit\n\nThis initial commit and the remote repository were created by Ferrit.\n";
