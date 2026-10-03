//! A painted colour scheme (`docs/PLAN_18_THEMES.md`): the background, the text
//! colour and the sixteen ANSI names as RGB, applied in one pass over a drawn
//! frame. Widgets keep drawing with `Reset` and the ANSI names; the pass turns
//! those into the scheme's own colours, so a dark or a light screen does not
//! depend on the terminal's background, and a colour a future widget picks is
//! covered without touching it.

use ratatui::buffer::Buffer;
use ratatui::style::Color;

/// The ANSI names in index order (0 to 15), for the two tables.
const NAMES: [Color; 16] = [
    Color::Black,
    Color::Red,
    Color::Green,
    Color::Yellow,
    Color::Blue,
    Color::Magenta,
    Color::Cyan,
    Color::Gray,
    Color::DarkGray,
    Color::LightRed,
    Color::LightGreen,
    Color::LightYellow,
    Color::LightBlue,
    Color::LightMagenta,
    Color::LightCyan,
    Color::White,
];

const fn rgb(hex: u32) -> Color {
    let [_, r, g, b] = hex.to_be_bytes();
    Color::Rgb(r, g, b)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Scheme {
    /// What an unset background becomes.
    pub background: Color,
    /// What an unset foreground becomes.
    pub text: Color,
    /// The ANSI names when used as a foreground (text on `background`).
    pub foreground: [Color; 16],
    /// The ANSI names when used as a background (behind white or dark text).
    /// `Black` is the dimming layer behind a popup or the drawer, so it is a
    /// slightly darker (or slightly greyer) `background`, not black.
    pub fill: [Color; 16],
}

impl Scheme {
    pub const DARK: Self = Self {
        background: rgb(0x10_1216),
        text: rgb(0xd7_dae0),
        foreground: [
            rgb(0x0b_0d10),
            rgb(0xf8_5149),
            rgb(0x3f_b950),
            rgb(0xd2_9922),
            rgb(0x58_a6ff),
            rgb(0xbc_8cff),
            rgb(0x39_c5cf),
            rgb(0x8b_949e),
            rgb(0x6e_7681),
            rgb(0xff_7b72),
            rgb(0x56_d364),
            rgb(0xe3_b341),
            rgb(0x79_c0ff),
            rgb(0xd2_a8ff),
            rgb(0x56_d4dd),
            rgb(0xf0_f6fc),
        ],
        fill: [
            rgb(0x0a_0b0d),
            rgb(0x8e_1519),
            rgb(0x1a_5f2a),
            rgb(0x7d_5a10),
            rgb(0x1f_4e8c),
            rgb(0x5b_3a99),
            rgb(0x14_636b),
            rgb(0x48_4f58),
            rgb(0x30_363d),
            rgb(0xb6_2324),
            rgb(0x23_863a),
            rgb(0x9e_6a03),
            rgb(0x31_6dca),
            rgb(0x6e_40c9),
            rgb(0x1b_7c83),
            rgb(0xf0_f6fc),
        ],
    };

    pub const LIGHT: Self = Self {
        background: rgb(0xff_ffff),
        text: rgb(0x1f_2328),
        foreground: [
            rgb(0x1f_2328),
            rgb(0xcf_222e),
            rgb(0x1a_7f37),
            rgb(0x9a_6700),
            rgb(0x09_69da),
            rgb(0x82_50df),
            rgb(0x1b_7c83),
            rgb(0x65_6d76),
            rgb(0x8c_959f),
            rgb(0xa4_0e26),
            rgb(0x11_6329),
            rgb(0x7d_4e00),
            rgb(0x05_50ae),
            rgb(0x66_39ba),
            rgb(0x13_6061),
            rgb(0xff_ffff),
        ],
        fill: [
            rgb(0xe6_e8eb),
            rgb(0xcf_222e),
            rgb(0x1a_7f37),
            rgb(0x9a_6700),
            rgb(0x09_69da),
            rgb(0x82_50df),
            rgb(0x1b_7c83),
            rgb(0xd0_d7de),
            rgb(0xaf_b8c1),
            rgb(0xff_ebe9),
            rgb(0xda_fbe1),
            rgb(0xff_f8c5),
            rgb(0xdd_f4ff),
            rgb(0xfb_efff),
            rgb(0xb6_e3ff),
            rgb(0xff_ffff),
        ],
    };

    /// The foreground a cell is drawn with once painted.
    #[must_use]
    pub fn paint_foreground(&self, color: Color) -> Color {
        match color {
            Color::Reset => self.text,
            named => Self::pick(&self.foreground, named),
        }
    }

    /// The background a cell is drawn with once painted.
    #[must_use]
    pub fn paint_background(&self, color: Color) -> Color {
        match color {
            Color::Reset => self.background,
            named => Self::pick(&self.fill, named),
        }
    }

    /// `color` from `table` when it is an ANSI name; an `Rgb` or `Indexed`
    /// colour is a deliberate choice (a diff tint, a syntax colour, the accent)
    /// and stays as it is.
    fn pick(table: &[Color; 16], color: Color) -> Color {
        NAMES
            .iter()
            .position(|name| *name == color)
            .and_then(|index| table.get(index).copied())
            .unwrap_or(color)
    }

    /// Paint every cell of a drawn frame.
    pub fn paint(&self, buffer: &mut Buffer) {
        for cell in &mut buffer.content {
            cell.fg = self.paint_foreground(cell.fg);
            cell.bg = self.paint_background(cell.bg);
        }
    }
}

/// WCAG relative luminance of an `Rgb` colour (`None` for anything else).
#[must_use]
pub fn luminance(color: Color) -> Option<f64> {
    let Color::Rgb(r, g, b) = color else {
        return None;
    };
    let channel = |v: u8| {
        let v = f64::from(v) / 255.0;
        if v <= 0.039_28 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    Some(0.0722f64.mul_add(
        channel(b),
        0.2126f64.mul_add(channel(r), 0.7152 * channel(g)),
    ))
}

/// WCAG contrast ratio between two `Rgb` colours, 1 to 21.
#[must_use]
pub fn contrast(a: Color, b: Color) -> Option<f64> {
    let (la, lb) = (luminance(a)?, luminance(b)?);
    let (hi, lo) = if la > lb { (la, lb) } else { (lb, la) };
    Some((hi + 0.05) / (lo + 0.05))
}
