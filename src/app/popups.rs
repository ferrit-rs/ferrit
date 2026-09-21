//! New-branch, remote-pick and note popup state / key handling.

use super::{App, CommitPopupView, KeyCode, KeyEvent, Popup, PopupView, TextInputMode, git};

#[derive(Clone, Copy)]
enum PopupKind {
    Commit,
    CommitAllConfirm,
    NewBranch,
    RemotePick,
    Note,
}

impl App {
    /// Read active popup as one enum for the renderer.
    pub fn popup_view(&mut self) -> Option<PopupView<'_>> {
        let kind = match self.popup.as_ref()? {
            Popup::Commit(_) => PopupKind::Commit,
            Popup::CommitAllConfirm => PopupKind::CommitAllConfirm,
            Popup::NewBranch(_) => PopupKind::NewBranch,
            Popup::RemotePick(_) => PopupKind::RemotePick,
            Popup::Note(_) => PopupKind::Note,
        };
        match kind {
            PopupKind::Commit => self.commit_popup().map(PopupView::Commit),
            PopupKind::CommitAllConfirm => {
                Some(PopupView::CommitAllConfirm(&mut self.commit_overlay))
            },
            PopupKind::NewBranch => self.new_branch_popup().map(PopupView::NewBranch),
            PopupKind::RemotePick => self
                .remote_pick()
                .map(|(remotes, selected)| PopupView::RemotePick(remotes, selected)),
            PopupKind::Note => self.note_popup().map(PopupView::Note),
        }
    }

    /// The pending discard / branch-delete confirmation message, for the
    /// keybar prompt (`ui::draw_keybar`), or `None` when nothing is
    /// pending.
    pub fn confirm_message(&self) -> Option<&str> {
        self.pending_confirm.as_ref().map(|p| p.message.as_str())
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

    /// A dismissible note's message (`ui::draw_note_popup`), or `None` when
    /// none is up.
    pub fn note_popup(&self) -> Option<&str> {
        match &self.popup {
            Some(Popup::Note(msg)) => Some(msg),
            _ => None,
        }
    }

    /// The remote-pick popup's remotes and highlighted index
    /// (`docs/PLAN_9_REMOTE.md`'s "No upstream" flow, 2+ remotes), or
    /// `None` when it is not up.
    pub fn remote_pick(&self) -> Option<(&[git::remote::RemoteEntry], usize)> {
        match &self.popup {
            Some(Popup::RemotePick(pick)) => Some((&pick.remotes, pick.selected)),
            _ => None,
        }
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
        let mut pick_remote_now = false;

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
            Some(Popup::RemotePick(pick)) => match key.code {
                KeyCode::Esc => dismiss = true,
                KeyCode::Enter => pick_remote_now = true,
                KeyCode::Char('j') | KeyCode::Down => {
                    pick.selected = (pick.selected + 1).min(pick.remotes.len().saturating_sub(1));
                },
                KeyCode::Char('k') | KeyCode::Up => pick.selected = pick.selected.saturating_sub(1),
                _ => {},
            },
        }

        if dismiss {
            self.popup = None;
        }
        if create_branch_now {
            self.do_create_branch();
        }
        if pick_remote_now {
            if let Some(Popup::RemotePick(pick)) = &self.popup {
                let name = pick.remotes.get(pick.selected).map(|r| r.name.clone());
                self.popup = None;
                if let Some(name) = name {
                    self.push_with_upstream(name);
                }
            }
        }
    }
}
