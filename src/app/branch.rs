//! What the keys do in `App` for `branch`: the glue between the interface, the git code and the app's state.

use crate::app::App;
use crate::git::branch::{self, MergeKind, MergeOutcome};
use crate::git::error::GitResult;
use crate::git::staging;
use crate::interface::components::ui::text_input::TextInput;
use crate::interface::state::confirm::ConfirmPrompt;
use crate::interface::state::menu::MenuState;
use crate::interface::state::pane::{BranchesTab, Pane};
use crate::interface::state::popup::Popup;
use crate::interface::state::selection::SelectionKey;

impl App {
    /// Enter on the Branches pane: lazygit's branch -> log drill-down. Read
    /// only, no checkout. `Esc` backs out (`on_key`).
    pub(crate) fn enter_branch_log(&mut self) {
        if !self.nav.on_local_branches() {
            return;
        }
        let Some(repo) = &self.repo else { return };
        let Some(name) = self.rows().selected_branch().map(|b| b.name.clone()) else {
            return;
        };
        match repo.branch_log(&name) {
            Ok(commits) => self.nav.drill_into_branch(name, commits),
            Err(e) => self.report_error(e),
        }
    }

    /// Refresh after a branch mutation (checkout / create / delete / fast-
    /// forward), then surface a failure in the Status pane. Same shape as
    /// `finish_apply`.
    pub(crate) fn finish_branch_action(&mut self, result: GitResult<()>) {
        self.request_refresh();
        if let Err(e) = result {
            self.report_error(e);
        }
    }

    /// `<space>` on the Branches pane (`Mode::Nav`): checkout the selected
    /// branch. `refresh()` picks up the new `HEAD`, branches, and files (a
    /// checkout changes the working tree too).
    pub(crate) fn checkout_selected_branch(&mut self) {
        if !self.nav.on_local_branches() {
            return;
        }
        let Some(name) = self.rows().selected_branch().map(|b| b.name.clone()) else {
            return;
        };
        let Some(repo) = &self.repo else { return };
        let result = repo.checkout(&name);
        self.finish_branch_action(result);
    }

    /// `n` (Nav, Branches focused): open the new-branch popup, named from
    /// the selected branch once submitted.
    pub(crate) fn open_new_branch_popup(&mut self) {
        if !self.nav.on_local_branches() || self.modal.popup().is_some() {
            return;
        }
        self.new_branch_title = format!(
            "New branch name (branch is off of '{}')",
            self.rows()
                .selected_branch()
                .map_or(self.snapshot.header.branch.as_str(), |b| b.name.as_str())
        );
        self.modal
            .open_popup(Popup::NewBranch(TextInput::default()));
    }

    /// `Enter` in the new-branch popup: `git checkout -b <name>` from
    /// the selected branch, without tracking it. Success closes the popup and refreshes; failure (a bad
    /// name, or one already taken) keeps the popup open with the typed
    /// text so the user can fix it and retry: the message surfaces in
    /// the Status pane rather than a second popup layered on this one.
    pub(crate) fn do_create_branch(&mut self) {
        let Some(Popup::NewBranch(buf)) = self.modal.popup() else {
            return;
        };
        let name = buf.text();
        let base = self.rows().selected_branch().map(|b| b.name.clone());
        let Some(repo) = &self.repo else { return };
        let result = match base {
            Some(base) => repo.create_branch_at(&name, &format!("refs/heads/{base}")),
            None => repo.create_branch(&name),
        };
        match result {
            Ok(()) => {
                self.modal.close_popup();
                // The new branch is the checked-out one: select it, not the
                // row the cursor was on (lazygit).
                if self.nav.branch_drill.is_none() && self.nav.branches_tab == BranchesTab::Local {
                    self.nav
                        .select_when_listed(Pane::Branches, SelectionKey::Branch(name));
                }
                self.request_refresh();
            },
            Err(e) => self.report_error(e),
        }
    }

    /// `d` (Nav, Branches focused): ask before deleting the selected
    /// branch. The currently checked-out branch skips the confirm
    /// entirely: `git` refuses to delete it either way, so its own
    /// message goes straight to the Status line rather than opening a
    /// confirm for an outcome that is already certain.
    pub(crate) fn delete_branch_prompt(&mut self) {
        if !self.nav.on_local_branches() {
            return;
        }
        let Some(entry) = self.rows().selected_branch() else {
            return;
        };
        let name = entry.name.clone();
        if entry.is_head {
            let Some(repo) = &self.repo else { return };
            let result = repo.delete_branch(&name, false);
            self.finish_branch_action(result);
            return;
        }
        self.modal.ask(ConfirmPrompt::delete_branch(name));
    }

    /// `u` (Nav, Branches focused): fast-forward the selected branch to
    /// its upstream, checked out or not. No confirm: exactly as reversible as
    /// any other git command, the reflog has your back.
    pub(crate) fn fast_forward_selected_branch(&mut self) {
        if !self.nav.on_local_branches() {
            return;
        }
        let Some(name) = self.rows().selected_branch().map(|b| b.name.clone()) else {
            return;
        };
        let Some(repo) = &self.repo else { return };
        let result = repo.fast_forward(&name);
        self.finish_branch_action(result);
    }

    /// `M` (Nav, Branches focused): open the Merge menu for the selected
    /// branch, and merge nothing until a row is chosen. On the current branch
    /// there is nothing to choose between, so it merges straight away (git
    /// answers "Already up to date").
    pub(crate) fn merge_selected_branch(&mut self) {
        if !self.nav.on_local_branches() || self.modal.is_some() {
            return;
        }
        let Some(entry) = self.rows().selected_branch() else {
            return;
        };
        if entry.is_head {
            self.merge_selected_branch_with(MergeKind::Regular);
            return;
        }
        self.modal.open_popup(Popup::Menu(MenuState::merge()));
    }

    /// Merge the selected branch into the current one the way `kind` says.
    /// `refresh()` always runs, even on a conflict: the Files pane already
    /// renders `Change::Conflicted`, so the conflicted paths are visible
    /// without a dedicated flow.
    pub(crate) fn merge_selected_branch_with(&mut self, kind: MergeKind) {
        if !self.nav.on_local_branches() {
            return;
        }
        let Some(name) = self.rows().selected_branch().map(|b| b.name.clone()) else {
            return;
        };
        let Some(repo) = &self.repo else { return };
        let result = branch::merge(repo.as_ref(), &name, kind);
        self.request_refresh();
        match result {
            Ok(MergeOutcome::Merged) => {},
            Ok(MergeOutcome::Conflicted) => {
                let files = staging::conflicted_paths(&self.snapshot.files).join(", ");
                self.modal.open_popup(Popup::Note(format!(
                    "merge conflict in {files}. Fix the files and stage them with <space>, \
                     then press m and choose Continue, or Abort."
                )));
            },
            Err(e) => self.report_error(e),
        }
    }
}
