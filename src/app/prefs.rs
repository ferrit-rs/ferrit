//! What the user configured, and what that makes: the loaded `config.toml`, the
//! keymap built from its `[keys]`, the file a save writes to, the colour depth
//! of the terminal and the palette everything is drawn with. The settings sheet
//! changes them (`settings`), the binary sets the colour depth once.

use std::path::PathBuf;

use super::{config, keymap};
use crate::components::ui::palette::Palette;
use crate::components::ui::scheme::ColorDepth;

pub(super) struct Prefs {
    /// Everything loaded from `config.toml`. Its `theme` is only the value read
    /// at startup: the theme being edited lives in `App.theme`.
    pub(super) config: config::Config,
    /// The keys, with the user's `[keys]` applied.
    pub(super) keymap: keymap::Keymap,
    /// The `config.toml` a save writes to; `None` for `App::open` and the mock.
    pub(super) file: Option<PathBuf>,
    /// What the terminal can show; the painted theme is RGB and is approximated
    /// with 256 colours when it has no 24-bit colour. True colour until the
    /// binary has looked (`COLORTERM`); the library never reads the environment.
    pub(super) color_depth: ColorDepth,
    /// The colours everything is drawn with (`[theme]` in `config.toml`).
    pub(super) palette: Palette,
}

impl Prefs {
    pub(super) const fn new(
        config: config::Config,
        keymap: keymap::Keymap,
        palette: Palette,
    ) -> Self {
        Self {
            config,
            keymap,
            file: None,
            color_depth: ColorDepth::TrueColor,
            palette,
        }
    }
}
