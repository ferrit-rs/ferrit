//! Composable color picker views and selected-color preview.

#[path = "color_picker_spectrum.rs"]
pub mod color_picker_spectrum;

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use self::color_picker_spectrum::ColorPickerSpectrum;

const PALETTE_COLUMNS: usize = 8;
const PALETTE_ROWS: usize = 2;
const PALETTE_LENGTH: usize = PALETTE_COLUMNS * PALETTE_ROWS;
const SWATCH_LABEL_WIDTH: usize = 8;
const SWATCH_MARKER_WIDTH: usize = 2;
const PALETTE_CELL_WIDTH: usize = SWATCH_LABEL_WIDTH + SWATCH_MARKER_WIDTH;
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ColorPickerDisplay {
    Palette,
    #[default]
    Spectrum,
}

#[derive(Debug, Clone, Copy)]
pub struct ColorPickerGridMetrics {
    pub columns: usize,
    pub rows: usize,
    pub cell_width: usize,
}

#[derive(Debug, Clone, Copy)]
pub enum PaletteDirection {
    Left,
    Right,
    Up,
    Down,
}

/// Composite picker. Parent owns selection and applies colors returned by its input handler.
pub struct ColorPicker {
    value: Color,
    selected: usize,
    active: bool,
    display: ColorPickerDisplay,
}

impl ColorPicker {
    pub fn new(value: Color) -> Self {
        Self {
            value,
            selected: nearest_index(value, ColorPickerDisplay::Palette),
            active: false,
            display: ColorPickerDisplay::Palette,
        }
    }

    pub fn selected(mut self, selected: usize) -> Self {
        self.selected = selected;
        self
    }

    pub fn active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    pub fn display(mut self, display: ColorPickerDisplay) -> Self {
        self.display = display;
        self
    }

    pub fn lines(self) -> Vec<Line<'static>> {
        let mut lines = vec![Line::styled(
            "Pick a color",
            Style::new().add_modifier(Modifier::BOLD),
        )];
        match self.display {
            ColorPickerDisplay::Palette => {
                lines.extend(ColorPickerPalette::new(self.selected, self.active).lines());
            },
            ColorPickerDisplay::Spectrum => {
                lines.extend(ColorPickerSpectrum::new(self.selected, self.active).lines());
            },
        }
        lines.extend(ColorPickerPreview::new(self.value).lines());
        lines
    }
}

/// Named swatch view, reusable without the composite picker.
pub struct ColorPickerPalette {
    selected: usize,
    active: bool,
}

impl ColorPickerPalette {
    pub fn new(selected: usize, active: bool) -> Self {
        Self { selected, active }
    }

    pub fn lines(self) -> Vec<Line<'static>> {
        let mut lines = Vec::with_capacity(PALETTE_ROWS);
        for row in 0..PALETTE_ROWS {
            let mut spans = Vec::new();
            for col in 0..PALETTE_COLUMNS {
                let index = row * PALETTE_COLUMNS + col;
                let (name, color) = PALETTE[index];
                let selected = index == self.selected;
                spans.push(Span::styled(
                    if selected { "▣ " } else { "● " },
                    Style::new().fg(color).add_modifier(if selected {
                        if self.active {
                            Modifier::BOLD | Modifier::UNDERLINED
                        } else {
                            Modifier::BOLD
                        }
                    } else {
                        Modifier::empty()
                    }),
                ));
                spans.push(Span::styled(
                    format!("{name:<SWATCH_LABEL_WIDTH$}"),
                    if selected {
                        Style::new().fg(color).add_modifier(if self.active {
                            Modifier::BOLD | Modifier::UNDERLINED
                        } else {
                            Modifier::BOLD
                        })
                    } else {
                        Style::new().fg(Color::Gray)
                    },
                ));
            }
            lines.push(Line::from(spans));
        }
        lines
    }
}

/// Displays the current value as a color chip and hexadecimal code.
pub struct ColorPickerPreview {
    value: Color,
}

impl ColorPickerPreview {
    pub fn new(value: Color) -> Self {
        Self { value }
    }

    pub fn lines(self) -> Vec<Line<'static>> {
        let (r, g, b) = rgb(self.value);
        vec![Line::from(vec![
            Span::styled("Selected: ", Style::new().fg(Color::Gray)),
            Span::styled("  ", Style::new().bg(self.value)),
            Span::raw(" "),
            Span::styled(
                format!("#{r:02X}{g:02X}{b:02X}"),
                Style::new().fg(Color::Gray),
            ),
        ])]
    }
}

pub fn color_at(display: ColorPickerDisplay, selected: usize) -> Option<Color> {
    match display {
        ColorPickerDisplay::Palette => PALETTE.get(selected).map(|(_, color)| *color),
        ColorPickerDisplay::Spectrum => color_picker_spectrum::color_at(selected),
    }
}

pub fn grid_metrics(display: ColorPickerDisplay) -> ColorPickerGridMetrics {
    match display {
        ColorPickerDisplay::Palette => ColorPickerGridMetrics {
            columns: PALETTE_COLUMNS,
            rows: PALETTE_ROWS,
            cell_width: PALETTE_CELL_WIDTH,
        },
        ColorPickerDisplay::Spectrum => ColorPickerGridMetrics {
            columns: color_picker_spectrum::columns(),
            rows: color_picker_spectrum::rows(),
            cell_width: color_picker_spectrum::cell_width(),
        },
    }
}

pub fn selection_at(display: ColorPickerDisplay, column: usize, row: usize) -> Option<usize> {
    let metrics = grid_metrics(display);
    (column < metrics.columns && row < metrics.rows).then_some(row * metrics.columns + column)
}

pub fn move_selection(
    selected: usize,
    direction: PaletteDirection,
    display: ColorPickerDisplay,
) -> usize {
    match display {
        ColorPickerDisplay::Palette => {
            let selected = selected.min(PALETTE_LENGTH.saturating_sub(1));
            let row = selected / PALETTE_COLUMNS;
            let column = selected % PALETTE_COLUMNS;
            match direction {
                PaletteDirection::Left => row * PALETTE_COLUMNS + column.saturating_sub(1),
                PaletteDirection::Right => {
                    row * PALETTE_COLUMNS + column.saturating_add(1).min(PALETTE_COLUMNS - 1)
                },
                PaletteDirection::Up => row.saturating_sub(1) * PALETTE_COLUMNS + column,
                PaletteDirection::Down => {
                    row.saturating_add(1).min(PALETTE_ROWS - 1) * PALETTE_COLUMNS + column
                },
            }
        },
        ColorPickerDisplay::Spectrum => color_picker_spectrum::move_selection(selected, direction),
    }
}

pub fn nearest_index(color: Color, display: ColorPickerDisplay) -> usize {
    let palette: Vec<Color> = match display {
        ColorPickerDisplay::Palette => PALETTE.iter().map(|(_, color)| *color).collect(),
        ColorPickerDisplay::Spectrum => color_picker_spectrum::all_colors(),
    };
    let (r, g, b) = rgb(color);
    palette
        .iter()
        .enumerate()
        .min_by_key(|(_, candidate)| {
            let (cr, cg, cb) = rgb(**candidate);
            i32::from(r).abs_diff(i32::from(cr)).pow(2)
                + i32::from(g).abs_diff(i32::from(cg)).pow(2)
                + i32::from(b).abs_diff(i32::from(cb)).pow(2)
        })
        .map_or(0, |(index, _)| index)
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
