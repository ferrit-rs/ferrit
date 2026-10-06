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

    /// `color` from `table` when it is an ANSI name; an `Rgb` colour is a
    /// deliberate choice (a diff tint, a syntax colour, the accent) and stays
    /// as it is. Indexed colours are normalized in `paint` for truecolor.
    fn pick(table: &[Color; 16], color: Color) -> Color {
        NAMES
            .iter()
            .position(|name| *name == color)
            .and_then(|index| table.get(index).copied())
            .unwrap_or(color)
    }

    /// `fg` when it reads on `bg` (contrast 3 or more), else the better of white
    /// and the scheme's text colour.
    fn readable_on(&self, fg: Color, bg: Color) -> Color {
        if contrast(fg, bg).is_some_and(|ratio| ratio >= 3.0) {
            return fg;
        }
        let white = Color::Rgb(255, 255, 255);
        let best = |c: Color| contrast(c, bg).unwrap_or(0.0);
        if best(white) >= best(self.text) {
            white
        } else {
            self.text
        }
    }

    /// Paint every cell of a drawn frame. Without 24-bit colour in the terminal
    /// (`ColorDepth::Indexed`) every `Rgb` the frame ends up with, the scheme's own and
    /// the widgets' (diff tints, syntax colours, the accent), becomes the nearest
    /// of the 256 colours, so the theme still reads as dark or light.
    pub fn paint(&self, buffer: &mut Buffer, depth: ColorDepth) {
        // Neighbouring cells mostly share their colours: remember the last answer
        // for the foreground and for the background instead of recomputing it.
        let mut last_fg = Memo::default();
        let mut last_bg = Memo::default();
        for cell in &mut buffer.content {
            // A cell on a named fill (a selection bar) must stay readable whatever
            // colour its text was given: yellow on the light theme's blue is not.
            let on_fill = !matches!(
                cell.bg,
                Color::Reset | Color::Black | Color::Rgb(..) | Color::Indexed(_)
            );
            cell.fg = self.paint_foreground(cell.fg);
            cell.bg = self.paint_background(cell.bg);
            if depth == ColorDepth::TrueColor {
                cell.fg = expand_indexed(cell.fg);
                cell.bg = expand_indexed(cell.bg);
            }
            if on_fill {
                cell.fg = self.readable_on(cell.fg, cell.bg);
            }
            if depth == ColorDepth::Indexed {
                cell.fg = last_fg.approximate(cell.fg);
                cell.bg = last_bg.approximate(cell.bg);
            }
        }
    }
}

fn expand_indexed(color: Color) -> Color {
    let Color::Indexed(index) = color else {
        return color;
    };
    let rgb = match index {
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
        16..=231 => {
            let cube = index - 16;
            let channel = |value| if value == 0 { 0 } else { 55 + 40 * value };
            (
                channel(cube / 36),
                channel((cube % 36) / 6),
                channel(cube % 6),
            )
        },
        232..=255 => {
            let gray = 8 + 10 * (index - 232);
            (gray, gray, gray)
        },
    };
    Color::Rgb(rgb.0, rgb.1, rgb.2)
}

/// How many colours the terminal can show.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ColorDepth {
    /// 24-bit colour: `Rgb` is drawn as it is.
    #[default]
    TrueColor,
    /// 256 colours: every `Rgb` is replaced by its nearest.
    Indexed,
}

impl ColorDepth {
    /// What a terminal that sets `COLORTERM` to `colorterm` can show.
    /// `Terminal.app` sets nothing and has 256 colours; tmux needs `Tc` besides.
    #[must_use]
    pub fn detect(colorterm: Option<&str>) -> Self {
        if colorterm
            .is_some_and(|v| v.eq_ignore_ascii_case("truecolor") || v.eq_ignore_ascii_case("24bit"))
        {
            Self::TrueColor
        } else {
            Self::Indexed
        }
    }
}

/// The levels of the xterm 6x6x6 colour cube.
const CUBE: [u8; 6] = [0, 95, 135, 175, 215, 255];

fn distance(a: (u8, u8, u8), b: (u8, u8, u8)) -> u32 {
    let d = |x: u8, y: u8| u32::from(x.abs_diff(y)).pow(2);
    d(a.0, b.0) + d(a.1, b.1) + d(a.2, b.2)
}

/// The nearest xterm-256 colour of an RGB one, among the cube (16 to 231) and the
/// grey ramp (232 to 255). The first sixteen are left out: a terminal maps them to
/// its own palette, so they are not a known colour.
#[must_use]
pub fn nearest_256(r: u8, g: u8, b: u8) -> u8 {
    let level = |v: u8| {
        (0..6u8)
            .min_by_key(|&i| {
                CUBE.get(usize::from(i))
                    .map_or(u32::MAX, |&c| u32::from(c.abs_diff(v)))
            })
            .unwrap_or(0)
    };
    let (ri, gi, bi) = (level(r), level(g), level(b));
    let at = |i: u8| CUBE.get(usize::from(i)).copied().unwrap_or(0);
    let cube = (at(ri), at(gi), at(bi));
    let cube_index = 16 + 36 * ri + 6 * gi + bi;
    let mean = u8::try_from((u32::from(r) + u32::from(g) + u32::from(b)) / 3).unwrap_or(u8::MAX);
    let step = u8::try_from((u32::from(mean).saturating_sub(8) + 5) / 10)
        .unwrap_or(23)
        .min(23);
    let grey = 8 + 10 * step;
    if distance((r, g, b), (grey, grey, grey)) < distance((r, g, b), cube) {
        232 + step
    } else {
        cube_index
    }
}

/// The last `Rgb` turned into its nearest of the 256, and what it became.
#[derive(Default)]
struct Memo(Option<((u8, u8, u8), u8)>);

impl Memo {
    /// `color` with an `Rgb` replaced by its nearest of the 256; anything else as is.
    fn approximate(&mut self, color: Color) -> Color {
        let Color::Rgb(r, g, b) = color else {
            return color;
        };
        match self.0 {
            Some((rgb, index)) if rgb == (r, g, b) => Color::Indexed(index),
            _ => {
                let index = nearest_256(r, g, b);
                self.0 = Some(((r, g, b), index));
                Color::Indexed(index)
            },
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
