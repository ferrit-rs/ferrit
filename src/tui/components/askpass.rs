//! The credential prompt a git child may raise: the popup that asks, and its keys.

use std::sync::mpsc;

use ratatui::crossterm::event::{KeyCode, KeyEvent};

use crate::git::askpass;
use crate::tui::components::popups::Popup;
use crate::tui::event::Event;
use crate::tui::widgets::text_input::{TextInput, TextInputMode};

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
        secret: askpass::is_secret(&prompt),
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
