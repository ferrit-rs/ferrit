//! Two-dimensional hue and shade grid used by the color picker.

use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use super::{PaletteDirection, rgb};

const SPECTRUM_COLUMNS: usize = 12;
const SPECTRUM_ROWS: usize = 5;
const SPECTRUM_LENGTH: usize = SPECTRUM_COLUMNS * SPECTRUM_ROWS;
const SPECTRUM_INDEX_STEP: usize = 1;
const HUE_CIRCLE_DEGREES: f32 = 360.0;
const HUE_SECTOR_DEGREES: f32 = 60.0;
const CHANNEL_MAX: f32 = 255.0;
const HIGHLIGHT_THRESHOLD: u32 = 128;
const SPECTRUM_CELL_WIDTH: usize = 2;
const RED_LUMINANCE_WEIGHT: u32 = 299;
const GREEN_LUMINANCE_WEIGHT: u32 = 587;
const BLUE_LUMINANCE_WEIGHT: u32 = 114;
const LUMINANCE_WEIGHT_TOTAL: u32 = 1000;
const SHADE_LEVELS: [(f32, f32); SPECTRUM_ROWS] = [
    (0.18, 0.35),
    (0.38, 0.58),
    (0.64, 0.82),
    (0.82, 0.94),
    (1.0, 1.0),
];

pub struct ColorPickerSpectrum {
    selected: usize,
    active: bool,
}

impl ColorPickerSpectrum {
    pub fn new(selected: usize, active: bool) -> Self {
        Self { selected, active }
    }

    pub fn lines(self) -> Vec<Line<'static>> {
        (0..SPECTRUM_ROWS)
            .map(|row| {
                let spans: Vec<Span<'static>> = (0..SPECTRUM_COLUMNS)
                    .map(|column| {
                        let index = row * SPECTRUM_COLUMNS + column;
                        let color = spectrum_color(column, row);
                        let selected = index == self.selected;
                        let marker = if selected { "[]" } else { "  " };
                        let foreground = if brightness(color) >= HIGHLIGHT_THRESHOLD {
                            Color::Black
                        } else {
                            Color::White
                        };
                        Span::styled(
                            marker,
                            Style::new().fg(foreground).bg(color).add_modifier(
                                if selected && self.active {
                                    Modifier::BOLD | Modifier::UNDERLINED
                                } else if selected {
                                    Modifier::BOLD
                                } else {
                                    Modifier::empty()
                                },
                            ),
                        )
                    })
                    .collect();
                Line::from(spans)
            })
            .collect()
    }
}

pub(super) fn color_at(index: usize) -> Option<Color> {
    if index >= SPECTRUM_LENGTH {
        return None;
    }
    let row = index / SPECTRUM_COLUMNS;
    let column = index % SPECTRUM_COLUMNS;
    Some(spectrum_color(column, row))
}

pub(super) fn all_colors() -> Vec<Color> {
    (0..SPECTRUM_LENGTH).filter_map(color_at).collect()
}

pub(super) const fn columns() -> usize {
    SPECTRUM_COLUMNS
}

pub(super) const fn rows() -> usize {
    SPECTRUM_ROWS
}

pub(super) const fn cell_width() -> usize {
    SPECTRUM_CELL_WIDTH
}

pub(super) fn move_selection(selected: usize, direction: PaletteDirection) -> usize {
    let selected = selected.min(SPECTRUM_LENGTH - SPECTRUM_INDEX_STEP);
    let row = selected / SPECTRUM_COLUMNS;
    let column = selected % SPECTRUM_COLUMNS;
    match direction {
        PaletteDirection::Left => {
            row * SPECTRUM_COLUMNS + column.saturating_sub(SPECTRUM_INDEX_STEP)
        },
        PaletteDirection::Right => {
            row * SPECTRUM_COLUMNS
                + column
                    .saturating_add(SPECTRUM_INDEX_STEP)
                    .min(SPECTRUM_COLUMNS - SPECTRUM_INDEX_STEP)
        },
        PaletteDirection::Up => row.saturating_sub(SPECTRUM_INDEX_STEP) * SPECTRUM_COLUMNS + column,
        PaletteDirection::Down => {
            row.saturating_add(SPECTRUM_INDEX_STEP)
                .min(SPECTRUM_ROWS - SPECTRUM_INDEX_STEP)
                * SPECTRUM_COLUMNS
                + column
        },
    }
}

fn spectrum_color(column: usize, row: usize) -> Color {
    let hue = (column as f32 * HUE_CIRCLE_DEGREES / SPECTRUM_COLUMNS as f32)
        .rem_euclid(HUE_CIRCLE_DEGREES);
    let (saturation, value) = SHADE_LEVELS[row];
    let chroma = value * saturation;
    let hue_sector = hue / HUE_SECTOR_DEGREES;
    let secondary = chroma * (1.0 - (hue_sector.rem_euclid(2.0) - 1.0).abs());
    let (red, green, blue) = match hue_sector as u8 {
        0 => (chroma, secondary, 0.0),
        1 => (secondary, chroma, 0.0),
        2 => (0.0, chroma, secondary),
        3 => (0.0, secondary, chroma),
        4 => (secondary, 0.0, chroma),
        _ => (chroma, 0.0, secondary),
    };
    let offset = value - chroma;
    Color::Rgb(
        channel(red + offset),
        channel(green + offset),
        channel(blue + offset),
    )
}

fn channel(value: f32) -> u8 {
    (value * CHANNEL_MAX).round().clamp(0.0, CHANNEL_MAX) as u8
}

fn brightness(color: Color) -> u32 {
    let (red, green, blue) = rgb(color);
    (u32::from(red) * RED_LUMINANCE_WEIGHT
        + u32::from(green) * GREEN_LUMINANCE_WEIGHT
        + u32::from(blue) * BLUE_LUMINANCE_WEIGHT)
        / LUMINANCE_WEIGHT_TOTAL
}
