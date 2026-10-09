//! The generic menu popup and the `m` menu for an operation stopped
//! mid-way (continue, skip, abort). See `docs/PLAN_11_REBASE.md` R2.
//!
//! `Popup::Menu` is deliberately not specific to operations: phase 12's `x`
//! menu reuses it with more `MenuAction`s.

use super::{App, KeyCode, KeyEvent, git, operation_noun};
use crate::git::error::GitResult;
use crate::git::operation::{OperationOutcome, Step};
use crate::interface::popups::confirm::{ConfirmAction, ConfirmPrompt};
use crate::interface::popups::popup::Popup;

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
fn operation_items(operation: git::model::Operation) -> Vec<MenuItem> {
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

impl App {
    /// `m`: the menu for the merge, rebase, cherry-pick or revert git is
    /// stopped in. Inert when there is none.
    pub(crate) fn open_operation_menu(&mut self) {
        let Some(operation) = self.snapshot.operation else {
            return;
        };
        if self.modal.is_some() {
            return;
        }
        self.modal.open_popup(Popup::Menu(MenuState {
            title: operation.label(),
            items: operation_items(operation),
            selected: 0,
        }));
    }

    /// Every key while a menu is up: `j` / `k` move, `Enter` or a row's own
    /// letter runs it, `Esc` closes.
    pub(crate) fn menu_key(&mut self, key: KeyEvent) {
        let Some(Popup::Menu(menu)) = self.modal.popup_mut() else {
            return;
        };
        let last = menu.items.len().saturating_sub(1);
        let chosen = match key.code {
            KeyCode::Esc => {
                self.modal.close_popup();
                return;
            },
            KeyCode::Char('j') | KeyCode::Down => {
                menu.selected = (menu.selected + 1).min(last);
                None
            },
            KeyCode::Char('k') | KeyCode::Up => {
                menu.selected = menu.selected.saturating_sub(1);
                None
            },
            KeyCode::Enter => menu.items.get(menu.selected).map(|item| item.action),
            KeyCode::Char(letter) => menu
                .items
                .iter()
                .find(|item| item.shortcut == letter)
                .map(|item| item.action),
            _ => None,
        };
        if let Some(action) = chosen {
            self.modal.close_popup();
            self.run_menu_action(action);
        }
    }

    fn run_menu_action(&mut self, action: MenuAction) {
        match action {
            MenuAction::ConfigValue(index) => self.pick_config_value(index),
            MenuAction::CreateRemote => self.open_create_remote(),
            MenuAction::Continue => self.apply_operation_step(Step::Continue),
            MenuAction::Skip => self.apply_operation_step(Step::Skip),
            MenuAction::RenameBranch
            | MenuAction::MergeNoFf
            | MenuAction::MergeFf
            | MenuAction::SquashStaged
            | MenuAction::SquashCommit
            | MenuAction::BranchFromCommit
            | MenuAction::StashKeepIndex
            | MenuAction::RenameStash
            | MenuAction::TakeOurs
            | MenuAction::TakeTheirs => self.run_context_action(action),
            // Throws away the resolution work so far: ask first.
            MenuAction::Abort => {
                let noun = self.snapshot.operation.map_or("operation", operation_noun);
                self.modal.ask(ConfirmPrompt {
                    message: format!("abort the {noun}? Work done in it so far is lost."),
                    action: ConfirmAction::AbortOperation,
                });
            },
        }
    }

    /// Run one step and say where git stopped.
    pub(crate) fn apply_operation_step(&mut self, step: Step) {
        let Some(repo) = &self.repo else { return };
        let result = repo.operation_step(step);
        self.finish_operation(result);
    }

    /// Refresh and report where git stopped after a step or a rewrite.
    /// Refreshes either way: a refusal changes nothing, a step changes a lot.
    pub(crate) fn finish_operation(&mut self, result: GitResult<OperationOutcome>) {
        self.request_refresh();
        match result {
            Ok(OperationOutcome::Done) => {},
            Ok(OperationOutcome::Stopped { conflicted: true }) => {
                self.modal.open_popup(Popup::Note(
                    "stopped on a conflict. Resolve it in Files, then press m and Continue."
                        .to_owned(),
                ));
            },
            Ok(OperationOutcome::Stopped { conflicted: false }) => {
                self.modal.open_popup(Popup::Note(
                    "stopped for you to edit. Make your change, then press m and Continue."
                        .to_owned(),
                ));
            },
            Err(e) => self.report_error(e),
        }
    }
}
