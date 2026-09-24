//! Stash actions: `s` on Files opens a message popup, Stash-pane keys apply,
//! pop and drop the selected entry. See `docs/PLAN_10_STASH.md`.

use super::{App, ConfirmAction, ConfirmPrompt, Mode, Pane, Popup, TextInput, git};
use crate::domain::git::stash::StashOutcome;

impl App {
    /// The selected stash entry's oid, only while Stash is focused, in
    /// `Mode::Nav` and no popup is up.
    fn selected_stash(&self) -> Option<&git::model::StashEntry> {
        if self.focus != Pane::Stash || self.mode != Mode::Nav || self.popup.is_some() {
            return None;
        }
        self.stashes.get(self.selected(Pane::Stash))
    }

    /// `s` (Nav, Files focused): open the stash message popup. A clean tree
    /// opens nothing and says so.
    pub(super) fn open_stash_popup(&mut self) {
        if self.focus != Pane::Files || self.mode != Mode::Nav || self.popup.is_some() {
            return;
        }
        if self.files.is_empty() {
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

    /// `<space>` (apply) or `g` (pop) on the Stash pane.
    pub(super) fn restore_selected_stash(&mut self, pop: bool) {
        let Some(oid) = self.selected_stash().map(|entry| entry.oid.clone()) else {
            return;
        };
        let Some(repo) = &mut self.repo else { return };
        let result = if pop {
            repo.stash_pop(&oid)
        } else {
            repo.stash_apply(&oid)
        };
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
