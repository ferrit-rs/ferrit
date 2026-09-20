//! Branches-pane actions: checkout, create, delete, fast-forward, merge.

use super::{
    App, BranchDrill, BranchesTab, ConfirmAction, ConfirmPrompt, GitResult, Pane, Popup, TextInput,
    git,
};

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
            Err(e) => self.last_error = Some(e.to_string()),
        }
    }

    /// Refresh after a branch mutation (checkout / create / delete / fast-
    /// forward), then surface a failure in the Status pane. Same shape as
    /// `finish_apply`.
    pub(super) fn finish_branch_action(&mut self, result: GitResult<()>) {
        self.request_refresh();
        if let Err(e) = result {
            self.last_error = Some(e.to_string());
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

    /// `n` (Nav, Branches focused): open the new-branch popup, named from
    /// the current `HEAD` once submitted.
    pub(super) fn open_new_branch_popup(&mut self) {
        if self.focus != Pane::Branches
            || self.popup.is_some()
            || self.branch_drill.is_some()
            || self.branches_tab == BranchesTab::Remotes
        {
            return;
        }
        self.popup = Some(Popup::NewBranch(TextInput::default()));
    }

    /// `Enter` in the new-branch popup: `git checkout -b <name>` from
    /// `HEAD`. Success closes the popup and refreshes; failure (a bad
    /// name, or one already taken) keeps the popup open with the typed
    /// text so the user can fix it and retry — the message surfaces in
    /// the Status pane rather than a second popup layered on this one.
    pub(super) fn do_create_branch(&mut self) {
        let Some(Popup::NewBranch(buf)) = &self.popup else {
            return;
        };
        let name = buf.text();
        let Some(repo) = &self.repo else { return };
        match repo.create_branch(&name) {
            Ok(()) => {
                self.popup = None;
                self.request_refresh();
            },
            Err(e) => self.last_error = Some(e.to_string()),
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
                f.staged == git::status::Change::Conflicted
                    || f.worktree == git::status::Change::Conflicted
            })
            .map(|f| f.path.display().to_string())
            .collect()
    }

    /// `M` (Nav, Branches focused): merge the selected branch into the
    /// current one. `refresh()` always runs, even on a conflict — the
    /// Files pane already renders `Change::Conflicted`, so the conflicted
    /// paths are visible without a dedicated flow.
    pub(super) fn merge_selected_branch(&mut self) {
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
        let result = repo.merge_branch(&name);
        self.request_refresh();
        match result {
            Ok(git::branch::MergeOutcome::Merged) => {},
            Ok(git::branch::MergeOutcome::Conflicted) => {
                let files = self.conflicted_paths().join(", ");
                self.popup = Some(Popup::Note(format!(
                    "merge conflict in {files}. Resolve and commit, or `git merge --abort` \
                     from the shell — conflict resolution UI is phase 11."
                )));
            },
            Err(e) => self.last_error = Some(e.to_string()),
        }
    }
}
