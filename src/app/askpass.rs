//! The credential popup: a passphrase, password or host-key question that
//! ssh or git asked during a fetch, pull or push, answered here instead of
//! on the terminal the TUI owns (`domain::git::askpass`,
//! `docs/PLAN_9_REMOTE.md`, "Credentials").

use super::{App, CommitPopupView, KeyCode, KeyEvent, Popup, TextInput, TextInputMode, mpsc};
use crate::domain::git::askpass;

/// One pending question and where its answer goes. `typed` holds the real
/// text; `shown` is what the popup draws, dots when the answer is secret.
pub(super) struct AskpassPrompt {
    prompt: String,
    typed: TextInput,
    shown: TextInput,
    secret: bool,
    reply: mpsc::Sender<Option<String>>,
}

impl AskpassPrompt {
    fn reply(&self, answer: Option<String>) {
        let _ = self.reply.send(answer);
    }
}

impl App {
    /// A git child asked `prompt`. Open the popup, unless another popup is
    /// up: overwriting a half-typed commit message would lose it, so that
    /// question is cancelled and the operation fails instead.
    pub(super) fn on_askpass(&mut self, prompt: String, reply: mpsc::Sender<Option<String>>) {
        if self.popup.is_some() {
            let _ = reply.send(None);
            return;
        }
        self.popup = Some(Popup::Askpass(AskpassPrompt {
            secret: askpass::is_secret(&prompt),
            prompt,
            typed: TextInput::default(),
            shown: TextInput::default(),
            reply,
        }));
    }

    /// Enter answers, Esc cancels (git then reports the failed login);
    /// anything else edits the answer.
    pub(super) fn askpass_key(&mut self, key: KeyEvent) {
        let Some(Popup::Askpass(ask)) = &mut self.popup else {
            return;
        };
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
                return;
            },
        }
        self.popup = None;
    }

    pub fn askpass_popup(&self) -> Option<CommitPopupView<'_>> {
        let Some(Popup::Askpass(ask)) = &self.popup else {
            return None;
        };
        Some(CommitPopupView {
            title: ask.prompt.trim_end().trim_end_matches(':'),
            input: &ask.shown,
            description: None,
            summary_focused: false,
            overlay_state: None,
            lines: ask.shown.lines(),
            cursor: ask.shown.cursor(),
            toggles: None,
            author: None,
            hints: "Send: Enter | Cancel: Esc",
        })
    }
}
