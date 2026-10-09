//! The colours ferrit draws with, as one value the app holds and hands to
//! every widget and line builder that colours something. `dark` is the palette
//! ferrit always had; see `docs/PLAN_12_POLISH.md` P5.

use ratatui::style::Color;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Palette {
    /// Border and title of the focused left pane (lazygit `activeBorderColor`).
    pub focus: Color,
    /// Border of every unfocused pane and other low-priority chrome
    /// (lazygit `inactiveBorderColor`, roughly the default foreground).
    pub idle: Color,
    /// Background of the selected row (lazygit `selectedLineBgColor`).
    pub selection: Color,
    /// Text on the selected row.
    pub selection_fg: Color,
    /// Added diff line, checked-out branch.
    pub add: Color,
    /// Removed diff line, deleted path.
    pub del: Color,
    /// Hunk header (`@@ ... @@`).
    pub hunk: Color,
    /// Commit hash and the graph node.
    pub hash: Color,
    /// Author initials in the commit list.
    pub author: Color,
    /// Modified path, ahead/behind counts, the `N of M` counter.
    pub warn: Color,
    /// Key names in the keybind bar.
    pub key: Color,
    /// Background tint boxing the hunk (or file, in a commit) a `]` / `[` jump
    /// last landed on.
    pub focus_box: Color,
    /// Full-line pastel background tint on a `+` line, under its
    /// syntax-coloured text.
    pub add_line_bg: Color,
    /// Full-line pastel background tint on a `-` line.
    pub del_line_bg: Color,
    /// The terminal background is light: syntax colours in a diff come from a
    /// light theme. Not a colour, so `[theme.colors]` cannot set it; `base`
    /// does.
    pub light: bool,
}

impl Palette {
    /// Tuned to match lazygit's default theme: green for the focused pane, a
    /// solid blue selection bar, green hashes, yellow keys.
    pub const DARK: Self = Self {
        focus: Color::Green,
        idle: Color::Gray,
        selection: Color::Blue,
        selection_fg: Color::White,
        add: Color::Green,
        del: Color::Red,
        hunk: Color::Cyan,
        hash: Color::Green,
        author: Color::Magenta,
        warn: Color::Yellow,
        key: Color::Yellow,
        focus_box: Color::DarkGray,
        add_line_bg: Color::Rgb(20, 45, 20),
        del_line_bg: Color::Rgb(55, 20, 20),
        light: false,
    };

    /// `DARK` where it works on a light terminal too (the other colours are
    /// ANSI names the terminal's own theme maps), and different where it
    /// assumes a dark one: the diff line tints and the box around a hunk are
    /// pastel instead of near-black, and the selection text is pure white
    /// because the ANSI `White` of a light theme can be dark.
    pub const LIGHT: Self = Self {
        selection_fg: Color::Rgb(255, 255, 255),
        focus_box: Color::Rgb(224, 224, 224),
        add_line_bg: Color::Rgb(214, 245, 214),
        del_line_bg: Color::Rgb(250, 214, 214),
        light: true,
        ..Self::DARK
    };
}

impl Palette {
    /// The colour called `name` in `[theme.colors]`, to set it. `None` for a
    /// name that is not one of the 14.
    pub fn color_mut(&mut self, name: &str) -> Option<&mut Color> {
        Some(match name {
            "focus" => &mut self.focus,
            "idle" => &mut self.idle,
            "selection" => &mut self.selection,
            "selection_fg" => &mut self.selection_fg,
            "add" => &mut self.add,
            "del" => &mut self.del,
            "hunk" => &mut self.hunk,
            "hash" => &mut self.hash,
            "author" => &mut self.author,
            "warn" => &mut self.warn,
            "key" => &mut self.key,
            "focus_box" => &mut self.focus_box,
            "add_line_bg" => &mut self.add_line_bg,
            "del_line_bg" => &mut self.del_line_bg,
            _ => return None,
        })
    }

    /// The names `color_mut` knows, for a message.
    pub const NAMES: &'static str = "focus, idle, selection, selection_fg, add, del, hunk, hash, \
         author, warn, key, focus_box, add_line_bg, del_line_bg";
}

impl Default for Palette {
    fn default() -> Self {
        Self::DARK
    }
}
