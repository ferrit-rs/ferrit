//! Small reusable UI building blocks (`components/ui/*`).

mod dialog;
mod key_bar;
mod panel;
mod select_list;
mod text_input;

pub use dialog::{Dialog, DialogAreas};
pub use key_bar::KeyBar;
pub use panel::Panel;
pub use select_list::SelectList;
pub use text_input::{TextInput, TextInputMode};
