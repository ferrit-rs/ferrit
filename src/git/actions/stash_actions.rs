//! Stash actions: `s` on Files opens a message popup, Stash-pane keys apply,
//! pop and drop the selected entry. See `docs/PLAN_10_STASH.md`.

use crate::app::App;
use crate::git;
use crate::git::stash::{self, StashOutcome};
use crate::interface::components::ui::text_input::TextInput;
use crate::interface::panes::pane::Pane;
use crate::interface::panes::selection::SelectionKey;
use crate::interface::popups::confirm::ConfirmPrompt;
use crate::interface::popups::popup::Popup;

impl App {
    /// The selected stash entry, only while Stash is focused in `Mode::Nav`
    /// and no popup is up.
    fn selected_stash(&self) -> Option<&git::model::StashEntry> {
        if !self.nav.on_stash() || self.modal.popup().is_some() {
            return None;
        }
        self.rows().selected_stash()
    }

    /// `s` (Nav, Files focused): open the stash message popup. A clean tree
    /// opens nothing and says so.
    pub(crate) fn open_stash_popup(&mut self) {
        if !self.nav.on_files() || self.modal.popup().is_some() {
            return;
        }
        if self.snapshot.files.is_empty() {
            self.report_error(git::error::GitError::NothingToStash);
            return;
        }
        self.modal.open_popup(Popup::Stash(TextInput::default()));
    }

    /// `Enter` in the stash popup: `git stash push --include-untracked`.
    /// Success and "nothing to stash" close it; any other failure keeps the
    /// popup and the typed message so the user can retry (the phase 8
    /// new-branch rule).
    pub(crate) fn do_stash_push(&mut self) {
        let Some(Popup::Stash(buf)) = self.modal.popup() else {
            return;
        };
        let message = buf.text();
        let Some(repo) = &self.repo else { return };
        match repo.stash_push(message.trim()) {
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

    /// `<space>` (apply) or `g` (pop) on the Stash pane: ask before doing either.
    pub(crate) fn restore_stash_prompt(&mut self, pop: bool) {
        let Some(entry) = self.selected_stash() else {
            return;
        };
        let prompt = ConfirmPrompt::restore_stash(entry, pop);
        self.modal.ask(prompt);
    }

    /// Confirmed apply or pop. A clean restore moves the focus to Files with
    /// the first restored file selected, like lazygit; a conflict or an error
    /// leaves the focus on Stash.
    pub(crate) fn restore_stash(&mut self, oid: &str, pop: bool) {
        let first_file = self.right.diff.first_stash_file(oid);
        let Some(repo) = &mut self.repo else { return };
        let result = stash::restore(repo.as_mut(), oid, pop);
        if matches!(result, Ok(StashOutcome::Done)) {
            self.nav.focus = Pane::Files;
            if let Some(path) = first_file {
                self.nav
                    .select_when_listed(Pane::Files, SelectionKey::File(path));
            }
        }
        self.request_refresh();
        match result {
            Ok(StashOutcome::Done) => {},
            Ok(StashOutcome::Conflicted) => {
                self.modal.open_popup(Popup::Note(
                    "stash applied with conflicts. The stash was kept. Resolve the conflicts \
                     in Files."
                        .to_owned(),
                ));
            },
            Err(e) => self.report_error(e),
        }
    }

    /// `d` on the Stash pane: ask before dropping.
    pub(crate) fn drop_stash_prompt(&mut self) {
        let Some(entry) = self.selected_stash() else {
            return;
        };
        let prompt = ConfirmPrompt::drop_stash(entry);
        self.modal.ask(prompt);
    }

    /// Confirmed `d`: `git stash drop`, refreshing either way.
    pub(crate) fn drop_stash(&mut self, oid: &str) {
        let Some(repo) = &mut self.repo else { return };
        let result = repo.stash_drop(oid);
        self.finish_branch_action(result);
    }
}
