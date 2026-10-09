//! The settings sheet's rows and what changing one does
//! (`docs/PLAN_17_SETTINGS.md`): ferrit's own settings, nothing of git's. A
//! change applies at once to the live `Config`, and is saved at once to
//! `config.toml` (only its own section, through `Config::save_sections`), so the
//! next start finds the sheet as it was left.

use ratatui::layout::Rect;

use crate::config::settings::SettingsRow;

/// What a click on a part of a row does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Click {
    /// Just highlight the row.
    Row,
    /// A radio: set the choice at this index.
    Choice(usize),
    /// A checkbox: flip it.
    Flip,
    /// The `‹` (down) or `›` (up) around a number.
    Step(bool),
}

/// Where the sheet's clickable parts landed on the last frame.
#[derive(Debug, Clone, Default)]
pub(crate) struct SettingsHits {
    pub color_grid: Rect,
    pub color_grid_first_row: usize,
    /// Narrowest last: a part of a row comes before the row's whole line.
    pub parts: Vec<(Rect, SettingsRow, Click)>,
}
