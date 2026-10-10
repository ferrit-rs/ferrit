//! Creating the GitHub repository: state, form input, and popup flow.

use crate::git::host::{
    CreateDraft, GhProgram, GhStatus, HostError, Visibility, parse_target, sanitize_name,
    validate_description,
};
use crate::git::ssh_config::read_github_aliases;
use crate::tui::components::popups::{Modal, Popup};
use crate::tui::error::AppError;
use crate::tui::widgets::text_input::{TextInput, TextInputMode};
use color_eyre::Result;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use std::path::PathBuf;
use std::sync::Arc;
use strum::{EnumIter, IntoEnumIterator};

pub mod view;

/// The creation's own state on `App`.
#[derive(Debug, Default)]
pub struct CreateRemoteState {
    /// The last draft, until a creation succeeds.
    pub draft: Option<CreateDraft>,
    /// Why the last creation was refused, for the form to show.
    pub error: Option<String>,
    /// The web URL of the repository just created.
    pub web_url: Option<String>,
    pub(crate) gh: GhProgram,
    /// Bumped each time the check starts or is abandoned.
    pub(crate) generation: u64,
    /// The push in flight is the one that follows a creation.
    pub(crate) pushing_after: bool,
    /// The ssh config the host aliases are read from; `None` is `~/.ssh/config`.
    /// A test points it elsewhere.
    pub(crate) ssh_config: Option<PathBuf>,
    /// The SSH host chosen when the creation started, used once `gh` has made
    /// the repository.
    pub(crate) ssh_host: String,
}

impl CreateRemoteState {
    /// Keep the `gh` program a test or the replay injected when the app is
    /// rebuilt on a new repository (`App::attach_repository`).
    pub(crate) fn carry_program_from(&mut self, previous: &Self) {
        self.gh = previous.gh.clone();
        self.ssh_config.clone_from(&previous.ssh_config);
    }

    /// The GitHub aliases of the ssh config, in the order it lists them.
    pub(crate) fn ssh_aliases(&self) -> Vec<String> {
        let path = self.ssh_config.clone().or_else(|| {
            std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".ssh/config"))
        });
        path.map_or_else(Vec::new, |path| read_github_aliases(&path))
    }

    /// The answer of the `gh` check the popup waits for. Only that check
    /// counts; `gh` ready opens the form, anything else says what to do.
    pub(crate) fn gh_checked(
        &self,
        modal: &mut Modal,
        generation: u64,
        status: GhStatus,
        repo_name: &str,
    ) {
        let waiting = matches!(
            modal.popup(),
            Some(Popup::CreateRemote(Step::Checking { generation: g })) if *g == generation
        );
        if !waiting {
            return;
        }
        modal.open_popup(match status {
            GhStatus::Ready => {
                let draft = self
                    .draft
                    .clone()
                    .unwrap_or_else(|| CreateDraft::new(sanitize_name(repo_name)));
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
    pub(crate) fn reopen_form(&self, modal: &mut Modal, error: String) {
        let Some(draft) = &self.draft else {
            return;
        };
        if modal.popup().is_none() {
            modal.open_popup(Popup::CreateRemote(Step::Form(Form::from_draft(
                draft,
                Some(error),
            ))));
        }
    }

    /// Every key while the popup is up. A confirmed creation comes back as the
    /// draft to start.
    pub(crate) fn key(&mut self, modal: &mut Modal, key: KeyEvent) -> Option<CreateDraft> {
        let Some(Popup::CreateRemote(step)) = modal.popup_mut() else {
            return None;
        };
        match step.key(key) {
            FormKey::None => {},
            FormKey::Close => self.close(modal),
            FormKey::Continue => {
                let Some(Popup::CreateRemote(Step::Form(form))) = modal.take_popup() else {
                    return None;
                };
                match form.validate() {
                    Ok(_) => modal.open_popup(Popup::CreateRemote(Step::Confirm(form))),
                    Err(reason) => modal.open_popup(Popup::CreateRemote(Step::Form(
                        form.with_error(reason.to_string()),
                    ))),
                }
            },
            FormKey::Back => {
                if let Some(Popup::CreateRemote(Step::Confirm(form))) = modal.take_popup() {
                    modal.open_popup(Popup::CreateRemote(Step::Form(form)));
                }
            },
            FormKey::Create => {
                if let Some(Popup::CreateRemote(Step::Confirm(form))) = modal.take_popup() {
                    return Some(form.draft());
                }
            },
        }
        None
    }

    /// `Esc`: nothing was created. Typed fields stay for next time; a check
    /// still running is abandoned.
    fn close(&mut self, modal: &mut Modal) {
        match modal.take_popup() {
            Some(Popup::CreateRemote(Step::Form(form) | Step::Confirm(form))) => {
                self.draft = Some(form.draft());
            },
            Some(Popup::CreateRemote(Step::Checking { .. })) => self.generation += 1,
            Some(other) => modal.open_popup(other),
            None => {},
        }
    }

    /// The repository exists: the draft is spent, and the web URL is kept for
    /// the push that follows. Returns the ssh host chosen when it started.
    pub(crate) fn created(&mut self, url: &str) -> String {
        self.draft = None;
        self.error = None;
        self.web_url = Some(url.to_owned());
        std::mem::take(&mut self.ssh_host)
    }

    /// The failure of the push that followed a creation says the repository
    /// exists and how to retry; any other failure is left as it is.
    pub(crate) fn explain_push(&mut self, failure: AppError) -> AppError {
        if !std::mem::take(&mut self.pushing_after) {
            return failure;
        }
        match &self.web_url {
            Some(url) => AppError::PushAfterCreation {
                source: Arc::new(failure),
                url: url.clone(),
            },
            None => failure,
        }
    }
}

/// The field of the form that has the focus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, EnumIter)]
pub enum Field {
    Name,
    Visibility,
    Description,
}

impl Field {
    fn step(self, forward: bool) -> Self {
        let at = Self::iter().position(|field| field == self).unwrap_or(0);
        let count = Self::iter().count();
        let next = if forward {
            (at + 1) % count
        } else {
            (at + count - 1) % count
        };
        Self::iter().nth(next).unwrap_or(Self::Name)
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

    /// One key in the form. Visibility is not text: arrows and `Space` change
    /// it. Text fields use single-line input.
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
