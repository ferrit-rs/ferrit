//! What a key does: the remappable bindings (`keymap`), and the key bar and the
//! help screen built from them (`hints`). Routing a key to the part of the app
//! that owns it is `app::input` and `app::dispatch`.

pub mod hints;
pub mod keymap;
