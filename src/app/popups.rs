//! New-branch, upstream-input and note popup state / key handling.

use super::{App, CommitPopupView, KeyCode, KeyEvent, Popup, PopupView, TextInputMode};

#[derive(Clone, Copy)]
enum PopupKind {
    Commit,
    CommitAllConfirm,
    NewBranch,
    Stash,
    Upstream,
    Note,
}

impl App {
    /// Read active popup as one enum for the renderer.
    pub fn popup_view(&mut self) -> Option<PopupView<'_>> {
        let kind = match self.popup.as_ref()? {
            Popup::Commit(_) => PopupKind::Commit,
            Popup::CommitAllConfirm => PopupKind::CommitAllConfirm,
            Popup::NewBranch(_) => PopupKind::NewBranch,
            Popup::Stash(_) => PopupKind::Stash,
            Popup::Upstream(_) => PopupKind::Upstream,
            Popup::Note(_) => PopupKind::Note,
        };
        match kind {
            PopupKind::Commit => self.commit_popup().map(PopupView::Commit),
            PopupKind::CommitAllConfirm => {
                Some(PopupView::CommitAllConfirm(&mut self.commit_overlay))
            },
            PopupKind::NewBranch => self.new_branch_popup().map(PopupView::NewBranch),
            PopupKind::Stash => self.stash_popup().map(PopupView::Stash),
            PopupKind::Upstream => self.upstream_popup().map(PopupView::Upstream),
            PopupKind::Note => self.note_popup().map(PopupView::Note),
        }
    }

    /// The pending discard / branch-delete confirmation message, for the
    /// keybar prompt (`ui::draw_keybar`), or `None` when nothing is
    /// pending.
    pub fn confirm_message(&self) -> Option<&str> {
        self.pending_confirm.as_ref().map(|p| p.message.as_str())
    }

    pub fn confirm_dialog_message(&self) -> Option<&str> {
        let prompt = self.pending_confirm.as_ref()?;
        matches!(prompt.action, super::ConfirmAction::SelectAuthor(_))
            .then_some(prompt.message.as_str())
    }

    /// The new-branch popup's render data, reusing `ui::draw_commit_popup`'s
    /// shape (`docs/PLAN_8_BRANCHES.md`), or `None` when it is not up.
    pub fn new_branch_popup(&self) -> Option<CommitPopupView<'_>> {
        let Some(Popup::NewBranch(buf)) = &self.popup else {
            return None;
        };
        Some(CommitPopupView {
            title: "New branch",
            input: buf,
            description: None,
            summary_focused: false,
            overlay_state: None,
            lines: buf.lines(),
            cursor: buf.cursor(),
            toggles: None,
            hints: "Create: Enter | Cancel: Esc",
        })
    }

    /// The stash popup's render data, same shape as the new-branch one.
    pub fn stash_popup(&self) -> Option<CommitPopupView<'_>> {
        let Some(Popup::Stash(buf)) = &self.popup else {
            return None;
        };
        Some(CommitPopupView {
            title: "Stash changes",
            input: buf,
            description: None,
            summary_focused: false,
            overlay_state: None,
            lines: buf.lines(),
            cursor: buf.cursor(),
            toggles: None,
            hints: "Stash: Enter | Cancel: Esc",
        })
    }

    /// A dismissible note's message (`ui::draw_note_popup`), or `None` when
    /// none is up.
    pub fn note_popup(&self) -> Option<&str> {
        match &self.popup {
            Some(Popup::Note(msg)) => Some(msg),
            _ => None,
        }
    }

    pub fn upstream_value(&self) -> Option<String> {
        match &self.popup {
            Some(Popup::Upstream(input)) => Some(input.text()),
            _ => None,
        }
    }

    pub fn upstream_popup(&self) -> Option<CommitPopupView<'_>> {
        let Some(Popup::Upstream(input)) = &self.popup else {
            return None;
        };
        Some(CommitPopupView {
            title: "Set upstream",
            input,
            description: None,
            summary_focused: false,
            overlay_state: None,
            lines: input.lines(),
            cursor: input.cursor(),
            toggles: None,
            hints: "Push: Enter | Cancel: Esc",
        })
    }

    /// Every key while a non-commit popup is up. Commit editor routes to
    /// `app::commit`, which owns its separate summary/body key model.
    pub(super) fn popup_key(&mut self, key: KeyEvent) {
        if matches!(self.popup, Some(Popup::Commit(_))) {
            self.commit_popup_key(key);
            return;
        }
        if matches!(self.popup, Some(Popup::CommitAllConfirm)) {
            self.commit_all_confirm_key(key);
            return;
        }
        let mut dismiss = false;
        let mut create_branch_now = false;
        let mut stash_now = false;
        let mut submit_upstream = None;

        match &mut self.popup {
            None => return,
            Some(Popup::Note(_)) => {
                if matches!(key.code, KeyCode::Esc | KeyCode::Enter) {
                    dismiss = true;
                }
            },
            Some(Popup::Commit(_)) => unreachable!("commit popup routed above"),
            Some(Popup::CommitAllConfirm) => unreachable!("commit confirmation routed above"),
            Some(Popup::NewBranch(buf)) => match key.code {
                KeyCode::Esc => dismiss = true,
                KeyCode::Enter => create_branch_now = true,
                _ => {
                    buf.handle_key_event(key, TextInputMode::SingleLine);
                },
            },
            Some(Popup::Stash(buf)) => match key.code {
                KeyCode::Esc => dismiss = true,
                KeyCode::Enter => stash_now = true,
                _ => {
                    buf.handle_key_event(key, TextInputMode::SingleLine);
                },
            },
            Some(Popup::Upstream(input)) => match key.code {
                KeyCode::Esc => dismiss = true,
                KeyCode::Enter => submit_upstream = Some(input.text()),
                _ => {
                    input.handle_key_event(key, TextInputMode::SingleLine);
                },
            },
        }

        if dismiss {
            self.popup = None;
        }
        if create_branch_now {
            self.do_create_branch();
        }
        if stash_now {
            self.do_stash_push();
        }
        if let Some(value) = submit_upstream {
            self.submit_upstream(&value);
        }
    }
}
