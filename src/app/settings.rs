//! The settings sheet's rows and what changing one does
//! (`docs/PLAN_17_SETTINGS.md`): ferrit's own settings, nothing of git's. A
//! change applies at once to the live `Config`, and is saved at once to
//! `config.toml` (only its own section, through `Config::save_sections`), so the
//! next start finds the sheet as it was left.

use std::time::Duration;

use super::App;
use super::config::{Config, Section};
use super::theme_config::{Base, Preset};

/// The values the refresh interval steps through: a short scale, since
/// "10 s, 11 s, 12 s…" is not what anyone wants to press. A value written in
/// the file that is not on it steps to the next one above or below.
pub const REFRESH_STEPS: [u64; 13] = [1, 2, 3, 5, 10, 15, 30, 60, 120, 300, 600, 1800, 3600];

/// One line of the sheet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsRow {
    Theme,
    Accent,
    Mouse,
    WheelStep,
    RefreshSecs,
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
    pub const ALL: [Self; 9] = [
        Self::Theme,
        Self::Accent,
        Self::Mouse,
        Self::WheelStep,
        Self::RefreshSecs,
        Self::DiffContext,
        Self::IgnoreWhitespace,
        Self::SignOff,
        Self::ShowReads,
    ];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Theme => "Theme",
            Self::Accent => "Accent",
            Self::Mouse => "Mouse",
            Self::WheelStep => "Wheel step",
            Self::RefreshSecs => "Refresh every",
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
            Self::Mouse | Self::WheelStep | Self::RefreshSecs => "Interface",
            Self::DiffContext | Self::IgnoreWhitespace => "Diff",
            Self::SignOff => "Commit",
            Self::ShowReads => "Command log",
        }
    }

    #[must_use]
    pub const fn kind(self) -> Kind {
        match self {
            Self::Theme => Kind::Choice(&["Dark", "Light"]),
            Self::Accent => Kind::Choice(&["Green", "Blue", "Purple", "Amber"]),
            Self::WheelStep => Kind::Number { min: 1, max: 50 },
            Self::RefreshSecs => Kind::Number { min: 1, max: 3600 },
            Self::DiffContext => Kind::Number { min: 0, max: 200 },
            Self::Mouse | Self::IgnoreWhitespace | Self::SignOff | Self::ShowReads => Kind::Toggle,
        }
    }

    const fn section(self) -> Section {
        match self {
            Self::Theme | Self::Accent => Section::Theme,
            Self::Mouse | Self::WheelStep | Self::RefreshSecs => Section::Ui,
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
    pub save: SaveState,
}

/// Something only the run loop can do for a setting that changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalRequest {
    /// Switch the terminal's mouse capture on or off.
    Mouse(bool),
}

/// The step after `value` on `REFRESH_STEPS`, up or down, staying on the scale.
fn refresh_step(value: u64, up: bool) -> u64 {
    let next = if up {
        REFRESH_STEPS.iter().copied().find(|&s| s > value)
    } else {
        REFRESH_STEPS.iter().rev().copied().find(|&s| s < value)
    };
    // Past an end of the scale: stay on that end.
    next.unwrap_or(if up { 3600 } else { 1 })
}

/// `value` moved by one unit in `up`'s direction, clamped to `min..=max`.
fn stepped(value: u64, up: bool, min: u64, max: u64) -> u64 {
    if up {
        value.saturating_add(1).min(max)
    } else {
        value.saturating_sub(1).max(min)
    }
}

impl App {
    /// The sheet's state, for the screen that draws it and for tests.
    pub fn settings(&self) -> &SettingsSheet {
        &self.settings
    }

    /// The live configuration: what ferrit is using now, saved or not.
    #[doc(hidden)]
    pub fn live_config(&self) -> &Config {
        &self.config
    }

    /// The row's value when it is a choice: the index among its names. For the
    /// accent, `None` when a custom colour is set (none of the presets).
    #[must_use]
    pub fn choice_index(&self, row: SettingsRow) -> Option<usize> {
        match row {
            SettingsRow::Theme => Some(usize::from(self.theme_config.base == Base::Light)),
            SettingsRow::Accent => {
                if self.theme_config.accent.is_some() {
                    None
                } else {
                    Preset::ALL
                        .iter()
                        .position(|p| *p == self.theme_config.preset)
                }
            },
            _ => None,
        }
    }

    /// The row's value when it is a toggle.
    #[must_use]
    pub fn toggle_value(&self, row: SettingsRow) -> bool {
        match row {
            SettingsRow::Mouse => self.config.ui.mouse,
            SettingsRow::IgnoreWhitespace => self.config.diff.ignore_whitespace,
            SettingsRow::SignOff => self.config.commit.sign_off,
            SettingsRow::ShowReads => self.config.log.show_reads,
            _ => false,
        }
    }

    /// The row's value when it is a number.
    #[must_use]
    pub fn number_value(&self, row: SettingsRow) -> u64 {
        match row {
            SettingsRow::WheelStep => u64::from(self.config.ui.wheel_step),
            SettingsRow::RefreshSecs => self.config.ui.poll_secs,
            SettingsRow::DiffContext => u64::from(self.config.diff.context),
            _ => 0,
        }
    }

    /// Change `row` one step: `up` is `→` (or `Space`, which goes the same way).
    /// A toggle flips, a choice moves to the next or previous name (wrapping),
    /// a number moves by one unit (the refresh interval along its scale) and
    /// stops at its ends. The change applies at once and is saved at once.
    pub fn change_setting(&mut self, row: SettingsRow, up: bool) {
        match row {
            SettingsRow::Theme => {
                self.theme_config.base = match self.theme_config.base {
                    Base::Dark => Base::Light,
                    Base::Light => Base::Dark,
                };
                self.theme_changed();
            },
            SettingsRow::Accent => {
                self.theme_config.preset = if up {
                    self.theme_config.preset.next()
                } else {
                    self.theme_config.preset.prev()
                };
                self.theme_config.accent = None;
                self.sync_theme_picker_selection();
                self.theme_changed();
            },
            SettingsRow::Mouse => {
                self.config.ui.mouse = !self.config.ui.mouse;
                self.terminal_request = Some(TerminalRequest::Mouse(self.config.ui.mouse));
            },
            SettingsRow::WheelStep => {
                let value = stepped(self.number_value(row), up, 1, 50);
                self.config.ui.wheel_step = u8::try_from(value).unwrap_or(50);
            },
            SettingsRow::RefreshSecs => {
                self.config.ui.poll_secs = refresh_step(self.config.ui.poll_secs, up);
                self.poll_request = Some(Duration::from_secs(self.config.ui.poll_secs));
            },
            SettingsRow::DiffContext => {
                let value = stepped(self.number_value(row), up, 0, 200);
                self.config.diff.context = u32::try_from(value).unwrap_or(200);
                self.request_refresh();
            },
            SettingsRow::IgnoreWhitespace => {
                self.config.diff.ignore_whitespace = !self.config.diff.ignore_whitespace;
                self.request_refresh();
            },
            SettingsRow::SignOff => self.config.commit.sign_off = !self.config.commit.sign_off,
            SettingsRow::ShowReads => self.config.log.show_reads = !self.config.log.show_reads,
        }
        self.save_settings(row.section());
    }

    /// The theme in `theme_config` changed (base, preset, colour): rebuild what
    /// was drawn from it, and mirror it into the live config so a clone of the
    /// config (the app rebuilt on a new repository) carries it.
    pub(super) fn theme_changed(&mut self) {
        self.palette = self.theme_config.palette();
        self.rendered_diff = None;
        self.config.theme = self.theme_config.clone();
    }

    /// Write `section` of the live config to `config.toml`. No file (tests, a
    /// system with no config directory) is not an error: nothing is written. A
    /// file that cannot be written, or is not TOML, is reported in the sheet's
    /// footer and the setting still applies for this run.
    pub(super) fn save_settings(&mut self, section: Section) {
        let Some(path) = self.config_file.clone() else {
            self.settings.save = SaveState::Idle;
            return;
        };
        self.settings.save = match Config::save_sections(&path, &self.config, &[section]) {
            Ok(()) => SaveState::Saved,
            Err(error) => SaveState::Failed(error),
        };
    }

    /// What the run loop has to do for the last change, once.
    pub(crate) fn take_terminal_request(&mut self) -> Option<TerminalRequest> {
        self.terminal_request.take()
    }

    /// The new refresh interval the run loop has to give the poll thread, once.
    pub(crate) fn take_poll_request(&mut self) -> Option<Duration> {
        self.poll_request.take()
    }
}
