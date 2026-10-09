//! The generic menu popup and the `m` menu for an operation stopped
//! mid-way (continue, skip, abort). See `docs/PLAN_11_REBASE.md` R2.
//!
//! `Popup::Menu` is deliberately not specific to operations: phase 12's `x`
//! menu reuses it with more `MenuAction`s.

use crate::git;

/// What choosing a menu row does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MenuAction {
    Continue,
    Skip,
    Abort,
    // The `x` menu (`docs/PLAN_12_POLISH.md` P4).
    RenameBranch,
    MergeNoFf,
    // The `M` menu (`docs/PLAN_8_BRANCHES.md`).
    MergeFf,
    SquashStaged,
    SquashCommit,
    BranchFromCommit,
    StashKeepIndex,
    RenameStash,
    TakeOurs,
    TakeTheirs,
    // The git config screen's allowed-values menu: the index of the row.
    ConfigValue(usize),
    // The `x` menu of a repository with no remote (`app::create_remote`).
    CreateRemote,
}

/// One row: what it says, the key that runs it from anywhere in the menu, and
/// what it does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MenuItem {
    pub(crate) label: &'static str,
    pub(crate) shortcut: char,
    pub(crate) action: MenuAction,
    /// One line under the menu while the row is highlighted; empty for none.
    pub(crate) hint: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MenuState {
    pub(crate) title: String,
    pub(crate) items: Vec<MenuItem>,
    pub(crate) selected: usize,
}

/// The rows of the operation menu. A merge has no skip: git has no
/// `merge --skip`.
pub(crate) fn operation_items(operation: git::model::Operation) -> Vec<MenuItem> {
    let mut items = vec![MenuItem {
        label: "Continue",
        shortcut: 'c',
        action: MenuAction::Continue,
        hint: "",
    }];
    if operation != git::model::Operation::Merge {
        items.push(MenuItem {
            label: "Skip this step",
            shortcut: 's',
            action: MenuAction::Skip,
            hint: "",
        });
    }
    items.push(MenuItem {
        label: "Abort",
        shortcut: 'a',
        action: MenuAction::Abort,
        hint: "",
    });
    items
}

impl MenuState {
    /// The `M` menu: the ways to merge the selected branch into the current one.
    pub(crate) fn merge() -> Self {
        let item = |label, shortcut, action, hint| MenuItem {
            label,
            shortcut,
            action,
            hint,
        };
        Self {
            title: "Merge".to_owned(),
            items: vec![
                item(
                    "Merge (fast-forward when possible)",
                    'm',
                    MenuAction::MergeFf,
                    "Fast-forward when history allows, else a merge commit.",
                ),
                item(
                    "Merge with --no-ff",
                    'n',
                    MenuAction::MergeNoFf,
                    "Always create a merge commit.",
                ),
                item(
                    "Squash, leave changes staged",
                    's',
                    MenuAction::SquashStaged,
                    "Stage the branch's changes without committing.",
                ),
                item(
                    "Squash and commit",
                    'c',
                    MenuAction::SquashCommit,
                    "Squash the branch's changes into one new commit.",
                ),
            ],
            selected: 0,
        }
    }
}
