//! Commit editor state and actions, following lazygit's summary/description
//! editor: `c` opens it; Tab switches fields; Enter confirms summary.

use super::{
    App, AppError, ApplyDir, CommitPopupView, KeyCode, KeyEvent, KeyModifiers, Popup, TextInput,
    TextInputMode, git,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum CommitField {
    Summary,
    Description,
}

pub(super) struct CommitDraft {
    pub(super) summary: TextInput,
    pub(super) description: TextInput,
    pub(super) focus: CommitField,
    pub(super) kind: git::commit::CommitKind,
    pub(super) sign_off: bool,
    pub(super) no_verify: bool,
    history_index: Option<usize>,
    saved_summary: String,
}

impl CommitDraft {
    fn message(&self) -> String {
        let summary = self.summary.text();
        let description = self.description.text();
        if description.is_empty() {
            summary
        } else {
            format!("{summary}\n\n{description}")
        }
    }

    fn set_message(&mut self, message: &str) {
        let (summary, description) = message
            .split_once('\n')
            .map_or((message, ""), |(summary, rest)| {
                (summary, rest.trim_start_matches('\n'))
            });
        self.summary = TextInput::from_text(summary);
        self.description = TextInput::from_text(description);
    }
}

impl App {
    /// Commit popup view for rendering.
    pub fn commit_popup(&mut self) -> Option<CommitPopupView<'_>> {
        let Self {
            popup,
            commit_overlay,
            ..
        } = self;
        let Some(Popup::Commit(draft)) = popup else {
            return None;
        };
        let hints = match draft.focus {
            CommitField::Summary => {
                "Enter: commit | Tab: description | ↑/↓: history | Ctrl-O/N: options | Esc: cancel"
            },
            CommitField::Description => {
                "Enter: newline | Tab: summary | Meta/Ctrl-Enter: commit | Ctrl-O/N: options | Esc: cancel"
            },
        };
        Some(CommitPopupView {
            title: draft.kind.title(),
            input: &draft.summary,
            description: Some(&draft.description),
            summary_focused: draft.focus == CommitField::Summary,
            overlay_state: Some(commit_overlay),
            lines: draft.summary.lines(),
            cursor: draft.summary.cursor(),
            toggles: Some((draft.sign_off, draft.no_verify)),
            hints,
        })
    }

    /// `c` / `A` / `w`: open the commit editor. Amend / Reword pre-fill
    /// `HEAD`'s message; a plain commit restores a cancelled draft.
    pub(super) fn open_commit(&mut self, kind: git::commit::CommitKind) {
        if self.popup.is_some() {
            return;
        }
        if self.repo.is_none() {
            return;
        }
        match &kind {
            git::commit::CommitKind::Normal
                if !self
                    .files
                    .iter()
                    .any(|f| f.staged != git::status::Change::None) =>
            {
                self.popup = Some(Popup::CommitAllConfirm);
                self.commit_overlay.open();
                return;
            },
            git::commit::CommitKind::Amend | git::commit::CommitKind::Reword
                if self.commits.is_empty() =>
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
            _ => self.commit_draft.take(),
        };
        self.open_commit_editor(kind, prefill);
    }

    fn open_commit_editor(&mut self, kind: git::commit::CommitKind, prefill: Option<String>) {
        let mut draft = CommitDraft {
            summary: TextInput::default(),
            description: TextInput::default(),
            focus: CommitField::Summary,
            kind,
            sign_off: false,
            no_verify: false,
            history_index: None,
            saved_summary: String::new(),
        };
        if let Some(prefill) = prefill {
            draft.set_message(&prefill);
        }
        self.popup = Some(Popup::Commit(draft));
        self.commit_overlay.open();
    }

    /// Handle the explicit "commit all" choice shown when the index is empty.
    pub(super) fn commit_all_confirm_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Char('y') => {
                let result = self
                    .repo
                    .as_ref()
                    .map(|repo| repo.stage_all(ApplyDir::Forward));
                match result {
                    Some(Ok(())) => {
                        self.popup = None;
                        self.commit_overlay.close();
                        self.request_refresh();
                        let prefill = self.commit_draft.take();
                        self.open_commit_editor(git::commit::CommitKind::Normal, prefill);
                    },
                    Some(Err(error)) => {
                        self.popup = None;
                        self.commit_overlay.close();
                        self.report_error(error);
                    },
                    None => {},
                }
            },
            KeyCode::Char('n') | KeyCode::Esc => {
                self.popup = None;
                self.commit_overlay.close();
            },
            _ => {},
        }
    }

    /// Route lazygit-style commit-editor keys while the editor owns input.
    pub(super) fn commit_popup_key(&mut self, key: KeyEvent) {
        let Some(Popup::Commit(draft)) = &mut self.popup else {
            return;
        };
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let meta = key
            .modifiers
            .intersects(KeyModifiers::SUPER | KeyModifiers::ALT);
        let mut commit = false;
        let mut cancel = false;

        if draft.focus == CommitField::Summary && !ctrl && !meta {
            match key.code {
                KeyCode::Up => {
                    let idx = draft.history_index.map_or(0, |i| i.saturating_add(1));
                    if let Some(entry) = self.commits.get(idx) {
                        if draft.history_index.is_none() {
                            draft.saved_summary = draft.summary.text();
                        }
                        draft.history_index = Some(idx);
                        draft.summary = TextInput::from_text(&entry.summary);
                        return;
                    }
                },
                KeyCode::Down => {
                    if let Some(idx) = draft.history_index {
                        if idx == 0 {
                            draft.history_index = None;
                            draft.summary = TextInput::from_text(&draft.saved_summary);
                        } else if let Some(entry) = self.commits.get(idx - 1) {
                            draft.history_index = Some(idx - 1);
                            draft.summary = TextInput::from_text(&entry.summary);
                        }
                        return;
                    }
                },
                _ => {},
            }
        }

        match key.code {
            KeyCode::Esc => cancel = true,
            KeyCode::Tab | KeyCode::BackTab => {
                draft.focus = match draft.focus {
                    CommitField::Summary => CommitField::Description,
                    CommitField::Description => CommitField::Summary,
                };
            },
            KeyCode::Enter if ctrl || meta => commit = true,
            KeyCode::Char('s') if ctrl => commit = true,
            KeyCode::Char('o') if ctrl => draft.sign_off = !draft.sign_off,
            KeyCode::Char('n') if ctrl => draft.no_verify = !draft.no_verify,
            KeyCode::Enter if draft.focus == CommitField::Summary => commit = true,
            KeyCode::Enter => {
                draft
                    .description
                    .handle_key_event(key, TextInputMode::MultiLine);
            },
            _ => match draft.focus {
                CommitField::Summary => {
                    draft
                        .summary
                        .handle_key_event(key, TextInputMode::SingleLine);
                },
                CommitField::Description => {
                    draft
                        .description
                        .handle_key_event(key, TextInputMode::MultiLine);
                },
            },
        }

        if cancel {
            if let Some(Popup::Commit(draft)) = &self.popup {
                self.commit_draft = Some(draft.message());
            }
            self.popup = None;
            self.commit_overlay.close();
        } else if commit {
            self.do_commit();
        }
    }

    /// Submit current editor content with `git commit`.
    pub(super) fn do_commit(&mut self) {
        let Some(Popup::Commit(draft)) = &self.popup else {
            return;
        };
        let message = draft.message();
        if !matches!(draft.kind, git::commit::CommitKind::Fixup { .. }) && draft.summary.is_blank()
        {
            self.report_error(AppError::EmptyCommitMessage);
            return;
        }
        let opts = git::commit::CommitOpts {
            sign_off: draft.sign_off,
            no_verify: draft.no_verify,
        };
        let kind = draft.kind.clone();
        let Some(repo) = &self.repo else { return };
        let result = repo.commit(&kind, &message, opts);

        match result {
            Ok(_) => {
                self.commit_draft = None;
                self.popup = None;
                self.commit_overlay.close();
                self.request_refresh();
            },
            Err(git::error::GitError::NothingStaged) => {
                self.report_error(AppError::NothingStaged);
            },
            Err(error) => self.report_error(error),
        }
    }
}
