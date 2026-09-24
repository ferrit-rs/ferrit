//! The generic menu popup and the `m` menu for an operation stopped
//! mid-way (continue, skip, abort). See `docs/PLAN_11_REBASE.md` R2.
//!
//! `Popup::Menu` is deliberately not specific to operations: phase 12's `x`
//! menu reuses it with more `MenuAction`s.

use super::{
    App, ConfirmAction, ConfirmPrompt, GitResult, KeyCode, KeyEvent, Popup, git, operation_noun,
};
use crate::domain::git::operation::{OperationOutcome, Step};

/// What choosing a menu row does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MenuAction {
    Continue,
    Skip,
    Abort,
}

/// One row: what it says, the key that runs it from anywhere in the menu, and
/// what it does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct MenuItem {
    pub(super) label: &'static str,
    pub(super) shortcut: char,
    pub(super) action: MenuAction,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct MenuState {
    pub(super) title: String,
    pub(super) items: Vec<MenuItem>,
    pub(super) selected: usize,
}

/// The rows of the operation menu. A merge has no skip: git has no
/// `merge --skip`.
fn operation_items(operation: git::model::Operation) -> Vec<MenuItem> {
    let mut items = vec![MenuItem {
        label: "Continue",
        shortcut: 'c',
        action: MenuAction::Continue,
    }];
    if operation != git::model::Operation::Merge {
        items.push(MenuItem {
            label: "Skip this step",
            shortcut: 's',
            action: MenuAction::Skip,
        });
    }
    items.push(MenuItem {
        label: "Abort",
        shortcut: 'a',
        action: MenuAction::Abort,
    });
    items
}

impl App {
    /// `m`: the menu for the merge, rebase, cherry-pick or revert git is
    /// stopped in. Inert when there is none.
    pub(super) fn open_operation_menu(&mut self) {
        let Some(operation) = self.operation else {
            return;
        };
        if self.popup.is_some() || self.pending_confirm.is_some() {
            return;
        }
        self.popup = Some(Popup::Menu(MenuState {
            title: operation.label(),
            items: operation_items(operation),
            selected: 0,
        }));
    }

    /// Every key while a menu is up: `j` / `k` move, `Enter` or a row's own
    /// letter runs it, `Esc` closes.
    pub(super) fn menu_key(&mut self, key: KeyEvent) {
        let Some(Popup::Menu(menu)) = &mut self.popup else {
            return;
        };
        let last = menu.items.len().saturating_sub(1);
        let chosen = match key.code {
            KeyCode::Esc => {
                self.popup = None;
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
            self.popup = None;
            self.run_menu_action(action);
        }
    }

    fn run_menu_action(&mut self, action: MenuAction) {
        match action {
            MenuAction::Continue => self.apply_operation_step(Step::Continue),
            MenuAction::Skip => self.apply_operation_step(Step::Skip),
            // Throws away the resolution work so far: ask first.
            MenuAction::Abort => {
                let noun = self.operation.map_or("operation", operation_noun);
                self.pending_confirm = Some(ConfirmPrompt {
                    message: format!("abort the {noun}? Work done in it so far is lost."),
                    action: ConfirmAction::AbortOperation,
                });
            },
        }
    }

    /// Run one step and say where git stopped.
    pub(super) fn apply_operation_step(&mut self, step: Step) {
        let Some(repo) = &self.repo else { return };
        let result = repo.operation_step(step);
        self.finish_operation(result);
    }

    /// Refresh and report where git stopped after a step or a rewrite.
    /// Refreshes either way: a refusal changes nothing, a step changes a lot.
    pub(super) fn finish_operation(&mut self, result: GitResult<OperationOutcome>) {
        self.request_refresh();
        match result {
            Ok(OperationOutcome::Done) => {},
            Ok(OperationOutcome::Stopped { conflicted: true }) => {
                self.popup = Some(Popup::Note(
                    "stopped on a conflict. Resolve it in Files, then press m and Continue."
                        .to_owned(),
                ));
            },
            Ok(OperationOutcome::Stopped { conflicted: false }) => {
                self.popup = Some(Popup::Note(
                    "stopped for you to edit. Make your change, then press m and Continue."
                        .to_owned(),
                ));
            },
            Err(e) => self.report_error(e),
        }
    }
}
