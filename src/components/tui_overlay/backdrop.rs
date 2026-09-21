use ratatui_core::buffer::Buffer;
use ratatui_core::layout::Rect;
use ratatui_core::style::Color;

/// Dimming layer behind an open overlay.
///
/// Sets `bg` to `base` and `fg` to a derived (or explicit) dim color
/// for all cells outside the overlay rect. Symbols are preserved.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Backdrop {
    base: Color,
    fg: Option<Color>,
}

impl Backdrop {
    pub fn new(base: Color) -> Self {
        Self { base, fg: None }
    }

    pub fn fg(mut self, color: Color) -> Self {
        self.fg = Some(color);
        self
    }

    /// Dim all cells in `area` except those inside `exclude`.
    pub(crate) fn apply(&self, buf: &mut Buffer, area: Rect, exclude: Rect) {
        let fg = self.resolved_fg();

        for y in area.top()..area.bottom() {
            for x in area.left()..area.right() {
                if x >= exclude.left()
                    && x < exclude.right()
                    && y >= exclude.top()
                    && y < exclude.bottom()
                {
                    continue;
                }

                let cell = &mut buf[(x, y)];
                cell.fg = fg;
                cell.bg = self.base;
            }
        }
    }

    fn resolved_fg(&self) -> Color {
        self.fg.unwrap_or_else(|| derive_fg(self.base))
    }
}

/// Midpoint blend: `|c| c / 2 + 64` per channel.
fn derive_fg(base: Color) -> Color {
    let (r, g, b) = color_to_rgb(base);
    Color::Rgb(r / 2 + 64, g / 2 + 64, b / 2 + 64)
}

fn color_to_rgb(color: Color) -> (u8, u8, u8) {
    match color {
        Color::Rgb(r, g, b) => (r, g, b),
        Color::Indexed(i) => indexed_to_rgb(i),
        Color::Reset => (0, 0, 0),
        named => indexed_to_rgb(named_to_index(named)),
    }
}

fn named_to_index(color: Color) -> u8 {
    match color {
        Color::Black => 0,
        Color::Red => 1,
        Color::Green => 2,
        Color::Yellow => 3,
        Color::Blue => 4,
        Color::Magenta => 5,
        Color::Cyan => 6,
        Color::Gray => 7,
        Color::DarkGray => 8,
        Color::LightRed => 9,
        Color::LightGreen => 10,
        Color::LightYellow => 11,
        Color::LightBlue => 12,
        Color::LightMagenta => 13,
        Color::LightCyan => 14,
        Color::White => 15,
        _ => 0,
    }
}

fn indexed_to_rgb(index: u8) -> (u8, u8, u8) {
    match index {
        // Standard ANSI 16 colors
        0 => (0, 0, 0),
        1 => (128, 0, 0),
        2 => (0, 128, 0),
        3 => (128, 128, 0),
        4 => (0, 0, 128),
        5 => (128, 0, 128),
        6 => (0, 128, 128),
        7 => (192, 192, 192),
        8 => (128, 128, 128),
        9 => (255, 0, 0),
        10 => (0, 255, 0),
        11 => (255, 255, 0),
        12 => (0, 0, 255),
        13 => (255, 0, 255),
        14 => (0, 255, 255),
        15 => (255, 255, 255),

        // 6×6×6 color cube
        16..=231 => {
            let i = index - 16;
            let to_channel = |v: u8| if v == 0 { 0 } else { 55 + 40 * v };
            (
                to_channel(i / 36),
                to_channel((i % 36) / 6),
                to_channel(i % 6),
            )
        },

        // 24-step grayscale ramp
        232..=255 => {
            let gray = 8 + 10 * (index - 232);
            (gray, gray, gray)
        },
    }
}
