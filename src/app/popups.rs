//! Popup state readers and the commit / new-branch / remote-pick / note popup key handling.

use super::{
    App, CommitDraft, CommitPopupView, KeyCode, KeyEvent, KeyModifiers, Popup, TextBuffer, git,
};

impl App {
    /// The pending discard / branch-delete confirmation message, for the
    /// keybar prompt (`ui::draw_keybar`), or `None` when nothing is
    /// pending.
    pub fn confirm_message(&self) -> Option<&str> {
        self.pending_confirm.as_ref().map(|p| p.message.as_str())
    }

    /// The commit popup's render data (`ui::draw_commit_popup`), or `None`
    /// when it is not up.
    pub fn commit_popup(&self) -> Option<CommitPopupView<'_>> {
        let Some(Popup::Commit(draft)) = &self.popup else {
            return None;
        };
        Some(CommitPopupView {
            title: draft.kind.title(),
            lines: &draft.text.lines,
            cursor: (draft.text.row, draft.text.col),
            toggles: Some((draft.sign_off, draft.no_verify)),
            hints: "Commit: Ctrl-S | Sign-off: Ctrl-O | No-verify: Ctrl-N | Cancel: Esc",
        })
    }

    /// The new-branch popup's render data, reusing `ui::draw_commit_popup`'s
    /// shape (`docs/PLAN_8_BRANCHES.md`), or `None` when it is not up.
    pub fn new_branch_popup(&self) -> Option<CommitPopupView<'_>> {
        let Some(Popup::NewBranch(buf)) = &self.popup else {
            return None;
        };
        Some(CommitPopupView {
            title: "New branch",
            lines: &buf.lines,
            cursor: (buf.row, buf.col),
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
    pub fn remote_pick(&self) -> Option<(&[git::RemoteEntry], usize)> {
        match &self.popup {
            Some(Popup::RemotePick(pick)) => Some((&pick.remotes, pick.selected)),
            _ => None,
        }
    }

    /// `c` / `A` / `w`: open the commit popup. Amend / Reword pre-fill
    /// `HEAD`'s current message; a plain commit reuses `commit_draft` if an
    /// earlier `Esc` left one behind (lazygit's "draft survives a cancel").
    /// A no-op with a popup already up, without a repo, with nothing staged
    /// (`c`), or with no commit yet to amend/reword.
    pub(super) fn open_commit(&mut self, kind: git::CommitKind) {
        if self.popup.is_some() {
            return;
        }
        let Some(repo) = &self.repo else { return };
        match &kind {
            git::CommitKind::Normal
                if !self.files.iter().any(|f| f.staged != git::Change::None) =>
            {
                self.last_error = Some("nothing staged to commit".to_owned());
                return;
            },
            git::CommitKind::Amend | git::CommitKind::Reword if self.commits.is_empty() => {
                self.last_error = Some("no commit yet to amend".to_owned());
                return;
            },
            _ => {},
        }

        let prefill = match &kind {
            git::CommitKind::Amend | git::CommitKind::Reword => repo.head_message().ok().flatten(),
            _ => self.commit_draft.take(),
        };
        let text = prefill.map_or_else(TextBuffer::default, |s| TextBuffer::from_text(&s));
        self.popup = Some(Popup::Commit(CommitDraft {
            text,
            kind,
            sign_off: false,
            no_verify: false,
        }));
    }

    /// Every key while `self.popup` is `Some`: printable/editing keys go to
    /// the draft's `TextBuffer`, `Ctrl-S` commits, `Ctrl-O` / `Ctrl-N` flip
    /// the sign-off / no-verify toggles, `Esc` cancels (keeping the draft
    /// for a commit popup, dropping it outright for a new-branch one — a
    /// few retyped characters cost nothing) or dismisses a note. `Enter`
    /// *submits* the new-branch popup rather than inserting a newline, the
    /// one behavioural difference from reusing `TextBuffer` as-is.
    pub(super) fn popup_key(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let mut dismiss = false;
        let mut cancel = false;
        let mut commit_now = false;
        let mut create_branch_now = false;
        let mut pick_remote_now = false;

        match &mut self.popup {
            None => return,
            Some(Popup::Note(_)) => {
                if matches!(key.code, KeyCode::Esc | KeyCode::Enter) {
                    dismiss = true;
                }
            },
            Some(Popup::Commit(draft)) => match key.code {
                KeyCode::Char('s') if ctrl => commit_now = true,
                KeyCode::Char('o') if ctrl => draft.sign_off = !draft.sign_off,
                KeyCode::Char('n') if ctrl => draft.no_verify = !draft.no_verify,
                KeyCode::Esc => cancel = true,
                KeyCode::Enter => draft.text.insert_newline(),
                KeyCode::Backspace => draft.text.backspace(),
                KeyCode::Left => draft.text.move_left(),
                KeyCode::Right => draft.text.move_right(),
                KeyCode::Up => draft.text.move_up(),
                KeyCode::Down => draft.text.move_down(),
                KeyCode::Char(c) if !ctrl => draft.text.insert_char(c),
                _ => {},
            },
            Some(Popup::NewBranch(buf)) => match key.code {
                KeyCode::Esc => dismiss = true,
                KeyCode::Enter => create_branch_now = true,
                KeyCode::Backspace => buf.backspace(),
                KeyCode::Left => buf.move_left(),
                KeyCode::Right => buf.move_right(),
                KeyCode::Char(c) if !ctrl => buf.insert_char(c),
                _ => {},
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
        if cancel {
            if let Some(Popup::Commit(draft)) = &self.popup {
                self.commit_draft = Some(draft.text.text());
            }
            self.popup = None;
        }
        if commit_now {
            self.do_commit();
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

    /// `Ctrl-S` in the commit popup: run `Repo::commit`, then either close
    /// the popup and refresh (phase 2's `refresh()` picks up the new
    /// `HEAD`, phase 3's `update_right_pane` sees the now-empty staged diff)
    /// or swap the popup for a dismissible `Note` on failure, keeping the
    /// draft either way except on success.
    pub(super) fn do_commit(&mut self) {
        let Some(Popup::Commit(draft)) = &self.popup else {
            return;
        };
        if !matches!(draft.kind, git::CommitKind::Fixup { .. }) && draft.text.is_blank() {
            self.popup = Some(Popup::Note("empty commit message".to_owned()));
            return;
        }
        let message = draft.text.text();
        let opts = git::CommitOpts {
            sign_off: draft.sign_off,
            no_verify: draft.no_verify,
        };
        let kind = draft.kind.clone();
        let Some(repo) = &self.repo else { return };
        let result = repo.commit(&kind, &message, opts);

        match result {
            Ok(_hash) => {
                self.commit_draft = None;
                self.popup = None;
                self.request_refresh();
            },
            Err(git::GitError::NothingStaged) => {
                self.popup = Some(Popup::Note("nothing staged to commit".to_owned()));
            },
            Err(e) => {
                self.popup = Some(Popup::Note(e.to_string()));
            },
        }
    }
}
