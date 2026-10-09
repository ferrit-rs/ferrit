//! What the keys do in `App` for `commit`: the glue between the interface, the git code and the app's state.

use crate::app::App;
use crate::app::error::AppError;
use crate::git;
use crate::git::apply::ApplyDir;
use crate::git::commit::{self, CommitKind, OpenPlan, Submitted};
use crate::interface::components::tui_overlay::state::OverlayState;
use crate::interface::state::commit_draft::{CommitDraft, DraftKey, RewordTarget};
use crate::interface::state::pane::Pane;
use crate::interface::state::popup::Popup;
use crate::interface::state::selection::SelectionKey;
use crate::interface::state::views::CommitPopupView;
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
            .map(|(name, email)| git::identity::Identity {
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
    pub(crate) fn open_commit(&mut self, kind: CommitKind) {
        if self.modal.popup().is_some() {
            return;
        }
        let Some(repo) = &self.repo else { return };
        match commit::plan_open(&kind, &self.snapshot.files, self.snapshot.commits.len()) {
            OpenPlan::StageAllFirst => {
                self.modal.open_popup(Popup::CommitAllConfirm);
                self.render.commit.open();
            },
            OpenPlan::NoCommit => self.report_error(AppError::NoCommitToAmend),
            OpenPlan::Edit => {
                let prefill = commit::prefill(&kind, repo.as_ref(), &mut self.commit_draft);
                self.open_commit_editor(kind, prefill.as_deref());
            },
        }
    }

    /// What a new commit starts from.
    fn new_commit_prefill(&mut self) -> Option<String> {
        let repo = self.repo.as_ref()?;
        commit::prefill(&CommitKind::Normal, repo.as_ref(), &mut self.commit_draft)
    }

    /// Open the editor to reword the older commit `hash` (a rebase, not an
    /// amend), pre-filled with its message.
    pub(crate) fn open_reword_editor(&mut self, hash: String, title: String, message: &str) {
        self.open_commit_editor(CommitKind::Reword, Some(message));
        if let Some(Popup::Commit(draft)) = self.modal.popup_mut() {
            draft.reword = Some(RewordTarget { hash, title });
        }
    }

    fn open_commit_editor(&mut self, kind: CommitKind, prefill: Option<&str>) {
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
                        self.open_commit_editor(CommitKind::Normal, prefill.as_deref());
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
        if draft.kind.needs_summary() && draft.summary.is_blank() {
            self.report_error(AppError::EmptyCommitMessage);
            return;
        }
        let Some(repo) = &self.repo else { return };
        let reword = draft.reword.as_ref().map(|target| target.hash.as_str());
        let opts = draft.opts(self.authorship.author_arg());
        let result = commit::submit(repo.as_ref(), &draft.kind, draft.message(), opts, reword);
        match result {
            // Like the new-branch popup: a refusal keeps the popup and the text
            // for a retry; a stop or a success closes it.
            Err(error) => self.report_error(error),
            Ok(Submitted::Reworded(outcome)) => {
                self.modal.close_popup();
                self.render.commit.close();
                self.finish_operation(Ok(outcome));
            },
            Ok(Submitted::Committed(head)) => {
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
        }
    }
}
