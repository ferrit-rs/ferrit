//! New-branch, upstream-input and note popup state / key handling.

use super::{
    App, CommandLogView, CommitPopupView, KeyCode, KeyEvent, Popup, PopupView, TextInputMode,
};

/// Rows a `PageUp` / `PageDown` moves the command log viewer.
const COMMAND_LOG_PAGE: usize = 10;

/// The viewer's scroll offset (rows up from the newest entry) after `key`.
/// `usize::MAX` means "as far up as it goes"; the renderer clamps it.
fn scrolled_command_log(from_bottom: usize, key: KeyCode) -> usize {
    match key {
        KeyCode::Char('k') | KeyCode::Up => from_bottom.saturating_add(1),
        KeyCode::Char('j') | KeyCode::Down => from_bottom.saturating_sub(1),
        KeyCode::PageUp => from_bottom.saturating_add(COMMAND_LOG_PAGE),
        KeyCode::PageDown => from_bottom.saturating_sub(COMMAND_LOG_PAGE),
        KeyCode::Home | KeyCode::Char('g') => usize::MAX,
        KeyCode::End | KeyCode::Char('G') => 0,
        _ => from_bottom,
    }
}

#[derive(Clone, Copy)]
enum PopupKind {
    Commit,
    CommitAllConfirm,
    NewBranch,
    Stash,
    CommandLog,
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
            Popup::CommandLog { .. } => PopupKind::CommandLog,
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
            PopupKind::CommandLog => self.command_log_popup().map(PopupView::CommandLog),
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

    /// The `@` viewer's render data: the whole ring, reads included.
    pub fn command_log_popup(&self) -> Option<CommandLogView> {
        let Some(Popup::CommandLog { from_bottom }) = &self.popup else {
            return None;
        };
        Some(CommandLogView {
            records: crate::domain::git::command_log::recent(usize::MAX, true),
            from_bottom: *from_bottom,
        })
    }

    /// `@`: open the command log viewer, scrolled to the newest entry.
    pub(super) fn open_command_log(&mut self) {
        if self.popup.is_none() {
            self.popup = Some(Popup::CommandLog { from_bottom: 0 });
        }
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
            // Both are routed to their own handlers above.
            Some(Popup::Commit(_) | Popup::CommitAllConfirm) => {},
            Some(Popup::NewBranch(buf)) => match key.code {
                KeyCode::Esc => dismiss = true,
                KeyCode::Enter => create_branch_now = true,
                _ => {
                    buf.handle_key_event(key, TextInputMode::SingleLine);
                },
            },
            Some(Popup::CommandLog { from_bottom }) => match key.code {
                KeyCode::Esc | KeyCode::Char('@' | 'q') => dismiss = true,
                other => *from_bottom = scrolled_command_log(*from_bottom, other),
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

#[cfg(test)]
mod tests {
    use super::{COMMAND_LOG_PAGE, KeyCode, scrolled_command_log};

    #[test]
    fn k_and_j_move_one_row_and_stop_at_the_newest() {
        assert_eq!(scrolled_command_log(3, KeyCode::Char('k')), 4);
        assert_eq!(scrolled_command_log(3, KeyCode::Char('j')), 2);
        assert_eq!(scrolled_command_log(0, KeyCode::Char('j')), 0);
    }

    #[test]
    fn pages_jump_and_the_ends_snap() {
        assert_eq!(scrolled_command_log(0, KeyCode::PageUp), COMMAND_LOG_PAGE);
        assert_eq!(scrolled_command_log(4, KeyCode::PageDown), 0);
        assert_eq!(scrolled_command_log(7, KeyCode::Char('g')), usize::MAX);
        assert_eq!(scrolled_command_log(7, KeyCode::Char('G')), 0);
    }

    #[test]
    fn scrolling_up_from_the_far_end_does_not_wrap() {
        assert_eq!(
            scrolled_command_log(usize::MAX, KeyCode::Char('k')),
            usize::MAX
        );
    }

    #[test]
    fn other_keys_leave_the_offset_alone() {
        assert_eq!(scrolled_command_log(5, KeyCode::Char('x')), 5);
    }
}
