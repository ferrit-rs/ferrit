//! The popups that fill a GitHub repository's creation: the `gh` check, the form
//! (name, visibility, description), and the last question. Their state, their
//! keys and what the renderer draws of them; running `gh` is
//! `git::actions::create_remote`.

use ratatui::crossterm::event::{KeyCode, KeyEvent};

use crate::git::host::{CreateDraft, HostError, Visibility, parse_target, validate_description};
use crate::tui::widgets::text_input::{TextInput, TextInputMode};

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
