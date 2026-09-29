//! Branches-pane actions: checkout, create, delete, fast-forward, merge.

use super::menu::{MenuAction, MenuItem, MenuState};
use super::{
    App, BranchDrill, BranchesTab, ConfirmAction, ConfirmPrompt, GitResult, Pane, Popup,
    SelectionKey, TextInput, git,
};

/// How a merge is done: the choices of the `M` menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum MergeKind {
    Regular,
    NoFf,
    Squash,
    SquashCommit,
}

impl App {
    /// `Ctrl-Right` / `Ctrl-Left`, Branches focused: switch its own Local
    /// branches / Remotes tab. A no-op while drilled into a branch's log —
    /// there is only one tab's worth of content to show there.
    pub(super) fn toggle_branches_tab(&mut self) {
        if self.focus != Pane::Branches || self.branch_drill.is_some() {
            return;
        }
        self.branches_tab = match self.branches_tab {
            BranchesTab::Local => BranchesTab::Remotes,
            BranchesTab::Remotes => BranchesTab::Local,
        };
    }

    /// Enter on the Branches pane: lazygit's branch -> log drill-down. Swaps
    /// the pane's own branch list for the selected branch's commit history,
    /// in place — focus stays on Branches, only its rows and title change
    /// (`branches_title`). Read only, no checkout. `Esc` backs out (`on_key`).
    pub(super) fn enter_branch_log(&mut self) {
        if self.focus != Pane::Branches
            || self.branch_drill.is_some()
            || self.branches_tab == BranchesTab::Remotes
        {
            return;
        }
        let Some(repo) = &self.repo else { return };
        let return_index = self.selected(Pane::Branches);
        let Some(branch) = self.branches.get(return_index) else {
            return;
        };
        let name = branch.name.clone();
        match repo.branch_log(&name) {
            Ok(commits) => {
                self.branch_drill = Some(BranchDrill {
                    branch: name,
                    commits,
                    return_index,
                });
                self.selection[Pane::Branches] = 0;
            },
            Err(e) => self.report_error(e),
        }
    }

    /// Refresh after a branch mutation (checkout / create / delete / fast-
    /// forward), then surface a failure in the Status pane. Same shape as
    /// `finish_apply`.
    pub(super) fn finish_branch_action(&mut self, result: GitResult<()>) {
        self.request_refresh();
        if let Err(e) = result {
            self.report_error(e);
        }
    }

    /// `<space>` on the Branches pane (`Mode::Nav`): checkout the selected
    /// branch. `refresh()` picks up the new `HEAD`, branches, and files (a
    /// checkout changes the working tree too). No-op while drilled into a
    /// branch's log, where the selected row is a commit, not a branch.
    pub(super) fn checkout_selected_branch(&mut self) {
        if self.focus != Pane::Branches
            || self.branch_drill.is_some()
            || self.branches_tab == BranchesTab::Remotes
        {
            return;
        }
        let Some(entry) = self.branches.get(self.selected(Pane::Branches)) else {
            return;
        };
        let name = entry.name.clone();
        let Some(repo) = &self.repo else { return };
        let result = repo.checkout(&name);
        self.finish_branch_action(result);
    }

    /// The branch `n` starts from: the selected one (lazygit), or `None` when
    /// the list has no row (a detached `HEAD`), which falls back to `HEAD`.
    fn new_branch_base(&self) -> Option<String> {
        let entry = self.branches.get(self.selected(Pane::Branches))?;
        Some(entry.name.clone())
    }

    /// `n` (Nav, Branches focused): open the new-branch popup, named from
    /// the selected branch once submitted.
    pub(super) fn open_new_branch_popup(&mut self) {
        if self.focus != Pane::Branches
            || self.popup.is_some()
            || self.branch_drill.is_some()
            || self.branches_tab == BranchesTab::Remotes
        {
            return;
        }
        self.new_branch_title = format!(
            "New branch name (branch is off of '{}')",
            self.new_branch_base()
                .as_deref()
                .unwrap_or(&self.header.branch)
        );
        self.popup = Some(Popup::NewBranch(TextInput::default()));
    }

    /// `Enter` in the new-branch popup: `git checkout -b <name>` from
    /// the selected branch, without tracking it. Success closes the popup and refreshes; failure (a bad
    /// name, or one already taken) keeps the popup open with the typed
    /// text so the user can fix it and retry — the message surfaces in
    /// the Status pane rather than a second popup layered on this one.
    pub(super) fn do_create_branch(&mut self) {
        let Some(Popup::NewBranch(buf)) = &self.popup else {
            return;
        };
        let name = buf.text();
        let base = self.new_branch_base();
        let Some(repo) = &self.repo else { return };
        let result = match base {
            Some(base) => repo.create_branch_at(&name, &format!("refs/heads/{base}")),
            None => repo.create_branch(&name),
        };
        match result {
            Ok(()) => {
                self.popup = None;
                // The new branch is the checked-out one: select it, not the
                // row the cursor was on (lazygit).
                if self.branch_drill.is_none() && self.branches_tab == BranchesTab::Local {
                    self.select_when_listed(Pane::Branches, SelectionKey::Branch(name));
                }
                self.request_refresh();
            },
            Err(e) => self.report_error(e),
        }
    }

    /// `d` (Nav, Branches focused): ask before deleting the selected
    /// branch. The currently checked-out branch skips the confirm
    /// entirely — `git` refuses to delete it either way, so its own
    /// message goes straight to `last_error`, the same "explain, do
    /// nothing" path an invalid discard already takes, rather than
    /// opening a confirm for an outcome that is already certain.
    pub(super) fn delete_branch_prompt(&mut self) {
        if self.focus != Pane::Branches
            || self.branch_drill.is_some()
            || self.branches_tab == BranchesTab::Remotes
        {
            return;
        }
        let Some(entry) = self.branches.get(self.selected(Pane::Branches)) else {
            return;
        };
        let name = entry.name.clone();
        if entry.is_head {
            let Some(repo) = &self.repo else { return };
            let result = repo.delete_branch(&name, false);
            self.finish_branch_action(result);
            return;
        }
        self.pending_confirm = Some(ConfirmPrompt {
            message: format!("delete branch {name}?"),
            action: ConfirmAction::DeleteBranch { name, force: false },
        });
    }

    /// `u` (Nav, Branches focused): fast-forward the selected branch to
    /// its upstream, checked out or not (`Repo::fast_forward` picks the
    /// mechanism). No confirm: exactly as reversible as any other git
    /// command, the reflog has your back the same way it does from a
    /// shell.
    pub(super) fn fast_forward_selected_branch(&mut self) {
        if self.focus != Pane::Branches
            || self.branch_drill.is_some()
            || self.branches_tab == BranchesTab::Remotes
        {
            return;
        }
        let Some(entry) = self.branches.get(self.selected(Pane::Branches)) else {
            return;
        };
        let name = entry.name.clone();
        let Some(repo) = &self.repo else { return };
        let result = repo.fast_forward(&name);
        self.finish_branch_action(result);
    }

    /// Every changed path currently reported as conflicted (staged or
    /// worktree side), for the merge-conflict note's message.
    pub(super) fn conflicted_paths(&self) -> Vec<String> {
        self.files
            .iter()
            .filter(|f| {
                f.staged == git::model::Change::Conflicted
                    || f.worktree == git::model::Change::Conflicted
            })
            .map(|f| f.path.display().to_string())
            .collect()
    }

    /// `M` (Nav, Branches focused): open the Merge menu for the selected
    /// branch, and merge nothing until a row is chosen. On the current branch
    /// there is nothing to choose between, so it merges straight away (git
    /// answers "Already up to date").
    pub(super) fn merge_selected_branch(&mut self) {
        if self.focus != Pane::Branches
            || self.branch_drill.is_some()
            || self.branches_tab == BranchesTab::Remotes
            || self.popup.is_some()
            || self.pending_confirm.is_some()
        {
            return;
        }
        let Some(entry) = self.branches.get(self.selected(Pane::Branches)) else {
            return;
        };
        if entry.is_head {
            self.merge_selected_branch_with(MergeKind::Regular);
            return;
        }
        let item = |label, shortcut, action, hint| MenuItem {
            label,
            shortcut,
            action,
            hint,
        };
        self.popup = Some(Popup::Menu(MenuState {
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
        }));
    }

    /// Merge the selected branch into the current one the way `kind` says.
    /// `refresh()` always runs, even on a conflict: the Files pane already
    /// renders `Change::Conflicted`, so the conflicted paths are visible
    /// without a dedicated flow.
    pub(super) fn merge_selected_branch_with(&mut self, kind: MergeKind) {
        if self.focus != Pane::Branches
            || self.branch_drill.is_some()
            || self.branches_tab == BranchesTab::Remotes
        {
            return;
        }
        let Some(entry) = self.branches.get(self.selected(Pane::Branches)) else {
            return;
        };
        let name = entry.name.clone();
        let Some(repo) = &self.repo else { return };
        let result = match kind {
            MergeKind::Regular => repo.merge_branch(&name),
            MergeKind::NoFf => repo.merge_branch_no_ff(&name),
            MergeKind::Squash => repo
                .merge_squash(&name, false)
                .map(|()| git::branch::MergeOutcome::Merged),
            MergeKind::SquashCommit => repo
                .merge_squash(&name, true)
                .map(|()| git::branch::MergeOutcome::Merged),
        };
        self.request_refresh();
        match result {
            Ok(git::branch::MergeOutcome::Merged) => {},
            Ok(git::branch::MergeOutcome::Conflicted) => {
                let files = self.conflicted_paths().join(", ");
                self.popup = Some(Popup::Note(format!(
                    "merge conflict in {files}. Fix the files and stage them with <space>, \
                     then press m and choose Continue, or Abort."
                )));
            },
            Err(e) => self.report_error(e),
        }
    }
}
