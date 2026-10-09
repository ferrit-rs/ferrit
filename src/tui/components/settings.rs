//! The settings sheet: its keys and clicks, the theme being edited, and how it is drawn.

use crate::config::settings::{
    Kind, SaveState, SettingsRow, SettingsSheet, TerminalRequest, stepped,
};
use crate::config::{Config, Section};
use crate::theme::color_picker::{
    self, ColorPicker, ColorPickerDisplay, PaletteDirection, grid_metrics, rgb,
};
use crate::theme::palette::Palette;
use crate::theme::scheme::ColorDepth;
use crate::theme::theme_config::{
    Preset, RGB_CHANNEL_COUNT, RGB_GREEN_CHANNEL, RGB_RED_CHANNEL, SchemeChoice, ThemeConfig,
    ThemeMode,
};
use crate::tui::App;
use crate::tui::draw::RenderedDiff;
use crate::tui::draw::{Landed, RenderState};
use crate::tui::event::Event;
use crate::tui::widgets::drawer::Drawer;
use crate::tui::widgets::scroll_bar::ScrollBar;
use crate::tui::widgets::separator::Separator;
use ratatui::Frame;
use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;
use std::path::Path;

const RGB_CHANNEL_STEP: i16 = 8;
const WHEEL_ROWS: usize = 3;

/// The settings sheet's state and the parts of the app it changes, borrowed for
/// as long as the sheet decides. It changes only these; what else follows (a
/// refresh, the sheet closing) it asks for in `events`.
pub(crate) struct Settings<'a> {
    /// The live configuration: what ferrit is using now, saved or not.
    pub(crate) config: &'a mut Config,
    /// The theme being edited.
    pub(crate) theme: &'a mut ThemeEditor,
    /// The colours everything is drawn with.
    pub(crate) palette: &'a mut Palette,
    /// The sheet's own state: the highlighted row and the footer.
    pub(crate) sheet: &'a mut SettingsSheet,
    /// The `config.toml` a change is saved to.
    pub(crate) file: Option<&'a Path>,
    /// What only the run loop can carry out for the last change.
    pub(crate) terminal_request: &'a mut Option<TerminalRequest>,
    /// The cached syntax-coloured diff, which a change of base makes stale.
    pub(crate) diff_cache: &'a mut Option<RenderedDiff>,
    /// What the change asks of the rest of the app.
    pub(crate) events: Vec<Event>,
}

/// The row's value when it is a choice: the index among its names. For the
/// accent, `None` when a custom colour is set (none of the presets).
pub(crate) fn choice_index(theme: &ThemeEditor, row: SettingsRow) -> Option<usize> {
    match row {
        SettingsRow::Theme => Some(match theme.config.effective_scheme() {
            SchemeChoice::Terminal => 0,
            SchemeChoice::Dark => 1,
            SchemeChoice::Light => 2,
        }),
        SettingsRow::Accent => {
            if theme.config.accent.is_some() {
                None
            } else {
                Preset::ALL.iter().position(|p| *p == theme.config.preset)
            }
        },
        _ => None,
    }
}

/// The row's value when it is a toggle.
pub(crate) const fn toggle_value(config: &Config, row: SettingsRow) -> bool {
    match row {
        SettingsRow::Mouse => config.ui.mouse,
        SettingsRow::IgnoreWhitespace => config.diff.ignore_whitespace,
        SettingsRow::SignOff => config.commit.sign_off,
        SettingsRow::ShowReads => config.log.show_reads,
        _ => false,
    }
}

/// The row's value when it is a number.
pub(crate) fn number_value(config: &Config, row: SettingsRow) -> u64 {
    match row {
        SettingsRow::WheelStep => u64::from(config.ui.wheel_step),
        SettingsRow::DiffContext => u64::from(config.diff.context),
        _ => 0,
    }
}

impl Settings<'_> {
    /// Change `row` one step: `up` is `→` (or `Space`, which goes the same way).
    /// A toggle flips, a choice moves to the next or previous name (wrapping),
    /// a number moves by one unit (the refresh interval along its scale) and
    /// stops at its ends. The change applies at once and is saved at once.
    pub(crate) fn change_setting(&mut self, row: SettingsRow, up: bool) {
        match row {
            SettingsRow::Theme => {
                let current = choice_index(self.theme, row).unwrap_or(0);
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
                self.config.ui.mouse = !self.config.ui.mouse;
                *self.terminal_request = Some(TerminalRequest::Mouse(self.config.ui.mouse));
            },
            SettingsRow::WheelStep => {
                let value = stepped(number_value(self.config, row), up, 1, 50);
                self.config.ui.wheel_step = u8::try_from(value).unwrap_or(50);
            },
            SettingsRow::DiffContext => {
                let value = stepped(number_value(self.config, row), up, 0, 200);
                self.config.diff.context = u32::try_from(value).unwrap_or(200);
                self.events.push(Event::Refresh);
            },
            SettingsRow::IgnoreWhitespace => {
                self.config.diff.ignore_whitespace = !self.config.diff.ignore_whitespace;
                self.events.push(Event::Refresh);
            },
            SettingsRow::SignOff => {
                self.config.commit.sign_off = !self.config.commit.sign_off;
            },
            SettingsRow::ShowReads => {
                self.config.log.show_reads = !self.config.log.show_reads;
            },
        }
        self.save_settings(row.section());
    }

    /// The theme in `theme_config` changed (base, preset, colour): rebuild what
    /// was drawn from it, and mirror it into the live config so a clone of the
    /// config (the app rebuilt on a new repository) carries it.
    pub(crate) fn theme_changed(&mut self) {
        let palette = self.theme.config.palette();
        // The cached diff holds syntax colours, which follow the base only: an
        // accent change must not make every click re-highlight the diff.
        if palette.light != self.palette.light {
            *self.diff_cache = None;
        }
        *self.palette = palette;
        self.config.theme = self.theme.config.clone();
    }

    /// Write `section` of the live config to `config.toml`. No file (tests, a
    /// system with no config directory) is not an error: nothing is written. A
    /// file that cannot be written, or is not TOML, is reported in the sheet's
    /// footer and the setting still applies for this run.
    pub(crate) fn save_settings(&mut self, section: Section) {
        let Some(path) = self.file else {
            self.sheet.save = SaveState::Idle;
            return;
        };
        self.sheet.save = match Config::save_sections(path, self.config, &[section]) {
            Ok(()) => SaveState::Saved,
            Err(error) => SaveState::Failed(error.to_string()),
        };
    }

    /// Set a choice row to the name at `index` (a click on a radio).
    pub(crate) fn set_choice(&mut self, row: SettingsRow, index: usize) {
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
            .get(self.sheet.selected)
            .copied()
            .unwrap_or(SettingsRow::Theme)
    }

    /// The sheet is about to open: back on the rows, at the top, the selected row
    /// scrolled into view.
    pub(crate) fn prepare(&mut self) {
        self.theme.mode = ThemeMode::Idle;
        self.sheet.scroll = 0;
        self.sheet.follow = true;
    }

    /// Every key while the sheet is up. It owns the keyboard: `↑` `↓` move
    /// between rows, `←` `→` and `Space` change the value, `Enter` opens the
    /// colour picker on the accent, `Esc` goes back (picker) or closes.
    pub(crate) fn key(&mut self, key: KeyEvent) {
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
            KeyCode::Esc => self.events.push(Event::CloseSheet),
            KeyCode::Up | KeyCode::Char('k') => {
                self.sheet.selected = self.sheet.selected.saturating_sub(1);
                self.sheet.follow = true;
            },
            KeyCode::Down | KeyCode::Char('j') => {
                self.sheet.selected = (self.sheet.selected + 1).min(last);
                self.sheet.follow = true;
            },
            KeyCode::Left | KeyCode::Char('h') => self.change_setting(row, false),
            KeyCode::Right | KeyCode::Char('l' | ' ') => self.change_setting(row, true),
            KeyCode::Enter if row == SettingsRow::Accent => {
                self.theme.mode = ThemeMode::Palette;
                self.sync_theme_picker_selection();
            },
            KeyCode::PageUp => {
                self.sheet.scroll = self.sheet.scroll.saturating_sub(10);
            },
            KeyCode::PageDown => {
                self.sheet.scroll = self.sheet.scroll.saturating_add(10);
            },
            KeyCode::Home | KeyCode::End => {
                self.sheet.selected = if key.code == KeyCode::Home { 0 } else { last };
                self.sheet.follow = true;
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
    pub(crate) fn mouse(&mut self, ev: MouseEvent, hits: &SettingsHits, overlay: Option<Rect>) {
        self.events.push(Event::HidePointer);
        let point = Position::new(ev.column, ev.row);
        match ev.kind {
            MouseEventKind::ScrollUp => {
                self.sheet.scroll = self.sheet.scroll.saturating_sub(WHEEL_ROWS);
            },
            MouseEventKind::ScrollDown => {
                self.sheet.scroll = self.sheet.scroll.saturating_add(WHEEL_ROWS);
            },
            MouseEventKind::Down(MouseButton::Left) => {
                let grid = hits.color_grid;
                if grid.contains(point) {
                    let metrics = grid_metrics(self.theme.picker_display);
                    let column = usize::from(ev.column.saturating_sub(grid.x)) / metrics.cell_width;
                    let row =
                        hits.color_grid_first_row + usize::from(ev.row.saturating_sub(grid.y));
                    self.select_theme_picker_cell(column, row);
                } else if let Some(&(_, row, click)) =
                    hits.parts.iter().find(|(area, ..)| area.contains(point))
                {
                    self.sheet.selected =
                        SettingsRow::ALL.iter().position(|r| *r == row).unwrap_or(0);
                    match click {
                        Click::Row => {},
                        Click::Choice(index) => self.set_choice(row, index),
                        Click::Flip => self.change_setting(row, true),
                        Click::Step(up) => self.change_setting(row, up),
                    }
                } else if !overlay.is_some_and(|rect| rect.contains(point)) {
                    self.events.push(Event::CloseSheet);
                }
            },
            _ => {},
        }
    }

    pub(crate) fn sync_theme_picker_selection(&mut self) {
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

pub struct ThemeEditor {
    /// The theme as chosen: what the screen is painted with.
    pub config: ThemeConfig,
    pub(crate) mode: ThemeMode,
    /// Which of red, green and blue `e` edits.
    pub(crate) rgb_channel: usize,
    /// The highlighted swatch of the picker.
    pub(crate) palette_selected: usize,
    pub(crate) picker_display: ColorPickerDisplay,
}

impl ThemeEditor {
    pub(crate) fn new(config: ThemeConfig) -> Self {
        let picker_display = ColorPickerDisplay::default();
        Self {
            palette_selected: color_picker::nearest_index(config.color(), picker_display),
            config,
            mode: ThemeMode::Idle,
            rgb_channel: RGB_RED_CHANNEL,
            picker_display,
        }
    }

    /// Put the highlight on the swatch nearest the current accent.
    pub(crate) fn sync_picker_selection(&mut self) {
        self.palette_selected =
            color_picker::nearest_index(self.config.color(), self.picker_display);
    }

    /// The highlighted swatch becomes the accent. `true` when it did.
    fn apply_picker_selection(&mut self) -> bool {
        let Some(color) = color_picker::color_at(self.picker_display, self.palette_selected) else {
            return false;
        };
        self.config.accent = Some(color);
        true
    }

    /// Move the highlight and take that swatch as the accent.
    pub(crate) fn move_palette(&mut self, direction: PaletteDirection) -> bool {
        self.palette_selected =
            color_picker::move_selection(self.palette_selected, direction, self.picker_display);
        self.apply_picker_selection()
    }

    /// Enter on the picker: take the highlighted swatch.
    pub(crate) fn pick_selected(&mut self) -> bool {
        self.apply_picker_selection()
    }

    /// A click on a swatch.
    pub(crate) fn select_cell(&mut self, column: usize, row: usize) -> bool {
        let Some(selected) = color_picker::selection_at(self.picker_display, column, row) else {
            return false;
        };
        self.mode = ThemeMode::Palette;
        self.palette_selected = selected;
        self.apply_picker_selection()
    }

    /// Palette grid or spectrum.
    pub(crate) fn toggle_picker_display(&mut self) {
        self.picker_display = match self.picker_display {
            ColorPickerDisplay::Palette => ColorPickerDisplay::Spectrum,
            ColorPickerDisplay::Spectrum => ColorPickerDisplay::Palette,
        };
        self.sync_picker_selection();
    }

    pub(crate) fn next_rgb_channel(&mut self) {
        self.rgb_channel = (self.rgb_channel + 1) % RGB_CHANNEL_COUNT;
    }

    /// Move the edited channel of the accent by `delta`, clamped to a byte.
    pub(crate) fn adjust_rgb(&mut self, delta: i16) {
        let (mut r, mut g, mut b) = rgb(self.config.color());
        let channel = match self.rgb_channel {
            RGB_RED_CHANNEL => &mut r,
            RGB_GREEN_CHANNEL => &mut g,
            _ => &mut b,
        };
        *channel = u8::try_from((i16::from(*channel) + delta).clamp(0, 255)).unwrap_or_default();
        self.config.accent = Some(Color::Rgb(r, g, b));
        self.sync_picker_selection();
    }
}

const LABEL_WIDTH: usize = 22;
const RGB_LABELS: [&str; 3] = ["R", "G", "B"];

/// A clickable stretch of a row line: its first cell, width and meaning.
struct Segment {
    x: u16,
    width: u16,
    row: SettingsRow,
    click: Click,
}

/// One row's line under construction: spans plus where each part sits.
struct RowLine {
    row: SettingsRow,
    spans: Vec<Span<'static>>,
    x: usize,
    segments: Vec<Segment>,
}

impl RowLine {
    fn text(&mut self, text: impl Into<String>, style: Style) {
        let text = text.into();
        self.x += Line::from(text.clone()).width();
        self.spans.push(Span::styled(text, style));
    }

    fn clickable(&mut self, text: impl Into<String>, style: Style, click: Click) {
        let text = text.into();
        let width = Line::from(text.clone()).width();
        self.segments.push(Segment {
            x: u16::try_from(self.x).unwrap_or(u16::MAX),
            width: u16::try_from(width).unwrap_or(u16::MAX),
            row: self.row,
            click,
        });
        self.text(text, style);
    }
}

/// The row's line: the marker, the label and the value with its click parts.
fn row_line(app: &App, row: SettingsRow, selected: bool, palette: &Palette) -> RowLine {
    let accent = app.theme.config.color();
    let mut line = RowLine {
        row,
        spans: Vec::new(),
        x: 0,
        segments: Vec::new(),
    };
    let label_style = if selected {
        Style::new().fg(accent).add_modifier(Modifier::BOLD)
    } else {
        Style::new()
    };
    line.text(if selected { "\u{25b8} " } else { "  " }, label_style);
    line.text(format!("{:<LABEL_WIDTH$}", row.label()), label_style);
    let idle = Style::new().fg(palette.idle);
    match row {
        SettingsRow::Theme => {
            let current = app.choice_index(row);
            let Kind::Choice(names) = row.kind() else {
                return line;
            };
            for (index, name) in names.iter().enumerate() {
                let on = current == Some(index);
                let mark = if on { "(\u{2022})" } else { "( )" };
                let style = if on { Style::new().fg(accent) } else { idle };
                line.clickable(format!("{mark} {name}"), style, Click::Choice(index));
                line.text("   ", idle);
            }
        },
        SettingsRow::Accent => {
            let current = app.choice_index(row);
            for (index, preset) in Preset::ALL.into_iter().enumerate() {
                let on = current == Some(index);
                let mark = if on { "\u{25cf}" } else { "\u{25cb}" };
                let style = Style::new().fg(preset.color());
                let style = if on {
                    style.add_modifier(Modifier::BOLD)
                } else {
                    style
                };
                line.clickable(
                    format!("{mark} {}", preset.name()),
                    style,
                    Click::Choice(index),
                );
                line.text("  ", idle);
            }
            if current.is_none() {
                line.text("\u{25a0} custom", Style::new().fg(accent));
            }
        },
        SettingsRow::Mouse
        | SettingsRow::IgnoreWhitespace
        | SettingsRow::SignOff
        | SettingsRow::ShowReads => {
            let on = app.toggle_value(row);
            let style = if on { Style::new().fg(accent) } else { idle };
            line.clickable(if on { "[x]" } else { "[ ]" }, style, Click::Flip);
        },
        SettingsRow::WheelStep | SettingsRow::DiffContext => {
            let value = app.number_value(row);
            let shown = value.to_string();
            line.clickable("\u{2039}", idle, Click::Step(false));
            line.text(format!(" {shown} "), Style::new());
            line.clickable("\u{203a}", idle, Click::Step(true));
        },
    }
    line
}

fn footer(app: &App, palette: &Palette) -> Line<'static> {
    let idle = Style::new().fg(palette.idle);
    let path = app.prefs.file.as_ref().map(|p| p.display().to_string());
    let line = match (&app.settings().save, path) {
        (SaveState::Failed(why), _) => {
            return Line::styled(format!("Not saved: {why}"), Style::new().fg(palette.warn));
        },
        (_, None) => "No config file: changes last for this run only".to_owned(),
        (SaveState::Saved, Some(path)) => format!("Saved \u{b7} {path}"),
        (SaveState::Idle, Some(path)) => format!("Saved as you change it \u{b7} {path}"),
    };
    let line = if app.prefs.config.ui.mouse {
        line
    } else {
        format!("Mouse is off: keyboard only \u{b7} {line}")
    };
    let line = if app.prefs.color_depth == ColorDepth::TrueColor {
        line
    } else {
        format!("256 colours: approximated \u{b7} {line}")
    };
    Line::styled(line, idle)
}

fn hint(app: &App) -> &'static str {
    match app.theme.mode {
        ThemeMode::Idle => {
            "\u{2191}\u{2193} row \u{b7} \u{2190}\u{2192} or Space change \u{b7} Enter picker (Accent) \u{b7} Esc close"
        },
        ThemeMode::Palette => {
            "arrows colour \u{b7} v view \u{b7} e RGB \u{b7} Enter apply \u{b7} Esc back"
        },
        ThemeMode::EditingRgb => "Tab channel \u{b7} arrows change \u{b7} Esc back",
    }
}

/// Draw the sheet and record where its clickable parts landed (nothing
/// clickable when it is not on screen).
pub(crate) fn draw(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &App,
    palette: &Palette,
    render: &mut RenderState,
    landed: &mut Landed,
) {
    let accent = app.theme.config.color();
    let selected_row = app.settings().selected;
    let Some(inner) = Drawer::new(&mut render.sheet, " Settings ")
        .width(Constraint::Percentage(75))
        .border_style(Style::new().fg(accent))
        .render(frame, area)
    else {
        landed.settings_hits = Some(SettingsHits::default());
        return;
    };
    let [content, foot] =
        Layout::vertical([Constraint::Min(0), Constraint::Length(2)]).areas(inner);
    let [body, track] =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(1)]).areas(content);

    let divider = |label: &str| {
        Separator::new(label)
            .style(Style::new().fg(palette.idle))
            .margin_x(1)
            .line(body.width)
    };
    let mut lines: Vec<Line<'static>> = Vec::new();
    let mut segments: Vec<(usize, Segment)> = Vec::new();
    let mut whole_rows: Vec<(usize, SettingsRow)> = Vec::new();
    let mut selected_line = 0;
    let mut grid_line = 0;
    let mut last_group = "";
    for (index, row) in SettingsRow::ALL.into_iter().enumerate() {
        if row.group() != last_group {
            if !lines.is_empty() {
                lines.push(Line::default());
            }
            lines.push(divider(row.group()));
            last_group = row.group();
        }
        let built = row_line(app, row, index == selected_row, palette);
        if index == selected_row {
            selected_line = lines.len();
        }
        let at = lines.len();
        whole_rows.push((at, row));
        segments.extend(built.segments.into_iter().map(|s| (at, s)));
        lines.push(Line::from(built.spans));
        if row == SettingsRow::Accent {
            grid_line = lines.len() + 1;
            lines.extend(
                ColorPicker::new(accent)
                    .selected(app.theme.palette_selected)
                    .active(app.theme.mode == ThemeMode::Palette)
                    .display(app.theme.picker_display)
                    .lines(),
            );
            let (r, g, b) = rgb(accent);
            let channel = RGB_LABELS
                .get(app.theme.rgb_channel)
                .copied()
                .unwrap_or("B");
            lines.push(Line::styled(
                if app.theme.mode == ThemeMode::EditingRgb {
                    format!("RGB: [R {r:03}] [G {g:03}] [B {b:03}] \u{b7} channel {channel}")
                } else {
                    "Enter opens the picker \u{b7} click a colour".to_owned()
                },
                Style::new().fg(palette.idle),
            ));
        }
    }

    let viewport = usize::from(body.height);
    let max_scroll = lines.len().saturating_sub(viewport);
    // Keep the selected row in view when it was just moved (`follow`), then keep
    // the offset inside the page. The result is this frame's scroll, and what
    // `App::land` stores for the next.
    let followed = app.sheets.settings.follow;
    let mut scroll = app.sheets.settings.scroll;
    if followed {
        if selected_line < scroll {
            scroll = selected_line;
        } else if viewport > 0 && selected_line >= scroll + viewport {
            scroll = selected_line + 1 - viewport;
        }
    }
    let scroll = scroll.min(max_scroll);
    landed.settings_scroll = Some((scroll, followed));

    let on_screen = |line: usize| -> Option<u16> {
        (scroll..scroll + viewport)
            .contains(&line)
            .then(|| body.y + u16::try_from(line - scroll).unwrap_or(u16::MAX))
    };
    let mut hits = SettingsHits::default();
    let grid_rows = grid_metrics(app.theme.picker_display).rows;
    let first = grid_line.max(scroll);
    let last = (grid_line + grid_rows).min(scroll + viewport);
    if first < last {
        hits.color_grid = Rect::new(
            body.x,
            body.y + u16::try_from(first - scroll).unwrap_or(u16::MAX),
            body.width,
            u16::try_from(last - first).unwrap_or(u16::MAX),
        );
        hits.color_grid_first_row = first - grid_line;
    }
    for (line, segment) in &segments {
        if let Some(y) = on_screen(*line) {
            let x = body.x.saturating_add(segment.x);
            let width = segment.width.min(body.right().saturating_sub(x));
            hits.parts
                .push((Rect::new(x, y, width, 1), segment.row, segment.click));
        }
    }
    for (line, row) in whole_rows {
        if let Some(y) = on_screen(line) {
            hits.parts
                .push((Rect::new(body.x, y, body.width, 1), row, Click::Row));
        }
    }
    landed.settings_hits = Some(hits);

    frame.render_widget(
        Paragraph::new(lines).scroll((u16::try_from(scroll).unwrap_or(u16::MAX), 0)),
        body,
    );
    ScrollBar::new(max_scroll + viewport, viewport, scroll)
        .style(Style::new().fg(palette.idle))
        .render(frame, track);
    let footer_lines = vec![
        footer(app, palette),
        Line::styled(hint(app), Style::new().fg(palette.idle)),
    ];
    frame.render_widget(Paragraph::new(footer_lines), foot);
}
