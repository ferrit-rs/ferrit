//! The commit editor, following lazygit's summary / description editor: `c`
//! opens it, Tab switches fields, Enter confirms the summary. This is its state
//! and its keys; making the commit is `git::actions::commit`.

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::git::commit::{CommitKind, CommitOpts};
use crate::git::model::CommitEntry;
use crate::interface::components::tui_overlay::state::OverlayState;
use crate::interface::components::ui::text_input::{TextInput, TextInputMode};
use crate::interface::panes::views::CommitPopupView;

/// Which of the two fields has the keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CommitField {
    Summary,
    Description,
}

/// The older commit a reword popup will rewrite with `git rebase -i`, instead
/// of amending `HEAD`.
pub(crate) struct RewordTarget {
    pub(crate) hash: String,
    pub(crate) title: String,
}

/// What a key did to the editor, for the app to act on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DraftKey {
    /// The text, the focus or a toggle changed (or nothing did).
    Edited,
    /// Make the commit.
    Commit,
    /// Close the editor.
    Cancel,
    /// Pick the next identity.
    CycleAuthor,
}

pub(crate) struct CommitDraft {
    pub(crate) summary: TextInput,
    pub(crate) description: TextInput,
    pub(crate) focus: CommitField,
    pub(crate) kind: CommitKind,
    /// `Some` when rewording a commit other than `HEAD`.
    pub(crate) reword: Option<RewordTarget>,
    pub(crate) sign_off: bool,
    pub(crate) no_verify: bool,
    history_index: Option<usize>,
    saved_summary: String,
}

impl CommitDraft {
    /// An editor for `kind`, signing off when the configuration says so,
    /// starting from `prefill` (`summary`, a blank line, then the description).
    pub(crate) fn new(kind: CommitKind, sign_off: bool, prefill: Option<&str>) -> Self {
        let mut draft = Self {
            summary: TextInput::default(),
            description: TextInput::default(),
            focus: CommitField::Summary,
            kind,
            reword: None,
            sign_off,
            no_verify: false,
            history_index: None,
            saved_summary: String::new(),
        };
        if let Some(prefill) = prefill {
            draft.set_message(prefill);
        }
        draft
    }

    /// The whole message: the summary, then the description after a blank line.
    pub(crate) fn message(&self) -> String {
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

    /// The toggles as `git commit` options, authored by `author` when set.
    pub(crate) fn opts(&self, author: Option<String>) -> CommitOpts {
        CommitOpts {
            sign_off: self.sign_off,
            no_verify: self.no_verify,
            author,
        }
    }

    /// The editor as data for the screen; `author` is the identity line, shown
    /// only for a commit that is not a rebase reword.
    pub(crate) fn view<'a>(
        &'a self,
        author: String,
        overlay: Option<&'a mut OverlayState>,
    ) -> CommitPopupView<'a> {
        let hints = match (self.reword.is_some(), self.focus) {
            (false, CommitField::Summary) => {
                "Enter: commit | Tab: description | ↑/↓: history | Ctrl-O/N/A: options | Esc: cancel"
            },
            (false, CommitField::Description) => {
                "Enter: newline | Tab: summary | Meta/Ctrl-Enter: commit | Ctrl-O/N/A: options | Esc: cancel"
            },
            // A rebase reword has no sign-off / no-verify to toggle.
            (true, CommitField::Summary) => {
                "Enter: reword | Tab: description | ↑/↓: history | Esc: cancel"
            },
            (true, CommitField::Description) => {
                "Enter: newline | Tab: summary | Meta/Ctrl-Enter: reword | Esc: cancel"
            },
        };
        CommitPopupView {
            title: self
                .reword
                .as_ref()
                .map_or_else(|| self.kind.title(), |target| target.title.as_str()),
            input: &self.summary,
            description: Some(&self.description),
            summary_focused: self.focus == CommitField::Summary,
            overlay_state: overlay,
            lines: self.summary.lines(),
            cursor: self.summary.cursor(),
            // A rebase reword runs the amend itself: neither toggle applies.
            toggles: self
                .reword
                .is_none()
                .then_some((self.sign_off, self.no_verify)),
            author: self.reword.is_none().then_some(author),
            hints,
        }
    }

    /// Up / Down in the summary walk the subjects of recent commits (newest
    /// first), and Down past the newest restores what was typed.
    fn walk_history(&mut self, key: KeyCode, commits: &[CommitEntry]) -> bool {
        match key {
            KeyCode::Up => {
                let idx = self.history_index.map_or(0, |i| i.saturating_add(1));
                let Some(entry) = commits.get(idx) else {
                    return false;
                };
                if self.history_index.is_none() {
                    self.saved_summary = self.summary.text();
                }
                self.history_index = Some(idx);
                self.summary = TextInput::from_text(&entry.summary);
                true
            },
            KeyCode::Down => {
                let Some(idx) = self.history_index else {
                    return false;
                };
                if idx == 0 {
                    self.history_index = None;
                    self.summary = TextInput::from_text(&self.saved_summary);
                } else if let Some(entry) = commits.get(idx - 1) {
                    self.history_index = Some(idx - 1);
                    self.summary = TextInput::from_text(&entry.summary);
                }
                true
            },
            _ => false,
        }
    }

    /// Route one key while the editor owns input; `commits` is the history Up
    /// and Down walk.
    pub(crate) fn on_key(&mut self, key: KeyEvent, commits: &[CommitEntry]) -> DraftKey {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let meta = key
            .modifiers
            .intersects(KeyModifiers::SUPER | KeyModifiers::ALT);
        if self.focus == CommitField::Summary
            && !ctrl
            && !meta
            && self.walk_history(key.code, commits)
        {
            return DraftKey::Edited;
        }
        let rewording = self.reword.is_some();
        match key.code {
            KeyCode::Esc => return DraftKey::Cancel,
            KeyCode::Tab | KeyCode::BackTab => {
                self.focus = match self.focus {
                    CommitField::Summary => CommitField::Description,
                    CommitField::Description => CommitField::Summary,
                };
            },
            KeyCode::Enter if ctrl || meta => return DraftKey::Commit,
            KeyCode::Char('s') if ctrl => return DraftKey::Commit,
            KeyCode::Char('o') if ctrl && !rewording => self.sign_off = !self.sign_off,
            KeyCode::Char('n') if ctrl && !rewording => self.no_verify = !self.no_verify,
            KeyCode::Char('a') if ctrl && !rewording => return DraftKey::CycleAuthor,
            KeyCode::Enter if self.focus == CommitField::Summary => return DraftKey::Commit,
            KeyCode::Enter => {
                self.description
                    .handle_key_event(key, TextInputMode::MultiLine);
            },
            _ => match self.focus {
                CommitField::Summary => {
                    self.summary
                        .handle_key_event(key, TextInputMode::SingleLine);
                },
                CommitField::Description => {
                    self.description
                        .handle_key_event(key, TextInputMode::MultiLine);
                },
            },
        }
        DraftKey::Edited
    }
}
