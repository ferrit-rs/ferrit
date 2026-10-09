//! The commit editor: its state, its keys, making the commit, and how it is drawn.

use crate::git;
use crate::git::apply::ApplyDir;
use crate::git::commit::{self, CommitKind, CommitOpts, OpenPlan, Submitted};
use crate::git::model::CommitEntry;
use crate::theme::palette::Palette;
use crate::tui::components::diff::CommitPopupView;
use crate::tui::components::panes::{Pane, SelectionKey};
use crate::tui::components::popups::Popup;
use crate::tui::error::AppError;
use crate::tui::widgets::dialog::Dialog;
use crate::tui::widgets::key_bar::KeyBar;
use crate::tui::widgets::panel::Panel;
use crate::tui::widgets::text_input::{TextInput, TextInputMode};
use crate::tui::widgets::tui_overlay::anchor::Anchor;
use crate::tui::widgets::tui_overlay::backdrop::Backdrop;
use crate::tui::widgets::tui_overlay::overlay::Overlay;
use crate::tui::widgets::tui_overlay::state::OverlayState;
use crate::tui::{App, row_lines};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Paragraph};

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

/// Shared editor popup for commit messages and branch names.
pub(crate) fn draw_commit(
    frame: &mut Frame<'_>,
    area: Rect,
    view: &mut CommitPopupView<'_>,
    accent: ratatui::style::Color,
    palette: &Palette,
) {
    let width = (area.width * 2 / 3).clamp(40.min(area.width), area.width);
    if let Some(description) = view.description {
        let focused = Style::new().fg(accent).add_modifier(Modifier::BOLD);
        let idle = Style::new().fg(palette.idle);
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(focused)
            .title(Line::styled(format!(" {} ", view.title), focused));
        // The commit editor always carries its overlay state; without it
        // there is nothing to anchor, so draw nothing.
        let Some(overlay_state) = view.overlay_state.take() else {
            return;
        };
        frame.render_stateful_widget(
            Overlay::new()
                .anchor(Anchor::Center)
                .width(Constraint::Percentage(72))
                .height(Constraint::Percentage(68))
                .backdrop(
                    Backdrop::new(ratatui::style::Color::Black).fg(ratatui::style::Color::DarkGray),
                )
                .block(block),
            area,
            overlay_state,
        );
        let Some(inner) = overlay_state.inner_area() else {
            return;
        };
        // The sign-off / no-verify line only exists when the toggles do (not
        // for a rebase reword, which runs the amend itself).
        let footer_rows =
            if view.toggles.is_some() { 2 } else { 1 } + u16::from(view.author.is_some());
        let [body_area, footer_area] =
            Layout::vertical([Constraint::Min(1), Constraint::Length(footer_rows)]).areas(inner);
        let [summary_area, description_area] =
            Layout::vertical([Constraint::Length(3), Constraint::Min(5)]).areas(body_area);
        let summary_style = if view.summary_focused { focused } else { idle };
        let description_style = if view.summary_focused { idle } else { focused };
        let summary_block = Panel::new()
            .title(Line::styled(" Summary ", summary_style))
            .bottom_title(row_lines::subject_counter(
                palette,
                view.input.text().chars().count(),
            ))
            .border_style(summary_style)
            .block();
        let summary_inner = summary_block.inner(summary_area);
        frame.render_widget(summary_block, summary_area);
        if view.summary_focused {
            view.input.render(frame, summary_inner);
        } else {
            view.input.render_inactive(frame, summary_inner);
        }

        let description_block = Panel::new()
            .title(Line::styled(" Description ", description_style))
            .border_style(description_style)
            .block();
        let description_inner = description_block.inner(description_area);
        frame.render_widget(description_block, description_area);
        if view.summary_focused {
            description.render_inactive(frame, description_inner);
        } else {
            description.render(frame, description_inner);
        }

        let hints = KeyBar::hints(view.hints, palette).line();
        let footer = match view.toggles {
            Some((sign_off, no_verify)) => {
                let on_off = |enabled: bool| if enabled { "on" } else { "off" };
                let status = Line::from(vec![
                    Span::styled("sign-off: ", Style::new().fg(palette.idle)),
                    Span::styled(on_off(sign_off), Style::new().fg(palette.add)),
                    Span::raw("   "),
                    Span::styled("no-verify: ", Style::new().fg(palette.idle)),
                    Span::styled(on_off(no_verify), Style::new().fg(palette.del)),
                ]);
                let mut rows = vec![status];
                rows.extend(
                    view.author
                        .as_ref()
                        .map(|author| Line::styled(author.clone(), Style::new().fg(palette.idle))),
                );
                rows.push(hints);
                rows
            },
            None => vec![hints],
        };
        frame.render_widget(Paragraph::new(footer), footer_area);
        return;
    }

    let body_height = u16::try_from(view.input.lines().len().max(1)).unwrap_or(u16::MAX);
    let footer_height: u16 = if view.toggles.is_some() { 2 } else { 1 };
    let focused = Style::new().fg(accent).add_modifier(Modifier::BOLD);
    let dialog = Dialog::new(Line::styled(format!(" {} ", view.title), focused))
        .fit_content(width, body_height, footer_height)
        .border_style(focused)
        .render(frame, area);
    view.input.render(frame, dialog.body);

    let hints = KeyBar::hints(view.hints, palette).line();
    match view.toggles {
        Some((sign_off, no_verify)) => {
            let sign_off_span = if sign_off {
                Span::styled("on", Style::new().fg(palette.add))
            } else {
                Span::styled("off", Style::new().fg(palette.idle))
            };
            let verify_span = if no_verify {
                Span::styled(
                    "off",
                    Style::new().fg(palette.del).add_modifier(Modifier::BOLD),
                )
            } else {
                Span::styled("on", Style::new().fg(palette.add))
            };
            let status = Line::from(vec![
                Span::styled("sign-off: ", Style::new().fg(palette.idle)),
                sign_off_span,
                Span::raw("   "),
                Span::styled("verify: ", Style::new().fg(palette.idle)),
                verify_span,
            ]);
            frame.render_widget(Paragraph::new(vec![status, hints]), dialog.footer);
        },
        None => frame.render_widget(Paragraph::new(hints), dialog.footer),
    }
}

/// Confirm staging all worktree changes when `c` is pressed with an empty index.
pub(crate) fn draw_commit_all_confirm(
    frame: &mut Frame<'_>,
    area: Rect,
    state: &mut OverlayState,
    accent: ratatui::style::Color,
    palette: &Palette,
) {
    let focused = Style::new().fg(accent).add_modifier(Modifier::BOLD);
    let block = Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(focused)
        .title(Line::styled(" Commit all files? ", focused));
    frame.render_stateful_widget(
        Overlay::new()
            .anchor(Anchor::Center)
            .width(Constraint::Percentage(68))
            .height(Constraint::Percentage(34))
            .backdrop(
                Backdrop::new(ratatui::style::Color::Black).fg(ratatui::style::Color::DarkGray),
            )
            .block(block),
        area,
        state,
    );
    let Some(inner) = state.inner_area() else {
        return;
    };
    let [heading, question, footer] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(2),
        Constraint::Length(1),
    ])
    .areas(inner);
    frame.render_widget(
        Paragraph::new("No files staged")
            .style(Style::new().fg(palette.add).add_modifier(Modifier::BOLD))
            .alignment(Alignment::Center),
        heading,
    );
    frame.render_widget(
        Paragraph::new("You have not staged any files.\nCommit all files?")
            .alignment(Alignment::Center),
        question,
    );
    frame.render_widget(
        Paragraph::new(KeyBar::hints("Y: Yes, stage all | N / Esc: No", palette).line())
            .alignment(Alignment::Center),
        footer,
    );
}
