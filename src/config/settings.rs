//! The rows of the settings sheet and what changing one does
//! (`docs/PLAN_17_SETTINGS.md`): ferrit's own settings, nothing of git's. The
//! sheet itself (keys, clicks, drawing) is in `app::settings` and
//! `app::screens::settings`.

use crate::config::Section;

/// One line of the sheet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, strum::EnumIter)]
pub enum SettingsRow {
    Theme,
    Accent,
    Mouse,
    WheelStep,
    DiffContext,
    IgnoreWhitespace,
    SignOff,
    ShowReads,
}

/// How a row's value is shown and changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// One of a few named values; `←` `→` walk them.
    Choice(&'static [&'static str]),
    Toggle,
    /// A number between `min` and `max`.
    Number {
        min: u64,
        max: u64,
    },
}

impl SettingsRow {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Theme => "Theme",
            Self::Accent => "Accent",
            Self::Mouse => "Mouse",
            Self::WheelStep => "Wheel step",
            Self::DiffContext => "Context lines",
            Self::IgnoreWhitespace => "Ignore whitespace",
            Self::SignOff => "Sign-off by default",
            Self::ShowReads => "Show read commands",
        }
    }

    /// The section title the row sits under.
    #[must_use]
    pub const fn group(self) -> &'static str {
        match self {
            Self::Theme | Self::Accent => "Appearance",
            Self::Mouse | Self::WheelStep => "Interface",
            Self::DiffContext | Self::IgnoreWhitespace => "Diff",
            Self::SignOff => "Commit",
            Self::ShowReads => "Command log",
        }
    }

    #[must_use]
    pub const fn kind(self) -> Kind {
        match self {
            Self::Theme => Kind::Choice(&["Terminal", "Dark", "Light"]),
            Self::Accent => Kind::Choice(&["Green", "Blue", "Purple", "Amber"]),
            Self::WheelStep => Kind::Number { min: 1, max: 50 },
            Self::DiffContext => Kind::Number { min: 0, max: 200 },
            Self::Mouse | Self::IgnoreWhitespace | Self::SignOff | Self::ShowReads => Kind::Toggle,
        }
    }

    pub(crate) const fn section(self) -> Section {
        match self {
            Self::Theme | Self::Accent => Section::Theme,
            Self::Mouse | Self::WheelStep => Section::Ui,
            Self::DiffContext | Self::IgnoreWhitespace => Section::Diff,
            Self::SignOff => Section::Commit,
            Self::ShowReads => Section::Log,
        }
    }
}

/// What the footer of the sheet says about the file.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum SaveState {
    /// Nothing written yet this run, or there is no file to write.
    #[default]
    Idle,
    /// The last change is on disk.
    Saved,
    /// The last change applies for this run but could not be written; why.
    Failed(String),
}

/// The sheet's own state: the highlighted row and the footer.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SettingsSheet {
    pub selected: usize,
    /// The next frame scrolls the selected row into view.
    pub follow: bool,
    pub save: SaveState,
    /// First visible line of the sheet.
    pub scroll: usize,
}

/// Something only the run loop can do for a setting that changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalRequest {
    /// Switch the terminal's mouse capture on or off.
    Mouse(bool),
}

/// `value` moved by one unit in `up`'s direction, clamped to `min..=max`.
pub(crate) fn stepped(value: u64, up: bool, min: u64, max: u64) -> u64 {
    if up {
        value.saturating_add(1).min(max)
    } else {
        value.saturating_sub(1).max(min)
    }
}
