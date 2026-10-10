//! Binding scopes.

use crate::tui::components::panes::nav::Pane;
use strum::{EnumString, IntoStaticStr};

/// Where a binding applies. Resolution tries the specific context first,
/// then `Global`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, EnumString, IntoStaticStr)]
#[strum(serialize_all = "snake_case")]
pub enum Context {
    Global,
    Files,
    /// Cursor inside a diff.
    Diff,
    Branches,
    Commits,
    Stash,
}

impl Context {
    /// Heading for the help screen.
    pub fn title(self) -> &'static str {
        match self {
            Self::Global => "Global",
            Self::Files => "Files",
            Self::Diff => "Diff cursor",
            Self::Branches => "Branches",
            Self::Commits => "Commits",
            Self::Stash => "Stash",
        }
    }

    /// `[keys.<name>]` table name.
    pub fn name(self) -> &'static str {
        self.into()
    }

    pub fn from_name(name: &str) -> Option<Self> {
        name.parse().ok()
    }

    /// Context of a focused pane, if it has bindings of its own.
    pub fn for_pane(pane: Pane) -> Option<Self> {
        match pane {
            Pane::Status => None,
            Pane::Files => Some(Self::Files),
            Pane::Branches => Some(Self::Branches),
            Pane::Commits => Some(Self::Commits),
            Pane::Stash => Some(Self::Stash),
        }
    }
}
