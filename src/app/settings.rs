//! The settings sheet's rows and what changing one does
//! (`docs/PLAN_17_SETTINGS.md`): ferrit's own settings, nothing of git's. A
//! change applies at once to the live `Config`, and is saved at once to
//! `config.toml` (only its own section, through `Config::save_sections`), so the
//! next start finds the sheet as it was left.

use ratatui::crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::{Position, Rect};

use super::config::{Config, Section};
use super::theme_config::{Preset, SchemeChoice, ThemeMode};
use super::{App, KeyModifiers};
use crate::components::ui::color_picker::{self, PaletteDirection};

const RGB_CHANNEL_STEP: i16 = 8;
const WHEEL_ROWS: usize = 3;

/// One line of the sheet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    pub const ALL: [Self; 8] = [
        Self::Theme,
        Self::Accent,
        Self::Mouse,
        Self::WheelStep,
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

    const fn section(self) -> Section {
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

/// What a click on a part of a row does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Click {
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
pub struct SettingsHits {
    pub color_grid: Rect,
    pub color_grid_first_row: usize,
    /// Narrowest last: a part of a row comes before the row's whole line.
    pub parts: Vec<(Rect, SettingsRow, Click)>,
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
        &self.sheets.settings
    }

    /// The live configuration: what ferrit is using now, saved or not.
    #[doc(hidden)]
    pub fn live_config(&self) -> &Config {
        &self.prefs.config
    }

    /// The row's value when it is a choice: the index among its names. For the
    /// accent, `None` when a custom colour is set (none of the presets).
    #[must_use]
    pub fn choice_index(&self, row: SettingsRow) -> Option<usize> {
        match row {
            SettingsRow::Theme => Some(match self.theme.config.effective_scheme() {
                SchemeChoice::Terminal => 0,
                SchemeChoice::Dark => 1,
                SchemeChoice::Light => 2,
            }),
            SettingsRow::Accent => {
                if self.theme.config.accent.is_some() {
                    None
                } else {
                    Preset::ALL
                        .iter()
                        .position(|p| *p == self.theme.config.preset)
                }
            },
            _ => None,
        }
    }

    /// The row's value when it is a toggle.
    #[must_use]
    pub fn toggle_value(&self, row: SettingsRow) -> bool {
        match row {
            SettingsRow::Mouse => self.prefs.config.ui.mouse,
            SettingsRow::IgnoreWhitespace => self.prefs.config.diff.ignore_whitespace,
            SettingsRow::SignOff => self.prefs.config.commit.sign_off,
            SettingsRow::ShowReads => self.prefs.config.log.show_reads,
            _ => false,
        }
    }

    /// The row's value when it is a number.
    #[must_use]
    pub fn number_value(&self, row: SettingsRow) -> u64 {
        match row {
            SettingsRow::WheelStep => u64::from(self.prefs.config.ui.wheel_step),
            SettingsRow::DiffContext => u64::from(self.prefs.config.diff.context),
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
                let current = self.choice_index(row).unwrap_or(0);
                let next = if up { current + 1 } else { current + 2 } % 3;
                // `set_choice` applies and saves it.
                self.set_choice(row, next);
                return;
            },
            SettingsRow::Accent => {
                self.theme.config.preset = if up {
                    self.theme.config.preset.next()
                } else {
                    self.theme.config.preset.prev()
                };
                self.theme.config.accent = None;
                self.sync_theme_picker_selection();
                self.theme_changed();
            },
            SettingsRow::Mouse => {
                self.prefs.config.ui.mouse = !self.prefs.config.ui.mouse;
                self.terminal_request = Some(TerminalRequest::Mouse(self.prefs.config.ui.mouse));
            },
            SettingsRow::WheelStep => {
                let value = stepped(self.number_value(row), up, 1, 50);
                self.prefs.config.ui.wheel_step = u8::try_from(value).unwrap_or(50);
            },
            SettingsRow::DiffContext => {
                let value = stepped(self.number_value(row), up, 0, 200);
                self.prefs.config.diff.context = u32::try_from(value).unwrap_or(200);
                self.request_refresh();
            },
            SettingsRow::IgnoreWhitespace => {
                self.prefs.config.diff.ignore_whitespace =
                    !self.prefs.config.diff.ignore_whitespace;
                self.request_refresh();
            },
            SettingsRow::SignOff => {
                self.prefs.config.commit.sign_off = !self.prefs.config.commit.sign_off;
            },
            SettingsRow::ShowReads => {
                self.prefs.config.log.show_reads = !self.prefs.config.log.show_reads;
            },
        }
        self.save_settings(row.section());
    }

    /// The theme in `theme_config` changed (base, preset, colour): rebuild what
    /// was drawn from it, and mirror it into the live config so a clone of the
    /// config (the app rebuilt on a new repository) carries it.
    pub(super) fn theme_changed(&mut self) {
        let palette = self.theme.config.palette();
        // The cached diff holds syntax colours, which follow the base only: an
        // accent change must not make every click re-highlight the diff.
        if palette.light != self.prefs.palette.light {
            self.right.rendered = None;
        }
        self.prefs.palette = palette;
        self.prefs.config.theme = self.theme.config.clone();
    }

    /// Write `section` of the live config to `config.toml`. No file (tests, a
    /// system with no config directory) is not an error: nothing is written. A
    /// file that cannot be written, or is not TOML, is reported in the sheet's
    /// footer and the setting still applies for this run.
    pub(super) fn save_settings(&mut self, section: Section) {
        let Some(path) = self.prefs.file.clone() else {
            self.sheets.settings.save = SaveState::Idle;
            return;
        };
        self.sheets.settings.save =
            match Config::save_sections(&path, &self.prefs.config, &[section]) {
                Ok(()) => SaveState::Saved,
                Err(error) => SaveState::Failed(error.to_string()),
            };
    }

    /// What the run loop has to do for the last change, once.
    pub(crate) fn take_terminal_request(&mut self) -> Option<TerminalRequest> {
        self.terminal_request.take()
    }

    /// Set a choice row to the name at `index` (a click on a radio).
    pub fn set_choice(&mut self, row: SettingsRow, index: usize) {
        match row {
            SettingsRow::Theme => {
                self.theme.config.scheme = Some(match index {
                    0 => SchemeChoice::Terminal,
                    1 => SchemeChoice::Dark,
                    _ => SchemeChoice::Light,
                });
                self.accent_changed();
            },
            SettingsRow::Accent => {
                if let Some(preset) = Preset::ALL.get(index) {
                    self.theme.config.preset = *preset;
                    self.theme.config.accent = None;
                    self.sync_theme_picker_selection();
                    self.accent_changed();
                }
            },
            _ => {},
        }
    }

    /// The accent (preset or picked colour) changed: apply and save it.
    fn accent_changed(&mut self) {
        self.theme_changed();
        self.save_settings(Section::Theme);
    }

    fn selected_row(&self) -> SettingsRow {
        SettingsRow::ALL
            .get(self.sheets.settings.selected)
            .copied()
            .unwrap_or(SettingsRow::Theme)
    }

    /// The sheet is about to open: back on the rows, at the top, the selected row
    /// scrolled into view.
    pub(super) fn prepare_settings_sheet(&mut self) {
        self.theme.mode = ThemeMode::Idle;
        self.sheets.settings.scroll = 0;
        self.sheets.settings.follow = true;
    }

    /// Every key while the sheet is up. It owns the keyboard: `↑` `↓` move
    /// between rows, `←` `→` and `Space` change the value, `Enter` opens the
    /// colour picker on the accent, `Esc` goes back (picker) or closes.
    pub(super) fn settings_key(&mut self, key: KeyEvent) {
        match self.theme.mode {
            ThemeMode::Palette => self.picker_key(key),
            ThemeMode::EditingRgb => self.rgb_key(key),
            ThemeMode::Idle => self.row_key(key),
        }
    }

    fn row_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) {
            return;
        }
        let last = SettingsRow::ALL.len() - 1;
        let row = self.selected_row();
        match key.code {
            KeyCode::Esc => self.close_sheet(),
            KeyCode::Up | KeyCode::Char('k') => {
                self.sheets.settings.selected = self.sheets.settings.selected.saturating_sub(1);
                self.sheets.settings.follow = true;
            },
            KeyCode::Down | KeyCode::Char('j') => {
                self.sheets.settings.selected = (self.sheets.settings.selected + 1).min(last);
                self.sheets.settings.follow = true;
            },
            KeyCode::Left | KeyCode::Char('h') => self.change_setting(row, false),
            KeyCode::Right | KeyCode::Char('l' | ' ') => self.change_setting(row, true),
            KeyCode::Enter if row == SettingsRow::Accent => {
                self.theme.mode = ThemeMode::Palette;
                self.sync_theme_picker_selection();
            },
            KeyCode::PageUp => {
                self.sheets.settings.scroll = self.sheets.settings.scroll.saturating_sub(10);
            },
            KeyCode::PageDown => {
                self.sheets.settings.scroll = self.sheets.settings.scroll.saturating_add(10);
            },
            KeyCode::Home | KeyCode::End => {
                self.sheets.settings.selected = if key.code == KeyCode::Home { 0 } else { last };
                self.sheets.settings.follow = true;
            },
            _ => {},
        }
    }

    fn picker_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc => self.theme.mode = ThemeMode::Idle,
            KeyCode::Left | KeyCode::Char('h') => self.move_theme_palette(PaletteDirection::Left),
            KeyCode::Right | KeyCode::Char('l') => self.move_theme_palette(PaletteDirection::Right),
            KeyCode::Up | KeyCode::Char('k') => self.move_theme_palette(PaletteDirection::Up),
            KeyCode::Down | KeyCode::Char('j') => self.move_theme_palette(PaletteDirection::Down),
            KeyCode::Char('v') => self.theme.toggle_picker_display(),
            KeyCode::Char('e') => self.theme.mode = ThemeMode::EditingRgb,
            KeyCode::Enter => self.apply_theme_picker_selection(),
            _ => {},
        }
    }

    fn rgb_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Tab => {
                self.theme.next_rgb_channel();
            },
            KeyCode::Up | KeyCode::Right => self.adjust_theme_rgb(RGB_CHANNEL_STEP),
            KeyCode::Down | KeyCode::Left => self.adjust_theme_rgb(-RGB_CHANNEL_STEP),
            KeyCode::Esc => self.theme.mode = ThemeMode::Palette,
            _ => {},
        }
    }

    /// The mouse while the sheet is up: a click sets the value it lands on, the
    /// wheel scrolls, a click outside closes it.
    pub(super) fn settings_mouse(&mut self, ev: MouseEvent) {
        self.mouse_pointer.request(false);
        let point = Position::new(ev.column, ev.row);
        match ev.kind {
            MouseEventKind::ScrollUp => {
                self.sheets.settings.scroll =
                    self.sheets.settings.scroll.saturating_sub(WHEEL_ROWS);
            },
            MouseEventKind::ScrollDown => {
                self.sheets.settings.scroll =
                    self.sheets.settings.scroll.saturating_add(WHEEL_ROWS);
            },
            MouseEventKind::Down(MouseButton::Left) => {
                let grid = self.hits.settings.color_grid;
                if grid.contains(point) {
                    let metrics = color_picker::grid_metrics(self.theme.picker_display);
                    let column = usize::from(ev.column.saturating_sub(grid.x)) / metrics.cell_width;
                    let row = self.hits.settings.color_grid_first_row
                        + usize::from(ev.row.saturating_sub(grid.y));
                    self.select_theme_picker_cell(column, row);
                } else if let Some(&(_, row, click)) = self
                    .hits
                    .settings
                    .parts
                    .iter()
                    .find(|(area, ..)| area.contains(point))
                {
                    self.sheets.settings.selected =
                        SettingsRow::ALL.iter().position(|r| *r == row).unwrap_or(0);
                    match click {
                        Click::Row => {},
                        Click::Choice(index) => self.set_choice(row, index),
                        Click::Flip => self.change_setting(row, true),
                        Click::Step(up) => self.change_setting(row, up),
                    }
                } else if !self
                    .render
                    .sheet
                    .overlay_rect()
                    .is_some_and(|rect| rect.contains(point))
                {
                    self.close_sheet();
                }
            },
            _ => {},
        }
    }

    pub(super) fn sync_theme_picker_selection(&mut self) {
        self.theme.sync_picker_selection();
    }

    fn move_theme_palette(&mut self, direction: PaletteDirection) {
        if self.theme.move_palette(direction) {
            self.accent_changed();
        }
    }

    /// The highlighted swatch becomes the accent, applied and saved.
    fn apply_theme_picker_selection(&mut self) {
        if self.theme.pick_selected() {
            self.accent_changed();
        }
    }

    fn select_theme_picker_cell(&mut self, column: usize, row: usize) {
        if self.theme.select_cell(column, row) {
            self.accent_changed();
        }
    }

    fn adjust_theme_rgb(&mut self, delta: i16) {
        self.theme.adjust_rgb(delta);
        self.accent_changed();
    }
}
