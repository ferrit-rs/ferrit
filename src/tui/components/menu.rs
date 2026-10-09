//! The menus: the `m` menu, the context menu, and the name popups they open.

use crate::git;
use crate::git::branch::MergeKind;
use crate::git::error::GitResult;
use crate::git::operation::{OperationOutcome, Step};
use crate::theme::palette::Palette;
use crate::tui::components::branches;
use crate::tui::components::diff::{MenuView, Mode};
use crate::tui::components::panes::{BranchesTab, Pane};
use crate::tui::components::popups::{ConfirmAction, ConfirmPrompt, Popup};
use crate::tui::widgets::dialog::Dialog;
use crate::tui::widgets::select_list::SelectList;
use crate::tui::widgets::text_input::TextInput;
use crate::tui::{App, operation_noun};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

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

impl App {
    /// `x` or a right-click: the menu for the selected row, or a note that it
    /// has nothing extra. Inert while a popup or a confirm is up.
    pub(crate) fn open_context_menu(&mut self) {
        if self.modal.is_some() || self.repo.is_none() {
            return;
        }
        let index = self.selected(self.nav.focus);
        let (title, mut items) = match self.nav.focus {
            Pane::Files if self.nav.mode == Mode::Nav => match self.rows().selected_file() {
                Some(file)
                    if file.staged == git::model::Change::Conflicted
                        || file.worktree == git::model::Change::Conflicted =>
                {
                    (
                        format!("{} (conflict)", file.path.display()),
                        vec![
                            item("Take ours", 'o', MenuAction::TakeOurs),
                            item("Take theirs", 't', MenuAction::TakeTheirs),
                        ],
                    )
                },
                _ => (String::new(), Vec::new()),
            },
            Pane::Branches
                if self.nav.branch_drill.is_none()
                    && self.nav.branches_tab == BranchesTab::Local =>
            {
                match self.snapshot.branches.get(index) {
                    Some(branch) => (
                        branch.name.clone(),
                        vec![
                            item("Rename branch", 'r', MenuAction::RenameBranch),
                            item("Merge with --no-ff", 'n', MenuAction::MergeNoFf),
                        ],
                    ),
                    None => (String::new(), Vec::new()),
                }
            },
            Pane::Commits if self.nav.commit_drill.is_none() => {
                match self.snapshot.commits.get(index) {
                    Some(commit) => (
                        commit.short_hash.clone(),
                        vec![item(
                            "New branch from this commit",
                            'b',
                            MenuAction::BranchFromCommit,
                        )],
                    ),
                    None => (String::new(), Vec::new()),
                }
            },
            Pane::Stash => {
                let mut items = vec![item(
                    "Stash, keeping the index",
                    'i',
                    MenuAction::StashKeepIndex,
                )];
                if self.snapshot.stashes.get(index).is_some() {
                    items.push(item("Rename stash", 'r', MenuAction::RenameStash));
                }
                ("Stash".to_owned(), items)
            },
            _ => (String::new(), Vec::new()),
        };
        // A repository with no remote can be published: optional, any time.
        let publishable = self.snapshot.remotes.is_empty()
            && (self.nav.focus == Pane::Status
                || (self.nav.focus == Pane::Branches && self.nav.branch_drill.is_none()));
        if publishable {
            items.push(MenuItem {
                label: "Create a repository on GitHub",
                shortcut: 'g',
                action: MenuAction::CreateRemote,
                hint: "G from anywhere. Needs gh. Private by default; asks again before creating.",
            });
        }
        if items.is_empty() {
            return;
        }
        let title = if title.is_empty() {
            "Repository".to_owned()
        } else {
            title
        };
        self.modal.open_popup(Popup::Menu(MenuState {
            title,
            items,
            selected: 0,
        }));
    }

    /// Run a menu action that belongs to the `x` menu, on the row it was opened
    /// for (the selection has not moved: a menu is modal).
    pub(crate) fn run_context_action(&mut self, action: MenuAction) {
        let index = self.selected(self.nav.focus);
        match action {
            MenuAction::RenameBranch => {
                let Some(branch) = self.snapshot.branches.get(index) else {
                    return;
                };
                let from = branch.name.clone();
                let input = TextInput::from_text(&from);
                self.open_name(
                    NameKind::RenameBranch { from },
                    "Rename branch".to_owned(),
                    input,
                );
            },
            MenuAction::MergeNoFf => self.apply(branches::merge_with(&self.env(), MergeKind::NoFf)),
            MenuAction::MergeFf => {
                self.apply(branches::merge_with(&self.env(), MergeKind::Regular));
            },
            MenuAction::SquashStaged => {
                self.apply(branches::merge_with(&self.env(), MergeKind::Squash));
            },
            MenuAction::SquashCommit => {
                self.apply(branches::merge_with(&self.env(), MergeKind::SquashCommit));
            },
            MenuAction::BranchFromCommit => {
                let Some(commit) = self.snapshot.commits.get(index) else {
                    return;
                };
                let hash = commit.full_hash.clone();
                let title = format!("New branch from {}", commit.short_hash);
                self.open_name(NameKind::BranchAt { hash }, title, TextInput::default());
            },
            MenuAction::StashKeepIndex => {
                if self.snapshot.files.is_empty() {
                    self.report_error(git::error::GitError::NothingToStash);
                    return;
                }
                self.open_name(
                    NameKind::StashKeepIndex,
                    "Stash, keeping the index".to_owned(),
                    TextInput::default(),
                );
            },
            MenuAction::RenameStash => {
                let Some(entry) = self.snapshot.stashes.get(index) else {
                    return;
                };
                let oid = entry.oid.clone();
                let input = TextInput::from_text(&entry.message);
                self.open_name(
                    NameKind::RenameStash { oid },
                    "Rename stash".to_owned(),
                    input,
                );
            },
            MenuAction::TakeOurs | MenuAction::TakeTheirs => {
                let Some(file) = self.rows().selected_file() else {
                    return;
                };
                let path = file.path.clone();
                let Some(repo) = &self.repo else { return };
                let result = repo.take_side(&path, action == MenuAction::TakeOurs);
                self.finish_apply(result);
            },
            MenuAction::Continue
            | MenuAction::Skip
            | MenuAction::Abort
            | MenuAction::ConfigValue(_)
            | MenuAction::CreateRemote => {},
        }
    }

    pub(crate) fn open_name(&mut self, kind: NameKind, title: String, input: TextInput) {
        self.modal
            .open_popup(Popup::Name(NameTarget { kind, title }, input));
    }

    /// `Enter` in a name popup. Success closes it and refreshes; a refusal
    /// (a taken name, an empty message) keeps the popup and the text for a
    /// retry, the same rule as the new-branch popup.
    pub(crate) fn submit_name(&mut self) {
        let Some(Popup::Name(target, input)) = self.modal.popup() else {
            return;
        };
        let kind = target.kind.clone();
        let text = input.text();
        if matches!(kind, NameKind::ConfigKey | NameKind::ConfigValue(_)) {
            // A value keeps its spaces; a refusal keeps the popup for a retry.
            if self.submit_git_config_name(&kind, &text) {
                self.modal.close_popup();
            }
            return;
        }
        let name = text.trim();
        let Some(repo) = &mut self.repo else { return };
        let result = match &kind {
            NameKind::RenameBranch { from } if name == from => {
                self.modal.close_popup();
                return;
            },
            NameKind::RenameBranch { from } => repo.rename_branch(from.as_str(), name),
            NameKind::BranchAt { hash } => repo.create_branch_at(name, hash.as_str()),
            NameKind::RenameStash { .. } if name.is_empty() => {
                self.report_notice("a stash needs a message");
                return;
            },
            NameKind::RenameStash { oid } => repo.stash_rename(oid.as_str(), name),
            NameKind::StashKeepIndex => repo.stash_push_keeping_index(name),
            // Handled above: a config value keeps its spaces.
            NameKind::ConfigKey | NameKind::ConfigValue(_) => return,
        };
        match result {
            Ok(()) => {
                self.modal.close_popup();
                self.request_refresh();
            },
            Err(e @ git::error::GitError::NothingToStash) => {
                self.modal.close_popup();
                self.report_error(e);
            },
            Err(e) => self.report_error(e),
        }
    }

    /// A right-click on a row: focus that pane, move its selection there, then
    /// open its menu. Off any row it does nothing. (A left click also toggles a
    /// directory; this one must not.)
    pub(crate) fn right_click(&mut self, column: u16, row: u16) {
        if self.modal.is_some() || self.help.open {
            return;
        }
        let Some(pane) = self.pane_at(column, row) else {
            return;
        };
        self.nav.right_focused = false;
        self.nav.mode = Mode::Nav;
        if self.click_pane(pane, row) {
            self.update_right_pane();
            self.open_context_menu();
        }
    }
}

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

/// A list of actions with the highlighted row filled, `Enter` or a row's own
/// letter to run it.
pub(crate) fn draw_menu(
    frame: &mut Frame<'_>,
    area: Rect,
    view: &MenuView,
    accent: ratatui::style::Color,
    palette: &Palette,
) {
    let focused = Style::new().fg(accent).add_modifier(Modifier::BOLD);
    let rows = u16::try_from(view.rows.len()).unwrap_or(u16::MAX).max(1);
    let hint_width = u16::try_from(view.hint.chars().count() + 4).unwrap_or(u16::MAX);
    let dialog = Dialog::new(Line::styled(format!(" {} ", view.title), focused))
        .fit_content(44.max(hint_width).min(area.width), rows, 1)
        .border_style(focused)
        .render(frame, area);
    let lines: Vec<Line<'static>> = view
        .rows
        .iter()
        .map(|row| Line::from(format!(" {row}")))
        .collect();
    SelectList::new(&lines, view.selected)
        .selection_style(
            Style::new()
                .fg(palette.selection_fg)
                .bg(palette.selection)
                .add_modifier(Modifier::BOLD),
        )
        .render(frame, dialog.body);
    frame.render_widget(
        Paragraph::new(Line::styled(
            if view.hint.is_empty() {
                "Enter / letter run \u{b7} Esc close"
            } else {
                view.hint
            },
            Style::new().fg(palette.idle),
        )),
        dialog.footer,
    );
}
