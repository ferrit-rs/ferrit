//! Compact palette and hex preview for choosing terminal accent colors.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

const PALETTE_COLUMNS: usize = 8;
const PALETTE_ROWS: usize = 2;
const PALETTE_LENGTH: usize = PALETTE_COLUMNS * PALETTE_ROWS;
const FIRST_PALETTE_INDEX: usize = 0;
const PALETTE_INDEX_STEP: usize = 1;
const SWATCH_LABEL_WIDTH: usize = 8;
const HEX_PREVIEW_LINES: usize = 1;
const PALETTE: [(&str, Color); PALETTE_LENGTH] = [
    ("Slate", Color::Rgb(100, 116, 139)),
    ("Gray", Color::Rgb(107, 114, 128)),
    ("Zinc", Color::Rgb(113, 113, 122)),
    ("Red", Color::Rgb(239, 68, 68)),
    ("Orange", Color::Rgb(249, 115, 22)),
    ("Amber", Color::Rgb(245, 158, 11)),
    ("Yellow", Color::Rgb(234, 179, 8)),
    ("Lime", Color::Rgb(132, 204, 22)),
    ("Green", Color::Rgb(34, 197, 94)),
    ("Emerald", Color::Rgb(16, 185, 129)),
    ("Teal", Color::Rgb(20, 184, 166)),
    ("Cyan", Color::Rgb(6, 182, 212)),
    ("Sky", Color::Rgb(14, 165, 233)),
    ("Blue", Color::Rgb(59, 130, 246)),
    ("Violet", Color::Rgb(139, 92, 246)),
    ("Pink", Color::Rgb(236, 72, 153)),
];

/// Stateless color picker presentation. Parent owns selection and input.
pub struct ColorPicker {
    value: Color,
    selected: usize,
    active: bool,
}

impl ColorPicker {
    pub fn new(value: Color) -> Self {
        Self {
            value,
            selected: nearest_palette_index(value),
            active: false,
        }
    }

    pub fn selected(mut self, selected: usize) -> Self {
        self.selected = selected.min(PALETTE_LENGTH - 1);
        self
    }

    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    pub fn palette() -> &'static [(&'static str, Color)] {
        &PALETTE
    }

    pub fn lines(self) -> Vec<Line<'static>> {
        let mut lines = Vec::with_capacity(PALETTE_ROWS + HEX_PREVIEW_LINES);
        for row in 0..PALETTE_ROWS {
            let mut spans = Vec::new();
            for col in 0..PALETTE_COLUMNS {
                let index = row * PALETTE_COLUMNS + col;
                let (name, color) = PALETTE[index];
                let selected = self.active && index == self.selected;
                let marker = if selected { "▣" } else { "●" };
                let style = Style::new().fg(color).add_modifier(if selected {
                    Modifier::BOLD | Modifier::UNDERLINED
                } else {
                    Modifier::empty()
                });
                spans.push(Span::styled(format!("{marker} "), style));
                spans.push(Span::styled(
                    format!("{name:<SWATCH_LABEL_WIDTH$}"),
                    if selected {
                        Style::new().fg(color).add_modifier(Modifier::BOLD)
                    } else {
                        Style::new().fg(Color::Gray)
                    },
                ));
            }
            lines.push(Line::from(spans));
        }
        let (r, g, b) = rgb(self.value);
        lines.push(Line::styled(
            format!("Hex  #{r:02X}{g:02X}{b:02X}"),
            Style::new().fg(self.value).add_modifier(Modifier::BOLD),
        ));
        lines
    }
}

pub fn rgb(color: Color) -> (u8, u8, u8) {
    match color {
        Color::Rgb(r, g, b) => (r, g, b),
        Color::Green => (0, 255, 0),
        Color::Cyan => (0, 255, 255),
        Color::Magenta => (255, 0, 255),
        Color::Yellow => (255, 255, 0),
        Color::Red => (255, 0, 0),
        Color::Blue => (0, 0, 255),
        _ => (0, 255, 0),
    }
}

pub fn nearest_palette_index(color: Color) -> usize {
    let (r, g, b) = rgb(color);
    PALETTE
        .iter()
        .enumerate()
        .min_by_key(|(_, (_, candidate))| {
            let (cr, cg, cb) = rgb(*candidate);
            i32::from(r).abs_diff(i32::from(cr)).pow(2)
                + i32::from(g).abs_diff(i32::from(cg)).pow(2)
                + i32::from(b).abs_diff(i32::from(cb)).pow(2)
        })
        .map_or(FIRST_PALETTE_INDEX, |(index, _)| index)
}

#[derive(Debug, Clone, Copy)]
pub enum PaletteDirection {
    Left,
    Right,
    Up,
    Down,
}

pub fn move_palette_selection(selected: usize, direction: PaletteDirection) -> usize {
    let selected = selected.min(PALETTE_LENGTH - 1);
    match direction {
        PaletteDirection::Left => selected.saturating_sub(PALETTE_INDEX_STEP),
        PaletteDirection::Right => (selected + PALETTE_INDEX_STEP).min(PALETTE_LENGTH - 1),
        PaletteDirection::Up => selected.saturating_sub(PALETTE_COLUMNS),
        PaletteDirection::Down => (selected + PALETTE_COLUMNS).min(PALETTE_LENGTH - 1),
    }
}

pub fn palette_color(selected: usize) -> Option<Color> {
    PALETTE.get(selected).map(|(_, color)| *color)
}
