//! The settings sheet: the drawer that opens on a click on the author's name
//! (`docs/PLAN_17_SETTINGS.md`). Ferrit's own settings only; the keys and the
//! clicks are in `app::settings`.

use crate::app::App;
use crate::app::settings::{Click, Kind, SaveState, SettingsHits, SettingsRow};
use crate::app::theme_config::{Preset, ThemeMode};
use crate::components::ui::color_picker::{ColorPicker, grid_metrics, rgb};
use crate::components::ui::drawer::Drawer;
use crate::components::ui::palette::Palette;
use crate::components::ui::scheme::ColorDepth;
use crate::components::ui::scroll_bar::ScrollBar;
use crate::components::ui::separator::Separator;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

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
    let accent = app.theme_config.color();
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
                    style
                } else {
                    style.add_modifier(Modifier::DIM)
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
    let path = app.config_file.as_ref().map(|p| p.display().to_string());
    let line = match (&app.settings().save, path) {
        (SaveState::Failed(why), _) => {
            return Line::styled(format!("Not saved: {why}"), Style::new().fg(palette.warn));
        },
        (_, None) => "No config file: changes last for this run only".to_owned(),
        (SaveState::Saved, Some(path)) => format!("Saved \u{b7} {path}"),
        (SaveState::Idle, Some(path)) => format!("Saved as you change it \u{b7} {path}"),
    };
    let line = if app.config.ui.mouse {
        line
    } else {
        format!("Mouse is off: keyboard only \u{b7} {line}")
    };
    let line = if app.color_depth == ColorDepth::TrueColor {
        line
    } else {
        format!("256 colours: approximated \u{b7} {line}")
    };
    Line::styled(line, idle)
}

fn hint(app: &App) -> &'static str {
    match app.theme_mode {
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
pub(super) fn draw(frame: &mut Frame<'_>, area: Rect, app: &mut App, palette: &Palette) {
    let accent = app.theme_config.color();
    let selected_row = app.settings().selected;
    let Some(inner) = Drawer::new(&mut app.author_overlay, " Settings ")
        .width(Constraint::Percentage(75))
        .border_style(Style::new().fg(accent))
        .render(frame, area)
    else {
        app.settings_hits = SettingsHits::default();
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
                    .selected(app.theme_palette_selected)
                    .active(app.theme_mode == ThemeMode::Palette)
                    .display(app.theme_picker_display)
                    .lines(),
            );
            let (r, g, b) = rgb(accent);
            let channel = RGB_LABELS
                .get(app.theme_rgb_channel)
                .copied()
                .unwrap_or("B");
            lines.push(Line::styled(
                if app.theme_mode == ThemeMode::EditingRgb {
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
    if app.settings.follow {
        app.settings.follow = false;
        if selected_line < app.settings_scroll {
            app.settings_scroll = selected_line;
        } else if viewport > 0 && selected_line >= app.settings_scroll + viewport {
            app.settings_scroll = selected_line + 1 - viewport;
        }
    }
    app.settings_scroll = app.settings_scroll.min(max_scroll);
    let scroll = app.settings_scroll;

    let on_screen = |line: usize| -> Option<u16> {
        (scroll..scroll + viewport)
            .contains(&line)
            .then(|| body.y + u16::try_from(line - scroll).unwrap_or(u16::MAX))
    };
    let mut hits = SettingsHits::default();
    let grid_rows = grid_metrics(app.theme_picker_display).rows;
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
    app.settings_hits = hits;

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
