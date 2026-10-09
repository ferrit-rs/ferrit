//! The credential popup: a passphrase, password or host-key question that
//! ssh or git asked during a fetch, pull or push, answered here instead of
//! on the terminal the TUI owns (`git::askpass`,
//! `docs/PLAN_9_REMOTE.md`, "Credentials").

use crate::interface::components::ui::text_input::TextInput;
use std::sync::mpsc;

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
