//! What the keys do in `App` for `askpass`: the glue between the interface, the git code and the app's state.

use crate::app::App;
use crate::app::state::askpass::AskpassPrompt;
use crate::app::state::popup::Popup;
use crate::app::state::views::CommitPopupView;
use crate::git::askpass;
use crate::ui::widgets::text_input::{TextInput, TextInputMode};
use ratatui::crossterm::event::KeyCode;
use ratatui::crossterm::event::KeyEvent;
use std::sync::mpsc;

impl App {
    /// A git child asked `prompt`. Open the popup, unless another popup is
    /// up: overwriting a half-typed commit message would lose it, so that
    /// question is cancelled and the operation fails instead.
    pub(crate) fn on_askpass(&mut self, prompt: String, reply: mpsc::Sender<Option<String>>) {
        if self.modal.popup().is_some() {
            let _ = reply.send(None);
            return;
        }
        self.modal.open_popup(Popup::Askpass(AskpassPrompt {
            secret: askpass::is_secret(&prompt),
            prompt,
            typed: TextInput::default(),
            shown: TextInput::default(),
            reply,
        }));
    }

    /// Enter answers, Esc cancels (git then reports the failed login);
    /// anything else edits the answer.
    pub(crate) fn askpass_key(&mut self, key: KeyEvent) {
        let Some(Popup::Askpass(ask)) = self.modal.popup_mut() else {
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
        self.modal.close_popup();
    }

    pub fn askpass_popup(&self) -> Option<CommitPopupView<'_>> {
        let Some(Popup::Askpass(ask)) = self.modal.popup() else {
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
