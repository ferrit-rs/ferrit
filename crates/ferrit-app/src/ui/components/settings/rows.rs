//! Settings row projection and hit targets.

use crate::ui::components::settings::state::{Kind, SettingsRow};
use crate::ui::scene::Scene;
use ferrit_tui::theme::palette::Palette;
use ferrit_tui::theme::theme_config::Preset;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use strum::IntoEnumIterator;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Click {
    Row,
    Choice(usize),
    Flip,
    Step(bool),
}

pub(crate) struct Segment {
    pub(crate) x: u16,
    pub(crate) width: u16,
    pub(crate) row: SettingsRow,
    pub(crate) click: Click,
}

pub(crate) struct RowLine {
    pub(crate) row: SettingsRow,
    pub(crate) spans: Vec<Span<'static>>,
    pub(crate) segments: Vec<Segment>,
    x: usize,
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

pub(crate) fn row_line(
    app: &Scene<'_>,
    row: SettingsRow,
    selected: bool,
    palette: &Palette,
) -> RowLine {
    const LABEL_WIDTH: usize = 22;
    let accent = app.theme.config.color();
    let mut line = RowLine {
        row,
        spans: Vec::new(),
        segments: Vec::new(),
        x: 0,
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
            for (index, preset) in Preset::iter().enumerate() {
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
