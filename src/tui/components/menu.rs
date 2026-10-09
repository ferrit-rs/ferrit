//! The menus: the `m` menu, the context menu, and the name popups they open.

use crate::git;
use crate::git::branch::MergeKind;
use crate::git::error::GitError;
use crate::git::port::GitPort;
use crate::git::rebase::Step;
use crate::theme::palette::Palette;
use crate::tui::components::branches;
use crate::tui::components::diff::right_pane::Mode;
use crate::tui::components::diff::views::MenuView;
use crate::tui::components::panes::nav::{BranchesTab, Pane};
use crate::tui::components::popups::{ConfirmAction, ConfirmPrompt, Popup};
use crate::tui::event::{Env, Event};
use crate::tui::operation_noun;
use crate::tui::widgets::chrome::Dialog;
use crate::tui::widgets::chrome::SelectList;
use crate::tui::widgets::text_input::TextInput;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;

/// `m`: the menu for the merge, rebase, cherry-pick or revert git is stopped in.
/// Inert when there is none.
pub(crate) fn open_operation(env: &Env<'_>) -> Vec<Event> {
    let Some(operation) = env.snapshot.operation else {
        return Vec::new();
    };
    if env.modal_up {
        return Vec::new();
    }
    vec![Event::OpenPopup(Popup::Menu(MenuState {
        title: operation.label(),
        items: operation_items(operation),
        selected: 0,
    }))]
}

/// What a key did to a menu.
pub(crate) enum MenuKey {
    /// Nothing to do beyond the highlight, which `on_key` already moved.
    Stay,
    /// Close the menu.
    Close,
    /// Close the menu and run this row.
    Choose(MenuAction),
}

/// Every key while a menu is up: `j` / `k` move, `Enter` or a row's own letter
/// runs it, `Esc` closes.
pub(crate) fn on_key(menu: &mut MenuState, key: KeyEvent) -> MenuKey {
    let last = menu.items.len().saturating_sub(1);
    let chosen = match key.code {
        KeyCode::Esc => return MenuKey::Close,
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
    chosen.map_or(MenuKey::Stay, MenuKey::Choose)
}

/// What choosing a row comes to.
pub(crate) fn run_action(action: MenuAction, env: &Env<'_>) -> Vec<Event> {
    match action {
        MenuAction::ConfigValue(index) => vec![Event::PickConfigValue(index)],
        MenuAction::CreateRemote => vec![Event::OpenCreateRemote],
        MenuAction::Continue => vec![Event::OperationStep(Step::Continue)],
        MenuAction::Skip => vec![Event::OperationStep(Step::Skip)],
        MenuAction::RenameBranch
        | MenuAction::MergeNoFf
        | MenuAction::MergeFf
        | MenuAction::SquashStaged
        | MenuAction::SquashCommit
        | MenuAction::BranchFromCommit
        | MenuAction::StashKeepIndex
        | MenuAction::RenameStash
        | MenuAction::TakeOurs
        | MenuAction::TakeTheirs => run_context_action(action, env),
        // Throws away the resolution work so far: ask first.
        MenuAction::Abort => {
            let noun = env.snapshot.operation.map_or("operation", operation_noun);
            vec![Event::Ask(ConfirmPrompt {
                message: format!("abort the {noun}? Work done in it so far is lost."),
                action: ConfirmAction::AbortOperation,
            })]
        },
    }
}

/// `x` or a right-click: the menu for the selected row, or nothing when it has
/// nothing extra. Inert while a popup or a question is up.
pub(crate) fn open_context(env: &Env<'_>) -> Vec<Event> {
    if env.modal_up || env.repo.is_none() {
        return Vec::new();
    }
    let index = env.nav.selection[env.nav.focus];
    let (title, mut items) = match env.nav.focus {
        Pane::Files if env.nav.mode == Mode::Nav => match env.rows().selected_file() {
            Some(file) if file.is_conflicted() => (
                format!("{} (conflict)", file.path.display()),
                vec![
                    item("Take ours", 'o', MenuAction::TakeOurs),
                    item("Take theirs", 't', MenuAction::TakeTheirs),
                ],
            ),
            _ => (String::new(), Vec::new()),
        },
        Pane::Branches
            if env.nav.branch_drill.is_none() && env.nav.branches_tab == BranchesTab::Local =>
        {
            match env.snapshot.branches.get(index) {
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
        Pane::Commits if env.nav.commit_drill.is_none() => match env.snapshot.commits.get(index) {
            Some(commit) => (
                commit.short_hash.clone(),
                vec![item(
                    "New branch from this commit",
                    'b',
                    MenuAction::BranchFromCommit,
                )],
            ),
            None => (String::new(), Vec::new()),
        },
        Pane::Stash => {
            let mut items = vec![item(
                "Stash, keeping the index",
                'i',
                MenuAction::StashKeepIndex,
            )];
            if env.snapshot.stashes.get(index).is_some() {
                items.push(item("Rename stash", 'r', MenuAction::RenameStash));
            }
            ("Stash".to_owned(), items)
        },
        _ => (String::new(), Vec::new()),
    };
    // A repository with no remote can be published: optional, any time.
    let publishable = env.snapshot.remotes.is_empty()
        && (env.nav.focus == Pane::Status
            || (env.nav.focus == Pane::Branches && env.nav.branch_drill.is_none()));
    if publishable {
        items.push(MenuItem {
            label: "Create a repository on GitHub",
            shortcut: 'g',
            action: MenuAction::CreateRemote,
            hint: "G from anywhere. Needs gh. Private by default; asks again before creating.",
        });
    }
    if items.is_empty() {
        return Vec::new();
    }
    let title = if title.is_empty() {
        "Repository".to_owned()
    } else {
        title
    };
    vec![Event::OpenPopup(Popup::Menu(MenuState {
        title,
        items,
        selected: 0,
    }))]
}

/// Run a menu action that belongs to the `x` menu, on the row it was opened for
/// (the selection has not moved: a menu is modal).
fn run_context_action(action: MenuAction, env: &Env<'_>) -> Vec<Event> {
    let index = env.nav.selection[env.nav.focus];
    match action {
        MenuAction::RenameBranch => {
            let Some(branch) = env.snapshot.branches.get(index) else {
                return Vec::new();
            };
            let from = branch.name.clone();
            let input = TextInput::from_text(&from);
            vec![open_name(
                NameKind::RenameBranch { from },
                "Rename branch".to_owned(),
                input,
            )]
        },
        MenuAction::MergeNoFf => branches::merge_with(env, MergeKind::NoFf),
        MenuAction::MergeFf => branches::merge_with(env, MergeKind::Regular),
        MenuAction::SquashStaged => branches::merge_with(env, MergeKind::Squash),
        MenuAction::SquashCommit => branches::merge_with(env, MergeKind::SquashCommit),
        MenuAction::BranchFromCommit => {
            let Some(commit) = env.snapshot.commits.get(index) else {
                return Vec::new();
            };
            let hash = commit.full_hash.clone();
            let title = format!("New branch from {}", commit.short_hash);
            vec![open_name(
                NameKind::BranchAt { hash },
                title,
                TextInput::default(),
            )]
        },
        MenuAction::StashKeepIndex => {
            if env.snapshot.files.is_empty() {
                return vec![Event::Report(GitError::NothingToStash.into())];
            }
            vec![open_name(
                NameKind::StashKeepIndex,
                "Stash, keeping the index".to_owned(),
                TextInput::default(),
            )]
        },
        MenuAction::RenameStash => {
            let Some(entry) = env.snapshot.stashes.get(index) else {
                return Vec::new();
            };
            let oid = entry.oid.clone();
            let input = TextInput::from_text(&entry.message);
            vec![open_name(
                NameKind::RenameStash { oid },
                "Rename stash".to_owned(),
                input,
            )]
        },
        MenuAction::TakeOurs | MenuAction::TakeTheirs => {
            let (Some(file), Some(repo)) = (env.rows().selected_file(), env.repo) else {
                return Vec::new();
            };
            vec![Event::FinishAction(
                repo.take_side(&file.path, action == MenuAction::TakeOurs),
            )]
        },
        MenuAction::Continue
        | MenuAction::Skip
        | MenuAction::Abort
        | MenuAction::ConfigValue(_)
        | MenuAction::CreateRemote => Vec::new(),
    }
}

/// A popup asking for a name (a branch, a stash message, a config key or value).
pub(crate) fn open_name(kind: NameKind, title: String, input: TextInput) -> Event {
    Event::OpenPopup(Popup::Name(NameTarget { kind, title }, input))
}

/// `Enter` in a name popup. Success closes it and refreshes; a refusal (a taken
/// name, an empty message) keeps the popup and the text for a retry, the same
/// rule as the new-branch popup.
pub(crate) fn submit_name(kind: &NameKind, text: &str, repo: &mut dyn GitPort) -> Vec<Event> {
    let name = text.trim();
    let result = match kind {
        NameKind::RenameBranch { from } if name == from => return vec![Event::ClosePopup],
        NameKind::RenameBranch { from } => repo.rename_branch(from.as_str(), name),
        NameKind::BranchAt { hash } => repo.create_branch_at(name, hash.as_str()),
        NameKind::RenameStash { .. } if name.is_empty() => {
            return vec![Event::Notice("a stash needs a message".to_owned())];
        },
        NameKind::RenameStash { oid } => repo.stash_rename(oid.as_str(), name),
        NameKind::StashKeepIndex => repo.stash_push_keeping_index(name),
        // Handled by the caller: a config value keeps its spaces.
        NameKind::ConfigKey | NameKind::ConfigValue(_) => return Vec::new(),
    };
    match result {
        Ok(()) => vec![Event::ClosePopup, Event::Refresh],
        Err(e @ GitError::NothingToStash) => vec![Event::ClosePopup, Event::Report(e.into())],
        Err(e) => vec![Event::Report(e.into())],
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
