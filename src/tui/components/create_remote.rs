//! Creating the GitHub repository: the form, `gh`, what follows.

use crate::git::create_remote::CreateRemote;
use crate::git::error::GitError;
use crate::git::host::{
    self, CreateDraft, CreateRequest, GhProgram, GhStatus, HostError, Visibility, parse_target,
    sanitize_name, ssh_remote_url, validate_description,
};
use crate::git::remote::RemoteOp;
use crate::git::remote::RemoteRequest;
use crate::theme::palette::Palette;
use crate::tui::App;
use crate::tui::components::popups::Popup;
use crate::tui::error::AppError;
use crate::tui::event::Event;
use crate::tui::events::AppEvent;
use crate::tui::widgets::dialog::Dialog;
use crate::tui::widgets::key_bar::KeyBar;
use crate::tui::widgets::panel::Panel;
use crate::tui::widgets::text_input::{TextInput, TextInputMode};
use crate::tui::workers::{WorkerKind, run_worker};
use color_eyre::Result;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use std::path::PathBuf;
use std::sync::{Arc, mpsc};
use std::thread;

impl App {
    /// The creation's state, for the popups and for tests.
    pub fn create_remote(&self) -> &CreateRemote {
        &self.create_remote
    }

    /// Read the ssh host aliases from this file instead of `~/.ssh/config`.
    /// Integration-test seam: a test must not depend on the user's own config.
    #[doc(hidden)]
    pub fn set_ssh_config_path(&mut self, path: PathBuf) {
        self.create_remote.ssh_config = Some(path);
    }

    /// Run this program for `gh` (a fake script in a test). Integration-test
    /// seam: the crate forbids `unsafe`, so the program cannot come from an
    /// environment variable set in-process.
    #[doc(hidden)]
    pub fn set_gh_program(&mut self, gh: GhProgram) {
        self.create_remote.gh = gh;
    }

    /// The `x` menu's "Create a repository on GitHub": ask `gh` whether it is
    /// ready (off the UI thread, it reaches the network), then show the form.
    /// Refused with a note when the repository already has a remote or another
    /// network operation runs.
    #[doc(hidden)]
    pub fn open_create_remote(&mut self) {
        if self.modal.is_some() || self.repo.is_none() {
            return;
        }
        if !self.snapshot.remotes.is_empty() {
            self.modal.open_popup(Popup::Note(
                "this repository already has a remote".to_owned(),
            ));
            return;
        }
        if self.workers.remote_busy.is_some() {
            self.modal.open_popup(Popup::Note(
                "another network operation is running".to_owned(),
            ));
            return;
        }
        self.create_remote.generation += 1;
        let generation = self.create_remote.generation;
        self.modal
            .open_popup(Popup::CreateRemote(Step::Checking { generation }));
        let gh = self.create_remote.gh.clone();
        let Some(sender) = self.workers.sender.clone() else {
            // No event loop (`App::mock`, a test without `run()`): ask now.
            let status = host::gh_status(&gh);
            self.on_gh_checked(generation, status);
            return;
        };
        thread::spawn(move || {
            let status = run_worker(WorkerKind::Remote, || host::gh_status(&gh))
                .unwrap_or(GhStatus::Missing);
            let _ = sender.send(AppEvent::GhChecked { generation, status });
        });
    }

    /// `AppEvent::GhChecked` arrived. Only the check the popup is waiting for
    /// counts; `gh` ready opens the form, anything else says what to do.
    pub(crate) fn on_gh_checked(&mut self, generation: u64, status: GhStatus) {
        let waiting = matches!(
            self.modal.popup(),
            Some(Popup::CreateRemote(Step::Checking { generation: g })) if *g == generation
        );
        if !waiting {
            return;
        }
        self.modal.open_popup(match status {
            GhStatus::Ready => {
                let draft = self
                    .create_remote
                    .draft
                    .clone()
                    .unwrap_or_else(|| CreateDraft::new(sanitize_name(&self.repo_name)));
                Popup::CreateRemote(Step::Form(Form::from_draft(&draft, None)))
            },
            GhStatus::Missing => Popup::Note(
                "gh is required: https://cli.github.com. Install it, then try again.".to_owned(),
            ),
            GhStatus::SignedOut => Popup::Note(
                "gh is not signed in: run `gh auth login` in a shell, then try again.".to_owned(),
            ),
        });
    }

    /// The form reopened on the name after a refusal, every field as typed.
    fn reopen_form(&mut self, error: String) {
        let Some(draft) = &self.create_remote.draft else {
            return;
        };
        if self.modal.popup().is_none() {
            self.modal
                .open_popup(Popup::CreateRemote(Step::Form(Form::from_draft(
                    draft,
                    Some(error),
                ))));
        }
    }

    /// The repository has no commit yet, asked of git itself: the app's own list
    /// of commits can be a refresh behind, right after a first commit.
    fn repo_has_no_commit(&self) -> bool {
        self.repo.as_ref().is_some_and(|repo| !repo.has_commits())
    }

    /// What the renderer draws, or `None` when this popup is not up.
    pub fn create_remote_view(&self) -> Option<CreateRemoteView<'_>> {
        let Some(Popup::CreateRemote(step)) = self.modal.popup() else {
            return None;
        };
        let aliases = self.create_remote.ssh_aliases();
        Some(step.view(&Consequences {
            first_commit: self.repo_has_no_commit(),
            ssh_host: aliases.first().map(String::as_str),
            branch: &self.snapshot.header.branch,
        }))
    }

    /// Every key while the popup is up.
    pub(crate) fn create_remote_key(&mut self, key: KeyEvent) {
        let Some(Popup::CreateRemote(step)) = self.modal.popup_mut() else {
            return;
        };
        match step.key(key) {
            FormKey::None => {},
            FormKey::Close => self.close_create_remote(),
            FormKey::Continue => {
                let Some(Popup::CreateRemote(Step::Form(form))) = self.modal.take_popup() else {
                    return;
                };
                match form.validate() {
                    Ok(_) => self
                        .modal
                        .open_popup(Popup::CreateRemote(Step::Confirm(form))),
                    Err(reason) => {
                        self.modal.open_popup(Popup::CreateRemote(Step::Form(
                            form.with_error(reason.to_string()),
                        )));
                    },
                }
            },
            FormKey::Back => {
                if let Some(Popup::CreateRemote(Step::Confirm(form))) = self.modal.take_popup() {
                    self.modal.open_popup(Popup::CreateRemote(Step::Form(form)));
                }
            },
            FormKey::Create => {
                let Some(Popup::CreateRemote(Step::Confirm(form))) = self.modal.take_popup() else {
                    return;
                };
                let Some(sender) = self.workers.sender.clone() else {
                    return;
                };
                self.start_create_remote(form.draft(), sender);
            },
        }
    }

    /// `Esc`: nothing was created. The typed fields are kept for next time; a
    /// check still running is abandoned.
    fn close_create_remote(&mut self) {
        match self.modal.take_popup() {
            Some(Popup::CreateRemote(Step::Form(form) | Step::Confirm(form))) => {
                self.create_remote.draft = Some(form.draft());
            },
            Some(Popup::CreateRemote(Step::Checking { .. })) => self.create_remote.generation += 1,
            Some(other) => self.modal.open_popup(other),
            None => {},
        }
    }

    /// Start `gh repo create` for `draft` in the background. One network
    /// operation at a time: ignored while another runs. A target that fails
    /// validation is reported at once and nothing starts. Takes `sender` like
    /// `start_remote_op`, so a test can drive the answer itself.
    #[doc(hidden)]
    pub fn start_create_remote(&mut self, draft: CreateDraft, sender: mpsc::Sender<AppEvent>) {
        if self.workers.remote_busy.is_some() {
            return;
        }
        let (owner, name) = match parse_target(&draft.target) {
            Ok(target) => target,
            Err(error) => {
                self.create_remote.error = Some(error.to_string());
                self.create_remote.draft = Some(draft);
                return;
            },
        };
        let Some(repo) = self.repo_handle() else {
            return;
        };
        let request = CreateRequest {
            owner,
            name,
            visibility: draft.visibility,
            description: draft.description.clone(),
        };
        let gh = self.create_remote.gh.clone();
        // The user's own alias, when they have one: the key it names is the one
        // that unlocks the push. None keeps the URL `gh` writes.
        self.create_remote.ssh_host = self
            .create_remote
            .ssh_aliases()
            .into_iter()
            .next()
            .unwrap_or_default();
        let author = self.authorship.author_arg();
        self.create_remote.draft = Some(draft);
        self.create_remote.error = None;
        self.workers.begin_remote(RemoteOp::Create);
        self.status_note = None;
        let cancel = Arc::clone(&self.workers.remote_cancel);
        self.workers.remote_worker = Some(thread::spawn(move || {
            let result = run_worker(WorkerKind::Remote, || {
                // The first commit comes first, and is local: if it fails
                // nothing has been created anywhere. A repository that already
                // has a commit is left as it is.
                repo.initial_commit(author).map_err(|error| {
                    GitError::HostFailed(format!(
                        "the first commit failed, nothing was created: {error}"
                    ))
                })?;
                repo.create_repo(&gh, &request, &cancel)
            })
            .map_err(AppError::from)
            .and_then(|result| result.map_err(AppError::from))
            .map(|created| created.web_url);
            let _ = sender.send(AppEvent::RemoteCreated(result));
        }));
    }

    /// `AppEvent::RemoteCreated` arrived. The slot is freed and the panes
    /// refresh (`gh` added `origin`). A success keeps the web URL and drops the
    /// draft, then pushes; a refusal keeps the draft and says
    /// why, nothing was configured.
    pub fn on_remote_created(&mut self, result: Result<String, AppError>) {
        self.workers.end_remote();
        self.request_refresh();
        match result {
            Ok(url) => {
                self.create_remote.draft = None;
                let ssh_host = std::mem::take(&mut self.create_remote.ssh_host);
                self.create_remote.error = None;
                self.last_error = None;
                self.status_note = Some(format!("Created {url}"));
                self.create_remote.web_url = Some(url.clone());
                self.after_creation(&url, &ssh_host);
            },
            Err(error) => {
                let message = error.to_string();
                self.create_remote.error = Some(message.clone());
                self.status_note = None;
                self.reopen_form(message);
                self.report_error(AppError::Background(Arc::new(error)));
            },
        }
    }

    /// What follows a creation: `origin` rewritten over the user's SSH alias (so
    /// the push reaches the key that alias names, and ferrit's passphrase popup
    /// answers for it, as for `P`), then the push of the current branch through
    /// the same path as `P`, or a note saying why not (a detached `HEAD`). The repository and
    /// `origin` stay whatever happens.
    fn after_creation(&mut self, url: &str, ssh_host: &str) {
        let mut created = format!("Created {url}.");
        if !ssh_host.is_empty() {
            let Some(remote) = ssh_remote_url(ssh_host, url) else {
                self.status_note = Some(format!(
                    "{created} Its owner and name could not be read from that URL: origin keeps \
                     the URL gh wrote."
                ));
                return;
            };
            let set = self
                .repo
                .as_ref()
                .map(|repo| repo.set_remote_url("origin", &remote));
            if let Some(Err(error)) = set {
                self.status_note = Some(format!(
                    "{created} {error}. origin keeps the URL gh wrote; fix it with git remote \
                     set-url, then P pushes."
                ));
                return;
            }
            created = format!("{created} origin is {remote}.");
        }
        if self.snapshot.header.detached {
            self.status_note = Some(format!("{created} HEAD is detached: nothing to push."));
        } else {
            // The push's own progress takes the status line from here.
            let branch = self.snapshot.header.branch.clone();
            self.apply(vec![Event::StartRemote(RemoteRequest::push_to(
                "origin".to_owned(),
                branch,
            ))]);
            // Only a push that really started is the one to explain if it fails.
            self.create_remote.pushing_after = self.workers.remote_busy.is_some();
        }
    }

    /// A push finished well: it was no longer "the one after a creation".
    pub(crate) fn create_remote_push_done(&mut self) {
        self.create_remote.pushing_after = false;
    }

    /// The failure of the push that followed a creation says the repository
    /// exists and how to retry; any other failure is left as it is.
    pub(crate) fn explain_push_after_creation(&mut self, failure: AppError) -> AppError {
        if !std::mem::take(&mut self.create_remote.pushing_after) {
            return failure;
        }
        match &self.create_remote.web_url {
            Some(url) => AppError::PushAfterCreation {
                source: Arc::new(failure),
                url: url.clone(),
            },
            None => failure,
        }
    }
}

/// The field of the form that has the focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Name,
    Visibility,
    Description,
}

impl Field {
    const ORDER: [Self; 3] = [Self::Name, Self::Visibility, Self::Description];

    fn step(self, forward: bool) -> Self {
        let at = Self::ORDER.iter().position(|f| *f == self).unwrap_or(0);
        let next = if forward {
            (at + 1) % Self::ORDER.len()
        } else {
            (at + Self::ORDER.len() - 1) % Self::ORDER.len()
        };
        Self::ORDER.get(next).copied().unwrap_or(Self::Name)
    }
}

/// The form's fields as they are being typed.
#[derive(Debug)]
pub(crate) struct Form {
    name: TextInput,
    description: TextInput,
    pub(crate) visibility: Visibility,
    focus: Field,
    pub(crate) error: Option<String>,
}

impl Form {
    pub(crate) fn from_draft(draft: &CreateDraft, error: Option<String>) -> Self {
        Self {
            name: TextInput::from_text(&draft.target),
            description: TextInput::from_text(&draft.description),
            visibility: draft.visibility,
            focus: Field::Name,
            error,
        }
    }

    /// The same form, showing why it was refused.
    pub(crate) fn with_error(self, reason: String) -> Self {
        Self {
            error: Some(reason),
            ..self
        }
    }

    pub(crate) fn draft(&self) -> CreateDraft {
        CreateDraft {
            target: self.name.text().trim().to_owned(),
            visibility: self.visibility,
            description: self.description.text(),
        }
    }

    /// The draft, if what is typed would be accepted; else the reason.
    pub(crate) fn validate(&self) -> Result<CreateDraft, HostError> {
        let draft = self.draft();
        parse_target(&draft.target)?;
        validate_description(&draft.description)?;
        Ok(draft)
    }

    /// One key in the form. `Visibility` is not text: arrows and `Space` change
    /// it. The name and the description are text; the description is one line
    /// of at most 350 characters that wraps over the rows of its box, like the
    /// body of a commit, so what is typed stays in view.
    pub(crate) fn key(&mut self, key: KeyEvent) -> FormKey {
        match key.code {
            KeyCode::Esc => return FormKey::Close,
            KeyCode::Enter => return FormKey::Continue,
            KeyCode::Tab | KeyCode::Down => self.focus = self.focus.step(true),
            KeyCode::BackTab | KeyCode::Up => self.focus = self.focus.step(false),
            _ => match self.focus {
                Field::Name => {
                    self.error = None;
                    self.name.handle_key_event(key, TextInputMode::SingleLine);
                },
                Field::Description => {
                    self.error = None;
                    self.description
                        .handle_key_event(key, TextInputMode::SingleLine);
                },
                Field::Visibility => {
                    if matches!(
                        key.code,
                        KeyCode::Left | KeyCode::Right | KeyCode::Char(' ')
                    ) {
                        self.visibility = match self.visibility {
                            Visibility::Private => Visibility::Public,
                            Visibility::Public => Visibility::Private,
                        };
                    }
                },
            },
        }
        FormKey::None
    }
}

/// Where the popup is.
#[derive(Debug)]
pub(crate) enum Step {
    /// `gh` is being asked whether it is ready; the answer carries this number
    /// so one that arrives after the popup was closed is ignored.
    Checking {
        generation: u64,
    },
    Form(Form),
    /// The last question, over the form it came from.
    Confirm(Form),
}

/// What a key asks the app to do with the popup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FormKey {
    None,
    Close,
    /// From the form: validate and ask the last question.
    Continue,
    /// From the question: back to the form.
    Back,
    Create,
}

impl Step {
    pub(crate) fn key(&mut self, key: KeyEvent) -> FormKey {
        match self {
            Self::Checking { .. } => {
                if key.code == KeyCode::Esc {
                    FormKey::Close
                } else {
                    FormKey::None
                }
            },
            Self::Form(form) => form.key(key),
            Self::Confirm(form) => match key.code {
                KeyCode::Esc | KeyCode::Char('n' | 'N') => FormKey::Back,
                KeyCode::Char('y' | 'Y') => FormKey::Create,
                // Enter confirms a private repository like every other question
                // of ferrit, and never a public one: that takes a `y`.
                KeyCode::Enter if form.visibility == Visibility::Private => FormKey::Create,
                _ => FormKey::None,
            },
        }
    }
}

/// What the renderer draws of the popup.
#[derive(Debug)]
pub enum CreateRemoteView<'a> {
    Checking,
    Form(FormView<'a>),
    Confirm(ConfirmView),
}

#[derive(Debug)]
pub struct FormView<'a> {
    pub name: &'a TextInput,
    pub description: &'a TextInput,
    pub visibility: Visibility,
    pub focus: Field,
    pub error: Option<&'a str>,
}

/// The last question: what will happen, and which keys answer it.
#[derive(Debug)]
pub struct ConfirmView {
    pub title: String,
    pub visibility: Visibility,
    pub lines: Vec<String>,
    pub hint: &'static str,
}

/// What the last question has to say about its consequences, which the form
/// does not know: they come from the repository.
pub(crate) struct Consequences<'a> {
    /// The repository has no commit yet, so one is made first.
    pub(crate) first_commit: bool,
    /// The user's own SSH alias for GitHub, if any.
    pub(crate) ssh_host: Option<&'a str>,
    /// The branch that will be pushed.
    pub(crate) branch: &'a str,
}

impl Step {
    /// What the renderer draws of this step.
    pub(crate) fn view(&self, consequences: &Consequences<'_>) -> CreateRemoteView<'_> {
        match self {
            Self::Checking { .. } => CreateRemoteView::Checking,
            Self::Form(form) => CreateRemoteView::Form(FormView {
                name: &form.name,
                description: &form.description,
                visibility: form.visibility,
                focus: form.focus,
                error: form.error.as_deref(),
            }),
            Self::Confirm(form) => {
                let draft = form.draft();
                let word = match draft.visibility {
                    Visibility::Private => "PRIVATE",
                    Visibility::Public => "PUBLIC",
                };
                let mut lines = vec![format!("{word} repository")];
                if draft.visibility == Visibility::Public {
                    lines.push("Everyone can read its history.".to_owned());
                }
                // Not a choice: a repository with no commit always gets one.
                if consequences.first_commit {
                    lines.push("first: commit an empty README.md, made by Ferrit".to_owned());
                }
                let over = consequences.ssh_host.map_or_else(String::new, |host| {
                    format!(" using your SSH key for {host}")
                });
                lines.push(format!(
                    "then: add remote `origin`{over}, push {}",
                    consequences.branch
                ));
                CreateRemoteView::Confirm(ConfirmView {
                    title: format!("Create {}", draft.target),
                    visibility: draft.visibility,
                    lines,
                    hint: match draft.visibility {
                        Visibility::Private => "Enter/y: create   n/Esc: back",
                        Visibility::Public => "y: create (Enter does not)   n/Esc: back",
                    },
                })
            },
        }
    }
}

/// The popups of creating the GitHub repository: the `gh` check, the form, and
/// the last question (`docs/PLAN_15_CREATE_REMOTE.md`).
pub(crate) fn draw_create_remote(
    frame: &mut Frame<'_>,
    area: Rect,
    view: &CreateRemoteView<'_>,
    accent: ratatui::style::Color,
    palette: &Palette,
) {
    match view {
        CreateRemoteView::Checking => {
            let focused = Style::new().fg(accent).add_modifier(Modifier::BOLD);
            let dialog = Dialog::new(Line::styled(" Create on GitHub ", focused))
                .fit_content(44.min(area.width), 1, 1)
                .border_style(focused)
                .render(frame, area);
            frame.render_widget(
                Paragraph::new(Line::styled(
                    " checking gh\u{2026}",
                    Style::new().fg(palette.idle),
                )),
                dialog.body,
            );
            frame.render_widget(
                Paragraph::new(KeyBar::hints("Cancel: Esc", palette).line()),
                dialog.footer,
            );
        },
        CreateRemoteView::Form(form) => draw_create_form(frame, area, form, accent, palette),
        CreateRemoteView::Confirm(confirm) => {
            draw_create_confirm(frame, area, confirm, accent, palette);
        },
    }
}

/// A framed text field like the commit popup's: the title on the border, a
/// character counter on the bottom border, and the text wrapped over the rows
/// of the box, so what was typed is always in view.
fn draw_text_box(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    input: &TextInput,
    focused: bool,
    max: usize,
    palette_style: (Style, Style),
) {
    let style = if focused {
        palette_style.0
    } else {
        palette_style.1
    };
    let counter = Line::styled(format!(" {}/{max} ", input.text().chars().count()), style);
    let block = Panel::new()
        .title(Line::styled(format!(" {title} "), style))
        .bottom_title(counter.right_aligned())
        .border_style(style)
        .block();
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if focused {
        input.render(frame, inner);
    } else {
        input.render_inactive(frame, inner);
    }
}

fn draw_create_form(
    frame: &mut Frame<'_>,
    area: Rect,
    form: &FormView<'_>,
    accent: ratatui::style::Color,
    palette: &Palette,
) {
    let focused = Style::new().fg(accent).add_modifier(Modifier::BOLD);
    let idle = Style::new().fg(palette.idle);
    // The name (two rows: up to 100 characters), the visibility, the
    // description (up to 350 characters wrap over its rows), an error line.
    let dialog = Dialog::new(Line::styled(" Create on GitHub ", focused))
        .fit_content(70.min(area.width), 14, 1)
        .border_style(focused)
        .render(frame, area);
    let rows = Layout::vertical([
        Constraint::Length(4),
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(1),
    ])
    .split(dialog.body);
    let at = |i: usize| rows.get(i).copied().unwrap_or_default();

    draw_text_box(
        frame,
        at(0),
        "Name",
        form.name,
        form.focus == Field::Name,
        100,
        (focused, idle),
    );

    let radio = |on: bool| if on { "(\u{2022})" } else { "( )" };
    let label_style = if form.focus == Field::Visibility {
        focused
    } else {
        idle
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            Span::styled(" Visibility   ", label_style),
            Span::raw(format!(
                "{} private   {} public",
                radio(form.visibility == Visibility::Private),
                radio(form.visibility == Visibility::Public)
            )),
        ])),
        at(1),
    );

    draw_text_box(
        frame,
        at(2),
        "Description",
        form.description,
        form.focus == Field::Description,
        350,
        (focused, idle),
    );

    if let Some(error) = form.error {
        frame.render_widget(
            Paragraph::new(Line::styled(
                format!(" {error}"),
                Style::new().fg(palette.del),
            )),
            at(3),
        );
    }
    frame.render_widget(
        Paragraph::new(KeyBar::hints("Next: Tab   Continue: Enter   Cancel: Esc", palette).line()),
        dialog.footer,
    );
}

fn draw_create_confirm(
    frame: &mut Frame<'_>,
    area: Rect,
    confirm: &ConfirmView,
    accent: ratatui::style::Color,
    palette: &Palette,
) {
    // A public repository takes the colour of the discard prompt.
    let border = match confirm.visibility {
        Visibility::Private => Style::new().fg(accent).add_modifier(Modifier::BOLD),
        Visibility::Public => Style::new().fg(palette.del).add_modifier(Modifier::BOLD),
    };
    let rows = u16::try_from(confirm.lines.len())
        .unwrap_or(u16::MAX)
        .max(1);
    let hint_width = u16::try_from(confirm.hint.chars().count() + 4).unwrap_or(u16::MAX);
    let title_width = u16::try_from(confirm.title.chars().count() + 6).unwrap_or(u16::MAX);
    // The widest line shows whole (a long SSH host, a long branch name).
    let text_width = confirm
        .lines
        .iter()
        .map(|line| u16::try_from(line.chars().count() + 4).unwrap_or(u16::MAX))
        .max()
        .unwrap_or(0);
    let dialog = Dialog::new(Line::styled(format!(" {} ", confirm.title), border))
        .fit_content(
            48.max(hint_width)
                .max(title_width)
                .max(text_width)
                .min(area.width),
            rows,
            1,
        )
        .border_style(border)
        .render(frame, area);
    let lines: Vec<Line<'static>> = confirm
        .lines
        .iter()
        .enumerate()
        .map(|(i, text)| {
            let style = if i == 0 { border } else { Style::new() };
            Line::styled(format!(" {text}"), style)
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), dialog.body);
    frame.render_widget(
        Paragraph::new(Line::styled(confirm.hint, Style::new().fg(palette.idle))),
        dialog.footer,
    );
}
