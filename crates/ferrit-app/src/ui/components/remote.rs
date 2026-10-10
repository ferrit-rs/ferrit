//! Fetch, pull, push: what the keys ask for. Running them in the background and
//! taking their answer are `workers`.

use crate::ui::components::popups::{ConfirmAction, ConfirmPrompt, Popup};
use crate::ui::error::AppError;
use crate::ui::event::{Env, Event};
use ferrit_domain::credentials::is_secret;
use ferrit_domain::remote::{self, PushPlan, RemoteOp, RemoteRequest};
use ferrit_tui::widgets::text_input::{TextInput, TextInputMode};
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use std::sync::mpsc;

/// `f` / `p`: fetch / pull. Global, not Branches-only: these act on the repo and
/// its current branch, not a selected row. A no-op with no event sender set
/// (`App::mock()`, or a test driving `on_key` without `run()`).
pub(crate) fn trigger(op: RemoteOp) -> Vec<Event> {
    vec![Event::StartRemote(RemoteRequest::new(op))]
}

/// `P`: push. Unlike `f`/`p`, push needs to know *before* running whether the
/// current branch has an upstream at all (the Status header already has it, read
/// for the ahead/behind count): `ferrit_domain::remote::plan_push` decides between a plain
/// push, a lease-guarded one that asks first, one that sets the upstream, and a
/// LazyGit-style editable `<remote> <branch>` prompt.
pub(crate) fn push(env: &Env<'_>) -> Vec<Event> {
    if env.popup_up {
        return Vec::new();
    }
    let Some(repo) = env.repo else {
        return Vec::new();
    };
    let remotes = if env.snapshot.header.upstream.is_some() {
        Vec::new()
    } else {
        match repo.remotes() {
            Ok(remotes) => remotes,
            Err(error) => return vec![Event::Report(error.into())],
        }
    };
    match remote::plan_push(&env.snapshot.header, &remotes, repo.push_default_current()) {
        PushPlan::Plain => trigger(RemoteOp::Push),
        PushPlan::ConfirmForce(message) => vec![Event::Ask(ConfirmPrompt {
            message,
            action: ConfirmAction::ForcePush,
        })],
        PushPlan::SetCurrent => vec![Event::StartRemote(RemoteRequest::push_current())],
        PushPlan::AskUpstream(text) => vec![Event::OpenPopup(Popup::Upstream(
            TextInput::from_text(&text),
        ))],
    }
}

/// The upstream prompt was answered: `git push -u <remote> <local>:<remote
/// branch>`.
pub(crate) fn submit_upstream(value: &str) -> Vec<Event> {
    let Some((remote, branch)) = remote::parse_upstream(value) else {
        return vec![Event::Report(AppError::BadUpstream)];
    };
    vec![
        Event::ClosePopup,
        Event::StartRemote(RemoteRequest::push_to(remote.to_owned(), branch.to_owned())),
    ]
}

/// One pending question and where its answer goes. `typed` holds the real
/// text; `shown` is what the popup draws, dots when the answer is secret.
pub(crate) struct AskpassPrompt {
    pub(crate) prompt: String,
    pub(crate) typed: TextInput,
    pub(crate) shown: TextInput,
    pub(crate) secret: bool,
    pub(crate) reply: mpsc::Sender<Option<String>>,
}

impl AskpassPrompt {
    pub(crate) fn reply(&self, answer: Option<String>) {
        let _ = self.reply.send(answer);
    }
}

/// A git child asked `prompt`. Open the popup, unless another popup is up:
/// overwriting a half-typed commit message would lose it, so that question is
/// cancelled and the operation fails instead.
pub(crate) fn ask(
    prompt: String,
    reply: mpsc::Sender<Option<String>>,
    popup_up: bool,
) -> Vec<Event> {
    if popup_up {
        let _ = reply.send(None);
        return Vec::new();
    }
    vec![Event::OpenPopup(Popup::Askpass(AskpassPrompt {
        secret: is_secret(&prompt),
        prompt,
        typed: TextInput::default(),
        shown: TextInput::default(),
        reply,
    }))]
}

/// Enter answers, Esc cancels (git then reports the failed login); anything else
/// edits the answer.
pub(crate) fn key(ask: &mut AskpassPrompt, key: KeyEvent) -> Vec<Event> {
    match key.code {
        KeyCode::Enter => ask.reply(Some(ask.typed.text())),
        KeyCode::Esc => ask.reply(None),
        _ => {
            ask.typed.handle_key_event(key, TextInputMode::SingleLine);
            let text = ask.typed.text();
            ask.shown = TextInput::from_text(&if ask.secret {
                "\u{2022}".repeat(text.chars().count())
            } else {
                text
            });
            return Vec::new();
        },
    }
    vec![Event::ClosePopup]
}
