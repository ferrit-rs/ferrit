//! What sits on top of the panes and takes the keys first: a popup (a text
//! box, a menu, a note) or a key-bar question. The two exclude each other, so
//! they are one value: opening one replaces the other, and the code that asks
//! "is a popup up?" cannot disagree with the code that asks "is a question up?".
//!
//! Methods live on `Modal`, not on `App`, so a caller can hold the popup
//! mutably while it reads other fields of `App`.

use crate::interface::state::confirm::ConfirmPrompt;
use crate::interface::state::popup::Popup;

#[derive(Default)]
pub(crate) enum Modal {
    #[default]
    None,
    Popup(Popup),
    Confirm(ConfirmPrompt),
}

impl Modal {
    pub(crate) fn is_some(&self) -> bool {
        !matches!(self, Self::None)
    }

    pub(crate) fn popup(&self) -> Option<&Popup> {
        match self {
            Self::Popup(popup) => Some(popup),
            _ => None,
        }
    }

    pub(crate) fn popup_mut(&mut self) -> Option<&mut Popup> {
        match self {
            Self::Popup(popup) => Some(popup),
            _ => None,
        }
    }

    /// Show `popup`, replacing whatever was up.
    pub(crate) fn open_popup(&mut self, popup: Popup) {
        *self = Self::Popup(popup);
    }

    /// Close the popup. A question that is up is not a popup and stays.
    pub(crate) fn close_popup(&mut self) {
        if matches!(self, Self::Popup(_)) {
            *self = Self::None;
        }
    }

    pub(crate) fn take_popup(&mut self) -> Option<Popup> {
        match std::mem::take(self) {
            Self::Popup(popup) => Some(popup),
            other => {
                *self = other;
                None
            },
        }
    }

    pub(crate) fn confirm(&self) -> Option<&ConfirmPrompt> {
        match self {
            Self::Confirm(prompt) => Some(prompt),
            _ => None,
        }
    }

    /// Ask `prompt` on the key bar, replacing whatever was up.
    pub(crate) fn ask(&mut self, prompt: ConfirmPrompt) {
        *self = Self::Confirm(prompt);
    }

    /// Withdraw the question. A popup that is up is not a question and stays.
    pub(crate) fn cancel_confirm(&mut self) {
        if matches!(self, Self::Confirm(_)) {
            *self = Self::None;
        }
    }

    pub(crate) fn take_confirm(&mut self) -> Option<ConfirmPrompt> {
        match std::mem::take(self) {
            Self::Confirm(prompt) => Some(prompt),
            other => {
                *self = other;
                None
            },
        }
    }
}
