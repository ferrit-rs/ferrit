//! The `x` menu (and right-click): extra actions for the selected row that do
//! not earn a key of their own. Built on the generic `Popup::Menu` of
//! `app::menu`. See `docs/PLAN_12_POLISH.md` P4.
//!
//! | Pane | Entries |
//! | --- | --- |
//! | Branches | rename the branch, merge it with `--no-ff` |
//! | Commits | new branch from this commit |
//! | Stash | stash keeping the index, rename the entry |
//! | Files (a conflicted file) | take ours, take theirs |

use super::menu::{MenuAction, MenuItem, MenuState};
use super::{App, BranchesTab, Mode, Pane, Popup, TextInput, git};

/// What a name popup will do with the text typed into it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum NameKind {
    RenameBranch { from: String },
    BranchAt { hash: String },
    RenameStash { oid: String },
    StashKeepIndex,
}

/// A name popup's purpose and its title (which may name a commit).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct NameTarget {
    pub(super) kind: NameKind,
    pub(super) title: String,
}

impl NameTarget {
    /// The footer hint for this popup.
    pub(super) fn hints(&self) -> &'static str {
        match self.kind {
            NameKind::RenameBranch { .. } | NameKind::RenameStash { .. } => {
                "Rename: Enter | Cancel: Esc"
            },
            NameKind::BranchAt { .. } => "Create: Enter | Cancel: Esc",
            NameKind::StashKeepIndex => "Stash: Enter | Cancel: Esc",
        }
    }
}

fn item(label: &'static str, shortcut: char, action: MenuAction) -> MenuItem {
    MenuItem {
        label,
        shortcut,
        action,
    }
}

impl App {
    /// `x` or a right-click: the menu for the selected row, or a note that it
    /// has nothing extra. Inert while a popup or a confirm is up.
    pub(super) fn open_context_menu(&mut self) {
        if self.popup.is_some() || self.pending_confirm.is_some() || self.repo.is_none() {
            return;
        }
        let index = self.selected(self.focus);
        let (title, items) = match self.focus {
            Pane::Files if self.mode == Mode::Nav => match self.selected_file() {
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
                if self.branch_drill.is_none() && self.branches_tab == BranchesTab::Local =>
            {
                match self.branches.get(index) {
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
            Pane::Commits if self.commit_drill.is_none() => match self.commits.get(index) {
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
                if self.stashes.get(index).is_some() {
                    items.push(item("Rename stash", 'r', MenuAction::RenameStash));
                }
                ("Stash".to_owned(), items)
            },
            _ => (String::new(), Vec::new()),
        };
        if items.is_empty() {
            self.report_notice("no extra actions for this row");
            return;
        }
        self.popup = Some(Popup::Menu(MenuState {
            title,
            items,
            selected: 0,
        }));
    }

    /// Run a menu action that belongs to the `x` menu, on the row it was opened
    /// for (the selection has not moved: a menu is modal).
    pub(super) fn run_context_action(&mut self, action: MenuAction) {
        let index = self.selected(self.focus);
        match action {
            MenuAction::RenameBranch => {
                let Some(branch) = self.branches.get(index) else {
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
            MenuAction::MergeNoFf => self.merge_selected_branch_with(true),
            MenuAction::BranchFromCommit => {
                let Some(commit) = self.commits.get(index) else {
                    return;
                };
                let hash = commit.full_hash.clone();
                let title = format!("New branch from {}", commit.short_hash);
                self.open_name(NameKind::BranchAt { hash }, title, TextInput::default());
            },
            MenuAction::StashKeepIndex => {
                if self.files.is_empty() {
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
                let Some(entry) = self.stashes.get(index) else {
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
                let Some(file) = self.selected_file() else {
                    return;
                };
                let path = file.path.clone();
                let Some(repo) = &self.repo else { return };
                let result = repo.take_side(&path, action == MenuAction::TakeOurs);
                self.finish_apply(result);
            },
            MenuAction::Continue | MenuAction::Skip | MenuAction::Abort => {},
        }
    }

    fn open_name(&mut self, kind: NameKind, title: String, input: TextInput) {
        self.popup = Some(Popup::Name(NameTarget { kind, title }, input));
    }

    /// `Enter` in a name popup. Success closes it and refreshes; a refusal
    /// (a taken name, an empty message) keeps the popup and the text for a
    /// retry, the same rule as the new-branch popup.
    pub(super) fn submit_name(&mut self) {
        let Some(Popup::Name(target, input)) = &self.popup else {
            return;
        };
        let kind = target.kind.clone();
        let text = input.text();
        let name = text.trim();
        let Some(repo) = &mut self.repo else { return };
        let result = match &kind {
            NameKind::RenameBranch { from } if name == from => {
                self.popup = None;
                return;
            },
            NameKind::RenameBranch { from } => repo.rename_branch(from, name),
            NameKind::BranchAt { hash } => repo.create_branch_at(name, hash),
            NameKind::RenameStash { .. } if name.is_empty() => {
                self.report_notice("a stash needs a message");
                return;
            },
            NameKind::RenameStash { oid } => repo.stash_rename(oid, name),
            NameKind::StashKeepIndex => repo.stash_push_keeping_index(name),
        };
        match result {
            Ok(()) => {
                self.popup = None;
                self.request_refresh();
            },
            Err(e @ git::error::GitError::NothingToStash) => {
                self.popup = None;
                self.report_error(e);
            },
            Err(e) => self.report_error(e),
        }
    }
}
