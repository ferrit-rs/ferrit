//! Stash actions: `s` on Files opens a message popup, Stash-pane keys apply,
//! pop and drop the selected entry. See `docs/PLAN_10_STASH.md`.

use std::path::PathBuf;

use super::{
    App, ConfirmAction, ConfirmPrompt, DiffView, Mode, Pane, Popup, SelectionKey, TextInput, git,
};
use crate::domain::git::stash::StashOutcome;

impl App {
    /// The selected stash entry's oid, only while Stash is focused, in
    /// `Mode::Nav` and no popup is up.
    fn selected_stash(&self) -> Option<&git::model::StashEntry> {
        if self.focus != Pane::Stash || self.mode != Mode::Nav || self.popup.is_some() {
            return None;
        }
        self.snapshot.stashes.get(self.selected(Pane::Stash))
    }

    /// `s` (Nav, Files focused): open the stash message popup. A clean tree
    /// opens nothing and says so.
    pub(super) fn open_stash_popup(&mut self) {
        if self.focus != Pane::Files || self.mode != Mode::Nav || self.popup.is_some() {
            return;
        }
        if self.snapshot.files.is_empty() {
            self.report_error(git::error::GitError::NothingToStash);
            return;
        }
        self.popup = Some(Popup::Stash(TextInput::default()));
    }

    /// `Enter` in the stash popup: `git stash push --include-untracked`.
    /// Success and "nothing to stash" close it; any other failure keeps the
    /// popup and the typed message so the user can retry (the phase 8
    /// new-branch rule).
    pub(super) fn do_stash_push(&mut self) {
        let Some(Popup::Stash(buf)) = &self.popup else {
            return;
        };
        let message = buf.text();
        let Some(repo) = &self.repo else { return };
        match repo.stash_push(message.trim()) {
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

    /// `<space>` (apply) or `g` (pop) on the Stash pane: ask before doing either,
    /// same reason as drop (`docs/PLAN_10_STASH.md`) — both mutate the working
    /// tree at once, with no undo, exactly like the discard prompt they mirror.
    pub(super) fn restore_stash_prompt(&mut self, pop: bool) {
        let Some(entry) = self.selected_stash() else {
            return;
        };
        let verb = if pop { "pop" } else { "apply" };
        let message = format!("{verb} stash@{{{}}}: {}?", entry.index, entry.message);
        let oid = entry.oid.clone();
        self.pending_confirm = Some(ConfirmPrompt {
            message,
            action: ConfirmAction::RestoreStash { oid, pop },
        });
    }

    /// Confirmed apply or pop. A clean restore moves the focus to Files with
    /// the first restored file selected, like lazygit; a conflict or an error
    /// leaves the focus on Stash.
    pub(super) fn restore_stash(&mut self, oid: &str, pop: bool) {
        let first_file = match &self.right.diff {
            DiffView::Stash(entry, diff) if entry.oid == oid => diff
                .files
                .first()
                .and_then(|f| diff.text.get(f.new_path.clone()))
                .map(PathBuf::from),
            _ => None,
        };
        let Some(repo) = &mut self.repo else { return };
        let result = if pop {
            repo.stash_pop(oid)
        } else {
            repo.stash_apply(oid)
        };
        if matches!(result, Ok(StashOutcome::Done)) {
            self.focus = Pane::Files;
            if let Some(path) = first_file {
                self.select_when_listed(Pane::Files, SelectionKey::File(path));
            }
        }
        self.request_refresh();
        match result {
            Ok(StashOutcome::Done) => {},
            Ok(StashOutcome::Conflicted) => {
                self.popup = Some(Popup::Note(
                    "stash applied with conflicts. The stash was kept. Resolve the conflicts \
                     in Files."
                        .to_owned(),
                ));
            },
            Err(e) => self.report_error(e),
        }
    }

    /// `d` on the Stash pane: ask before dropping, the one irreversible
    /// stash action.
    pub(super) fn drop_stash_prompt(&mut self) {
        let Some(entry) = self.selected_stash() else {
            return;
        };
        let message = format!("drop stash@{{{}}}: {}?", entry.index, entry.message);
        let oid = entry.oid.clone();
        self.pending_confirm = Some(ConfirmPrompt {
            message,
            action: ConfirmAction::DropStash { oid },
        });
    }

    /// Confirmed `d`: `git stash drop`, refreshing either way.
    pub(super) fn drop_stash(&mut self, oid: &str) {
        let Some(repo) = &mut self.repo else { return };
        let result = repo.stash_drop(oid);
        self.finish_branch_action(result);
    }
}
