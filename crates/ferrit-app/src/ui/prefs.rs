//! What the user configured, and what that makes.

use crate::ui::keymap;
use ferrit_domain::diff::DiffOpts;
use ferrit_tui::theme::palette::Palette;
use ferrit_tui::theme::scheme::ColorDepth;
use std::path::PathBuf;
use std::time::Duration;

pub(crate) struct Prefs {
    /// Everything loaded from `config.toml`. Its `theme` is only the value read
    /// at startup: the theme being edited lives in `App.theme`.
    pub(crate) config: ferrit_config::Config,
    /// The keys, with the user's `[keys]` applied.
    pub(crate) keymap: keymap::Keymap,
    /// The `config.toml` a save writes to; `None` for `App::open` and the mock.
    pub(crate) file: Option<PathBuf>,
    /// What the terminal can show; the painted theme is RGB and is approximated
    /// with 256 colours when it has no 24-bit colour. True colour until the
    /// binary has looked (`COLORTERM`); the library never reads the environment.
    pub(crate) color_depth: ColorDepth,
    /// The colours everything is drawn with (`[theme]` in `config.toml`).
    pub(crate) palette: Palette,
}

impl Prefs {
    pub(crate) const fn new(
        config: ferrit_config::Config,
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

impl Prefs {
    /// `[ui] mouse`: should the terminal capture the mouse?
    pub(crate) const fn mouse_enabled(&self) -> bool {
        self.config.ui.mouse
    }

    /// `[ui] poll_secs` as a duration.
    pub(crate) const fn poll_interval(&self) -> Duration {
        Duration::from_secs(self.config.ui.poll_secs)
    }

    /// `[diff]` as the options `git diff` / `git show` are run with.
    pub(crate) const fn diff_opts(&self) -> DiffOpts {
        DiffOpts {
            context: self.config.diff.context,
            ignore_whitespace: self.config.diff.ignore_whitespace,
            rename_threshold: self.config.diff.rename_threshold,
        }
    }
}
