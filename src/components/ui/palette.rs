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
    };
}

impl Default for Palette {
    fn default() -> Self {
        Self::DARK
    }
}
