//! The `x` menu (and right-click): extra actions for the selected row that do
//! not earn a key of their own. Built on the generic `Popup::Menu` of
//! `app::menu`. See `docs/PLAN_12_POLISH.md` P4.
//!
//! | Pane | Entries |
//! | --- | --- |
//! | Branches | rename the branch, merge it with `--no-ff` (`M` opens the full Merge menu) |
//! | Commits | new branch from this commit |
//! | Stash | stash keeping the index, rename the entry |
//! | Files (a conflicted file) | take ours, take theirs |

use super::menu::{MenuAction, MenuItem};
use crate::git;

/// What a name popup will do with the text typed into it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NameKind {
    RenameBranch {
        from: String,
    },
    BranchAt {
        hash: String,
    },
    RenameStash {
        oid: String,
    },
    StashKeepIndex,
    /// A git config value being typed (`app::git_config_edit`).
    ConfigValue(git::config_edit::ConfigOp),
    /// The key of a config entry about to be added.
    ConfigKey,
}

/// A name popup's purpose and its title (which may name a commit).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NameTarget {
    pub(crate) kind: NameKind,
    pub(crate) title: String,
}

impl NameTarget {
    /// The footer hint for this popup.
    pub(crate) fn hints(&self) -> &'static str {
        match self.kind {
            NameKind::RenameBranch { .. } | NameKind::RenameStash { .. } => {
                "Rename: Enter | Cancel: Esc"
            },
            NameKind::BranchAt { .. } => "Create: Enter | Cancel: Esc",
            NameKind::StashKeepIndex => "Stash: Enter | Cancel: Esc",
            NameKind::ConfigKey => "Next: Enter | Cancel: Esc",
            NameKind::ConfigValue(_) => "Save: Enter | Cancel: Esc",
        }
    }
}

pub(crate) fn item(label: &'static str, shortcut: char, action: MenuAction) -> MenuItem {
    MenuItem {
        label,
        shortcut,
        action,
        hint: "",
    }
}
