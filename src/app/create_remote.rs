//! Creating the GitHub repository from ferrit (`docs/PLAN_15_CREATE_REMOTE.md`,
//! R2, R3): the draft the user typed, the popups that fill it (the `gh`
//! check, the form, the last question), the background `gh repo create`, and
//! what happens when it answers.
//!
//! The creation takes the same slot as fetch, pull and push (`remote_busy`),
//! so one network operation runs at a time, the Status pane shows it, and quit
//! cancels it the way it cancels them.

use std::sync::atomic::Ordering;
use std::time::Instant;

use super::{
    App, AppError, AppEvent, KeyCode, KeyEvent, Popup, Result, TextInput, TextInputMode,
    WorkerKind, events, mpsc, run_worker, thread,
};
use crate::domain::git::host::{
    self, CreateRequest, GhProgram, GhStatus, Visibility, parse_target, sanitize_name,
    validate_description,
};

/// What the form holds. Kept on the app while `gh` runs, so a refusal can
/// reopen the form with every field as typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateDraft {
    /// `name` or `owner/name`.
    pub target: String,
    pub visibility: Visibility,
    pub description: String,
    /// Push the current branch once the repository exists.
    pub push_after: bool,
}

impl CreateDraft {
    /// The form's starting point: private, pushing, named after the folder.
    #[must_use]
    pub fn new(target: String) -> Self {
        Self {
            target,
            visibility: Visibility::Private,
            description: String::new(),
            push_after: true,
        }
    }
}

/// The field of the form that has the focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Name,
    Visibility,
    Description,
    Push,
}

impl Field {
    const ORDER: [Self; 4] = [Self::Name, Self::Visibility, Self::Description, Self::Push];

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
pub(super) struct Form {
    name: TextInput,
    description: TextInput,
    visibility: Visibility,
    push_after: bool,
    focus: Field,
    error: Option<String>,
}

impl Form {
    fn from_draft(draft: &CreateDraft, error: Option<String>) -> Self {
        Self {
            name: TextInput::from_text(&draft.target),
            description: TextInput::from_text(&draft.description),
            visibility: draft.visibility,
            push_after: draft.push_after,
            focus: Field::Name,
            error,
        }
    }

    fn draft(&self) -> CreateDraft {
        CreateDraft {
            target: self.name.text().trim().to_owned(),
            visibility: self.visibility,
            description: self.description.text(),
            push_after: self.push_after,
        }
    }

    /// The draft, if what is typed would be accepted; else the reason.
    fn validate(&self) -> Result<CreateDraft, String> {
        let draft = self.draft();
        parse_target(&draft.target).map_err(|e| e.to_string())?;
        validate_description(&draft.description).map_err(|e| e.to_string())?;
        Ok(draft)
    }

    /// One key in the form. `Visibility` and `Push` are not text: arrows and
    /// `Space` change them, and the rest of the keys do nothing there.
    fn key(&mut self, key: KeyEvent) -> Action {
        match key.code {
            KeyCode::Esc => return Action::Close,
            KeyCode::Enter => return Action::Continue,
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
                Field::Push => {
                    if key.code == KeyCode::Char(' ') {
                        self.push_after = !self.push_after;
                    }
                },
            },
        }
        Action::None
    }
}

/// Where the popup is.
#[derive(Debug)]
pub(super) enum Step {
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
enum Action {
    None,
    Close,
    /// From the form: validate and ask the last question.
    Continue,
    /// From the question: back to the form.
    Back,
    Create,
}

impl Step {
    fn key(&mut self, key: KeyEvent) -> Action {
        match self {
            Self::Checking { .. } => {
                if key.code == KeyCode::Esc {
                    Action::Close
                } else {
                    Action::None
                }
            },
            Self::Form(form) => form.key(key),
            Self::Confirm(form) => match key.code {
                KeyCode::Esc | KeyCode::Char('n' | 'N') => Action::Back,
                KeyCode::Char('y' | 'Y') => Action::Create,
                // Enter confirms a private repository like every other question
                // of ferrit, and never a public one: that takes a `y`.
                KeyCode::Enter if form.visibility == Visibility::Private => Action::Create,
                _ => Action::None,
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
    pub push_after: bool,
    pub focus: Field,
    pub error: Option<&'a str>,
    /// The branch that gets pushed, for the checkbox's label.
    pub branch: &'a str,
}

/// The last question: what will happen, and which keys answer it.
#[derive(Debug)]
pub struct ConfirmView {
    pub title: String,
    pub visibility: Visibility,
    pub lines: Vec<String>,
    pub hint: &'static str,
}

/// The creation's own state on `App`.
#[derive(Debug, Default)]
pub struct CreateRemote {
    /// The last draft, until a creation succeeds.
    pub draft: Option<CreateDraft>,
    /// Why the last creation was refused, for the form to show.
    pub error: Option<String>,
    /// The web URL of the repository just created.
    pub web_url: Option<String>,
    gh: GhProgram,
    /// Bumped each time the check starts or is abandoned.
    generation: u64,
}

impl App {
    /// The creation's state, for the popups and for tests.
    pub fn create_remote(&self) -> &CreateRemote {
        &self.create_remote
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
        if self.popup.is_some() || self.pending_confirm.is_some() || self.repo.is_none() {
            return;
        }
        if !self.remotes.is_empty() {
            self.popup = Some(Popup::Note(
                "this repository already has a remote".to_owned(),
            ));
            return;
        }
        if self.remote_busy.is_some() {
            self.popup = Some(Popup::Note(
                "another network operation is running".to_owned(),
            ));
            return;
        }
        self.create_remote.generation += 1;
        let generation = self.create_remote.generation;
        self.popup = Some(Popup::CreateRemote(Step::Checking { generation }));
        let gh = self.create_remote.gh.clone();
        let Some(sender) = self.event_sender.clone() else {
            // No event loop (`App::mock`, a test without `run()`): ask now.
            let status = host::gh_status(&gh);
            self.on_gh_checked(generation, status);
            return;
        };
        thread::spawn(move || {
            let status = run_worker(WorkerKind::RemoteOperation, || host::gh_status(&gh))
                .unwrap_or(GhStatus::Missing);
            let _ = sender.send(AppEvent::GhChecked { generation, status });
        });
    }

    /// `AppEvent::GhChecked` arrived. Only the check the popup is waiting for
    /// counts; `gh` ready opens the form, anything else says what to do.
    pub(super) fn on_gh_checked(&mut self, generation: u64, status: GhStatus) {
        let waiting = matches!(
            self.popup,
            Some(Popup::CreateRemote(Step::Checking { generation: g })) if g == generation
        );
        if !waiting {
            return;
        }
        self.popup = Some(match status {
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
        if self.popup.is_none() {
            self.popup = Some(Popup::CreateRemote(Step::Form(Form::from_draft(
                draft,
                Some(error),
            ))));
        }
    }

    /// What the renderer draws, or `None` when this popup is not up.
    pub fn create_remote_view(&self) -> Option<CreateRemoteView<'_>> {
        let Some(Popup::CreateRemote(step)) = &self.popup else {
            return None;
        };
        Some(match step {
            Step::Checking { .. } => CreateRemoteView::Checking,
            Step::Form(form) => CreateRemoteView::Form(FormView {
                name: &form.name,
                description: &form.description,
                visibility: form.visibility,
                push_after: form.push_after,
                focus: form.focus,
                error: form.error.as_deref(),
                branch: &self.header.branch,
            }),
            Step::Confirm(form) => {
                let draft = form.draft();
                let word = match draft.visibility {
                    Visibility::Private => "PRIVATE",
                    Visibility::Public => "PUBLIC",
                };
                let mut lines = vec![format!("{word} repository")];
                if draft.visibility == Visibility::Public {
                    lines.push("Everyone can read its history.".to_owned());
                }
                lines.push(if draft.push_after {
                    format!("then: add remote `origin`, push {}", self.header.branch)
                } else {
                    "then: add remote `origin` (nothing is pushed)".to_owned()
                });
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
        })
    }

    /// Every key while the popup is up.
    pub(super) fn create_remote_key(&mut self, key: KeyEvent) {
        let Some(Popup::CreateRemote(step)) = &mut self.popup else {
            return;
        };
        match step.key(key) {
            Action::None => {},
            Action::Close => self.close_create_remote(),
            Action::Continue => {
                let Some(Popup::CreateRemote(Step::Form(form))) = self.popup.take() else {
                    return;
                };
                match form.validate() {
                    Ok(_) => self.popup = Some(Popup::CreateRemote(Step::Confirm(form))),
                    Err(reason) => {
                        self.popup = Some(Popup::CreateRemote(Step::Form(Form {
                            error: Some(reason),
                            ..form
                        })));
                    },
                }
            },
            Action::Back => {
                if let Some(Popup::CreateRemote(Step::Confirm(form))) = self.popup.take() {
                    self.popup = Some(Popup::CreateRemote(Step::Form(form)));
                }
            },
            Action::Create => {
                let Some(Popup::CreateRemote(Step::Confirm(form))) = self.popup.take() else {
                    return;
                };
                let Some(sender) = self.event_sender.clone() else {
                    return;
                };
                self.start_create_remote(form.draft(), sender);
            },
        }
    }

    /// `Esc`: nothing was created. The typed fields are kept for next time; a
    /// check still running is abandoned.
    fn close_create_remote(&mut self) {
        match self.popup.take() {
            Some(Popup::CreateRemote(Step::Form(form) | Step::Confirm(form))) => {
                self.create_remote.draft = Some(form.draft());
            },
            Some(Popup::CreateRemote(Step::Checking { .. })) => self.create_remote.generation += 1,
            other => self.popup = other,
        }
    }

    /// Start `gh repo create` for `draft` in the background. One network
    /// operation at a time: ignored while another runs. A target that fails
    /// validation is reported at once and nothing starts. Takes `sender` like
    /// `start_remote_op`, so a test can drive the answer itself.
    #[doc(hidden)]
    pub fn start_create_remote(&mut self, draft: CreateDraft, sender: mpsc::Sender<AppEvent>) {
        if self.remote_busy.is_some() {
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
        self.create_remote.draft = Some(draft);
        self.create_remote.error = None;
        self.remote_busy = Some(events::RemoteOp::Create);
        self.remote_busy_started = Some(Instant::now());
        self.status_note = None;
        self.remote_cancel.store(false, Ordering::Release);
        let cancel = std::sync::Arc::clone(&self.remote_cancel);
        self.remote_worker = Some(thread::spawn(move || {
            let result = run_worker(WorkerKind::RemoteOperation, || {
                repo.create_repo(&gh, &request, &cancel)
            })
            .map_err(|error| error.to_string())
            .and_then(|result| result.map_err(|error| error.to_string()))
            .map(|created| created.web_url);
            let _ = sender.send(AppEvent::RemoteCreated(result));
        }));
    }

    /// `AppEvent::RemoteCreated` arrived. The slot is freed and the panes
    /// refresh (`gh` added `origin`). A success keeps the web URL and drops the
    /// draft; a refusal keeps the draft and says why, nothing was configured.
    pub fn on_remote_created(&mut self, result: Result<String, String>) {
        self.remote_busy = None;
        self.remote_busy_started = None;
        if let Some(worker) = self.remote_worker.take() {
            let _ = worker.join();
        }
        self.request_refresh();
        match result {
            Ok(url) => {
                self.create_remote.draft = None;
                self.create_remote.error = None;
                self.last_error = None;
                self.status_note = Some(format!("Created {url}"));
                self.create_remote.web_url = Some(url);
            },
            Err(message) => {
                self.create_remote.error = Some(message.clone());
                self.status_note = None;
                self.reopen_form(message.clone());
                self.report_error(AppError::Background(message));
            },
        }
    }
}
