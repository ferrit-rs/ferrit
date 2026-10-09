//! Opening the commit editor and making the commit. The editor itself (its
//! state, its keys) is `interface::popups::commit_draft`.

use crate::app::App;
use crate::app::error::AppError;
use crate::git;
use crate::git::apply::ApplyDir;
use crate::interface::components::tui_overlay::state::OverlayState;
use crate::interface::panes::pane::Pane;
use crate::interface::panes::selection::SelectionKey;
use crate::interface::panes::views::CommitPopupView;
use crate::interface::popups::commit_draft::{CommitDraft, DraftKey, RewordTarget};
use crate::interface::popups::popup::Popup;
use ratatui::crossterm::event::{KeyCode, KeyEvent};

impl App {
    /// The commit popup as data, without any animation state.
    pub fn commit_popup(&self) -> Option<CommitPopupView<'_>> {
        self.commit_popup_with(None)
    }

    /// The commit popup for drawing: `overlay` is its animation.
    pub(crate) fn commit_popup_with<'a>(
        &'a self,
        overlay: Option<&'a mut OverlayState>,
    ) -> Option<CommitPopupView<'a>> {
        let author = self.author_line();
        let Some(Popup::Commit(draft)) = self.modal.popup() else {
            return None;
        };
        Some(draft.view(author, overlay))
    }

    /// Replace the identities git knows globally. Integration-test seam: the
    /// real ones come from the machine's own git config.
    #[doc(hidden)]
    pub fn set_global_identities(&mut self, identities: Vec<(String, String)>) {
        self.authorship.profile.settings.global_identities = identities
            .into_iter()
            .map(|(name, email)| git::profile::settings::Identity {
                name,
                email: Some(email),
            })
            .collect();
    }

    /// The popup's author line: who the next commit is by. Ferrit's pick is for
    /// this run only and never writes git's config.
    fn author_line(&self) -> String {
        self.authorship.line()
    }

    /// `Ctrl-A`: the next identity git knows (from its global config), then
    /// git's own, then round again. Nothing to cycle when git knows none.
    fn cycle_author(&mut self) {
        self.authorship.cycle();
    }

    /// `c` / `A` / `w`: open the commit editor. Amend / Reword pre-fill
    /// `HEAD`'s message; a plain commit restores a cancelled draft.
    pub(crate) fn open_commit(&mut self, kind: git::commit::CommitKind) {
        if self.modal.popup().is_some() {
            return;
        }
        if self.repo.is_none() {
            return;
        }
        match &kind {
            git::commit::CommitKind::Normal
                if !self
                    .snapshot
                    .files
                    .iter()
                    .any(|f| f.staged != git::model::Change::None) =>
            {
                self.modal.open_popup(Popup::CommitAllConfirm);
                self.render.commit.open();
                return;
            },
            git::commit::CommitKind::Amend | git::commit::CommitKind::Reword
                if self.snapshot.commits.is_empty() =>
            {
                self.report_error(AppError::NoCommitToAmend);
                return;
            },
            _ => {},
        }

        let prefill = match &kind {
            git::commit::CommitKind::Amend | git::commit::CommitKind::Reword => {
                let Some(repo) = &self.repo else { return };
                repo.head_message().ok().flatten()
            },
            git::commit::CommitKind::Normal => self.new_commit_prefill(),
            _ => self.commit_draft.take(),
        };
        self.open_commit_editor(kind, prefill.as_deref());
    }

    /// What a new commit starts from: the draft a cancelled editor kept, else
    /// the `commit.template` file, else nothing.
    fn new_commit_prefill(&mut self) -> Option<String> {
        let template = || self.repo.as_ref()?.commit_template();
        self.commit_draft.take().or_else(template)
    }

    /// Open the editor to reword the older commit `hash` (a rebase, not an
    /// amend), pre-filled with its message.
    pub(crate) fn open_reword_editor(&mut self, hash: String, title: String, message: &str) {
        self.open_commit_editor(git::commit::CommitKind::Reword, Some(message));
        if let Some(Popup::Commit(draft)) = self.modal.popup_mut() {
            draft.reword = Some(RewordTarget { hash, title });
        }
    }

    fn open_commit_editor(&mut self, kind: git::commit::CommitKind, prefill: Option<&str>) {
        let draft = CommitDraft::new(kind, self.prefs.config.commit.sign_off, prefill);
        self.modal.open_popup(Popup::Commit(draft));
        self.render.commit.open();
    }

    /// Handle the explicit "commit all" choice shown when the index is empty.
    pub(crate) fn commit_all_confirm_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('y') => {
                let result = self
                    .repo
                    .as_ref()
                    .map(|repo| repo.stage_all(ApplyDir::Forward));
                match result {
                    Some(Ok(())) => {
                        self.modal.close_popup();
                        self.render.commit.close();
                        self.request_refresh();
                        let prefill = self.new_commit_prefill();
                        self.open_commit_editor(
                            git::commit::CommitKind::Normal,
                            prefill.as_deref(),
                        );
                    },
                    Some(Err(error)) => {
                        self.modal.close_popup();
                        self.render.commit.close();
                        self.report_error(error);
                    },
                    None => {},
                }
            },
            KeyCode::Char('n') | KeyCode::Esc => {
                self.modal.close_popup();
                self.render.commit.close();
            },
            _ => {},
        }
    }

    /// Route lazygit-style commit-editor keys while the editor owns input.
    pub(crate) fn commit_popup_key(&mut self, key: KeyEvent) {
        let Some(Popup::Commit(draft)) = self.modal.popup_mut() else {
            return;
        };
        match draft.on_key(key, &self.snapshot.commits) {
            DraftKey::Edited => {},
            DraftKey::CycleAuthor => self.cycle_author(),
            DraftKey::Commit => self.do_commit(),
            DraftKey::Cancel => {
                // A reword of an older commit is not a half-written new commit:
                // its text must not come back as the next `c`'s draft.
                if draft.reword.is_none() {
                    self.commit_draft = Some(draft.message());
                }
                self.modal.close_popup();
                self.render.commit.close();
            },
        }
    }

    /// Submit current editor content with `git commit`.
    pub(crate) fn do_commit(&mut self) {
        let Some(Popup::Commit(draft)) = self.modal.popup() else {
            return;
        };
        let message = draft.message();
        if !matches!(draft.kind, git::commit::CommitKind::Fixup { .. }) && draft.summary.is_blank()
        {
            self.report_error(AppError::EmptyCommitMessage);
            return;
        }
        if let Some(target) = &draft.reword {
            let hash = target.hash.clone();
            let Some(repo) = &self.repo else { return };
            let result = repo.rebase_edit(&hash, &git::rebase::RebaseEdit::Reword(message));
            // Like the new-branch popup: a refusal keeps the popup and the
            // text for a retry; a stop or success closes it.
            if let Err(error) = result {
                self.report_error(error);
                return;
            }
            self.modal.close_popup();
            self.render.commit.close();
            self.finish_operation(result);
            return;
        }
        let opts = draft.opts(self.authorship.author_arg());
        let kind = draft.kind.clone();
        let Some(repo) = &self.repo else { return };
        let result = repo.commit(&kind, &message, opts);

        match result {
            Ok(head) => {
                self.commit_draft = None;
                self.modal.close_popup();
                self.render.commit.close();
                // The commit just made tops the list, and is the row selected
                // once it shows up (lazygit).
                if self.nav.commit_drill.is_none() {
                    self.nav
                        .select_when_listed(Pane::Commits, SelectionKey::Commit(head));
                }
                self.request_refresh();
            },
            Err(git::error::GitError::NothingStaged) => {
                self.report_error(git::error::GitError::NothingStaged);
            },
            Err(error) => self.report_error(error),
        }
    }
}
