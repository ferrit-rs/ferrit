//! The commit editor: its state, its keys, making the commit, and how it is drawn.

use crate::git::apply::ApplyDir;
use crate::git::commit::{self, CommitKind, CommitOpts, OpenPlan, Submitted};
use crate::git::model::{CommitEntry, FileEntry};
use crate::git::port::GitPort;
use crate::tui::components::panes::nav::Pane;
use crate::tui::components::panes::selection::SelectionKey;
use crate::tui::components::popups::Popup;
use crate::tui::error::AppError;
use crate::tui::event::Event;
use crate::tui::widgets::text_input::{TextInput, TextInputMode};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub mod view;

/// What opening the editor needs to know of the app.
pub(crate) struct OpenCtx<'a> {
    pub(crate) repo: Option<&'a dyn GitPort>,
    pub(crate) files: &'a [FileEntry],
    pub(crate) commits: &'a [CommitEntry],
    pub(crate) sign_off: bool,
    /// The draft a cancelled editor kept, taken by a new commit.
    pub(crate) saved: &'a mut Option<String>,
}

/// `c` / `A` / `w`: open the commit editor. Amend / Reword pre-fill `HEAD`'s
/// message; a plain commit restores a cancelled draft.
pub(crate) fn open(kind: CommitKind, ctx: &mut OpenCtx<'_>) -> Vec<Event> {
    let Some(repo) = ctx.repo else {
        return Vec::new();
    };
    match commit::plan_open(&kind, ctx.files, ctx.commits.len()) {
        OpenPlan::StageAllFirst => vec![
            Event::OpenPopup(Popup::CommitAllConfirm),
            Event::CommitAnimation(true),
        ],
        OpenPlan::NoCommit => vec![Event::Report(AppError::NoCommitToAmend)],
        OpenPlan::Edit => {
            let prefill = commit::prefill(&kind, repo, ctx.saved);
            open_editor(kind, ctx.sign_off, prefill.as_deref())
        },
    }
}

fn open_editor(kind: CommitKind, sign_off: bool, prefill: Option<&str>) -> Vec<Event> {
    vec![
        Event::OpenPopup(Popup::Commit(CommitDraft::new(kind, sign_off, prefill))),
        Event::CommitAnimation(true),
    ]
}

/// Open the editor to reword the older commit `hash` (a rebase, not an amend),
/// pre-filled with its message.
pub(crate) fn open_reword(
    hash: String,
    title: String,
    message: &str,
    sign_off: bool,
) -> Vec<Event> {
    let mut draft = CommitDraft::new(CommitKind::Reword, sign_off, Some(message));
    draft.reword = Some(RewordTarget { hash, title });
    vec![
        Event::OpenPopup(Popup::Commit(draft)),
        Event::CommitAnimation(true),
    ]
}

/// The explicit "commit all" choice shown when the index is empty: `y` stages
/// everything and opens the editor, `n` and `Esc` leave.
pub(crate) fn all_confirm_key(key: KeyEvent, ctx: &mut OpenCtx<'_>) -> Vec<Event> {
    let close = || vec![Event::ClosePopup, Event::CommitAnimation(false)];
    match key.code {
        KeyCode::Char('y') => {
            let Some(repo) = ctx.repo else {
                return Vec::new();
            };
            match repo.stage_all(ApplyDir::Forward) {
                Ok(()) => {
                    let mut events = close();
                    events.push(Event::Refresh);
                    let prefill = commit::prefill(&CommitKind::Normal, repo, ctx.saved);
                    events.extend(open_editor(
                        CommitKind::Normal,
                        ctx.sign_off,
                        prefill.as_deref(),
                    ));
                    events
                },
                Err(error) => {
                    let mut events = close();
                    events.push(Event::Report(error.into()));
                    events
                },
            }
        },
        KeyCode::Char('n') | KeyCode::Esc => close(),
        _ => Vec::new(),
    }
}

/// What submitting the editor needs to know of the app.
pub(crate) struct SubmitCtx<'a> {
    pub(crate) repo: Option<&'a dyn GitPort>,
    pub(crate) commits: &'a [CommitEntry],
    /// `Name <email>` for `--author`, from ferrit's identity pick.
    pub(crate) author: Option<String>,
    /// The Commits pane is showing one commit's files, not the commit list.
    pub(crate) drilled: bool,
}

/// Route lazygit-style commit-editor keys while the editor owns input.
pub(crate) fn popup_key(draft: &mut CommitDraft, key: KeyEvent, ctx: &SubmitCtx<'_>) -> Vec<Event> {
    match draft.on_key(key, ctx.commits) {
        DraftKey::Edited => Vec::new(),
        DraftKey::CycleAuthor => vec![Event::CycleAuthor],
        DraftKey::Commit => submit(draft, ctx),
        DraftKey::Cancel => {
            let mut events = Vec::new();
            // A reword of an older commit is not a half-written new commit: its
            // text must not come back as the next `c`'s draft.
            if draft.reword.is_none() {
                events.push(Event::KeepCommitDraft(Some(draft.message())));
            }
            events.push(Event::ClosePopup);
            events.push(Event::CommitAnimation(false));
            events
        },
    }
}

/// Submit the editor's content with `git commit`.
fn submit(draft: &CommitDraft, ctx: &SubmitCtx<'_>) -> Vec<Event> {
    if draft.kind.needs_summary() && draft.summary.is_blank() {
        return vec![Event::Report(AppError::EmptyCommitMessage)];
    }
    let Some(repo) = ctx.repo else {
        return Vec::new();
    };
    let reword = draft.reword.as_ref().map(|target| target.hash.as_str());
    let opts = draft.opts(ctx.author.clone());
    match commit::submit(repo, &draft.kind, draft.message(), opts, reword) {
        // Like the new-branch popup: a refusal keeps the popup and the text for
        // a retry; a stop or a success closes it.
        Err(error) => vec![Event::Report(error.into())],
        Ok(Submitted::Reworded(outcome)) => vec![
            Event::ClosePopup,
            Event::CommitAnimation(false),
            Event::FinishOperation(Ok(outcome)),
        ],
        Ok(Submitted::Committed(head)) => {
            let mut events = vec![
                Event::KeepCommitDraft(None),
                Event::ClosePopup,
                Event::CommitAnimation(false),
            ];
            // The commit just made tops the list, and is the row selected once
            // it shows up (lazygit).
            if !ctx.drilled {
                events.push(Event::SelectWhenListed(
                    Pane::Commits,
                    SelectionKey::Commit(head),
                ));
            }
            events.push(Event::Refresh);
            events
        },
    }
}

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
