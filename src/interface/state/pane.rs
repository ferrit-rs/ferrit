//! The five left panes, in order, and the Branches pane's two tabs.

use enum_map::Enum;

/// `(discriminant, diff text)` for cheap "did the right pane actually change"
/// checks: `String` equality on a few KB, no hashing.
/// The five left panes, in top-to-bottom screen order.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug, Enum)]
pub enum Pane {
    Status,
    /// Where ferrit opens, like lazygit.
    #[default]
    Files,
    Branches,
    Commits,
    Stash,
}

/// Panes in order. Index into this is also the index into `App::selection`.
pub const PANES: [Pane; 5] = [
    Pane::Status,
    Pane::Files,
    Pane::Branches,
    Pane::Commits,
    Pane::Stash,
];

impl Pane {
    /// Position in `PANES`, for the focus-cycling arithmetic in `pane_offset`.
    /// The variant order is the `PANES` order, so the discriminant is it.
    pub fn index(self) -> usize {
        self as usize
    }

    /// Bordered-box title, lazygit style: `[N] Tab - Tab - Tab`. The extra tab
    /// names are inert labels for now; only the first is a real view.
    pub fn title(self) -> &'static str {
        match self {
            Self::Status => "[1] Status",
            Self::Files => "[2] Files - Worktrees - Submodules",
            Self::Branches => "[3] Local branches - Remotes - Tags",
            Self::Commits => "[4] Commits - Reflog",
            Self::Stash => "[5] Stash",
        }
    }

    /// Contextual title for the right pane when this left pane has focus,
    /// matching what lazygit shows there.
    pub fn right_title(self) -> &'static str {
        match self {
            Self::Status => " Status ",
            Self::Files => " Unstaged changes ",
            Self::Branches => " Log ",
            Self::Commits => " Patch ",
            Self::Stash => " Stash ",
        }
    }
}

/// Which of the Branches pane's own two real tabs is showing (the third,
/// Tags, is still an inert label — `Pane::title`). `Remotes` has no
/// selection cursor of its own; it is `Repo::remotes()` rendered plainly,
/// same as the Local tab's list was for the entirety of phase 2 before
/// phase 8 made it actionable. `docs/PLAN_9_REMOTE.md`.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub(crate) enum BranchesTab {
    #[default]
    Local,
    Remotes,
}
