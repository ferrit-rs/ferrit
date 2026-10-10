//! The colours of the dashboard's charts, derived from the theme
//! (`docs/PLAN_13_DASHBOARD.md`, "Colours"). Widgets never name a colour: they
//! take these. One meaning, one colour: a kind of change has the same colour and
//! marker in the donut, its legend and every bar.
//!
//! The `[theme.colors] chart1..6` overrides and the `[dashboard] charts` key are
//! not read yet; `charts_mode_auto` is the pure part of the latter.

use ratatui::style::{Color, Modifier, Style};

use crate::theme::palette::Palette;
use ferrit_domain::stats::kind::Kind;

/// Categorical slots: feat, fix, docs, test, refactor, then gray "others".
pub const SLOTS: usize = 6;
/// The gray "others" slot.
pub const OTHERS: usize = SLOTS - 1;
/// One shape per categorical slot, so colour is never the only signal.
const MARKERS: [&str; SLOTS] = ["●", "■", "▲", "◆", "▼", "○"];

/// Dark variants of the colours that wash out on white, and the gray.
const LIGHT_YELLOW: Color = Color::Rgb(154, 103, 0);
const LIGHT_CYAN: Color = Color::Rgb(14, 116, 144);
const LIGHT_GRAY: Color = Color::Rgb(107, 114, 128);

/// How the charts are drawn: Braille dots, or block glyphs for a terminal
/// without Braille (the Linux console, a non-UTF-8 locale).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChartMode {
    Braille,
    Blocks,
}

/// `auto`: Braille, unless the locale is not UTF-8 or `TERM=linux`. `locale` is
/// the first set of `LC_ALL`, `LC_CTYPE`, `LANG`; an unset locale is not held
/// against the terminal.
pub fn charts_mode_auto(locale: Option<&str>, term: Option<&str>) -> ChartMode {
    let utf8 = locale
        .filter(|l| !l.is_empty())
        .is_none_or(|l| l.to_ascii_lowercase().replace('-', "").contains("utf8"));
    if utf8 && term != Some("linux") {
        ChartMode::Braille
    } else {
        ChartMode::Blocks
    }
}

/// `charts_mode_auto` on this process's environment.
pub fn charts_mode_from_env() -> ChartMode {
    let locale = ["LC_ALL", "LC_CTYPE", "LANG"]
        .iter()
        .find_map(|name| std::env::var(name).ok().filter(|v| !v.is_empty()));
    charts_mode_auto(locale.as_deref(), std::env::var("TERM").ok().as_deref())
}

/// The brighter variant of an ANSI colour; anything else stays as it is.
fn bright(color: Color) -> Color {
    match color {
        Color::Black => Color::DarkGray,
        Color::Red => Color::LightRed,
        Color::Green => Color::LightGreen,
        Color::Yellow => Color::LightYellow,
        Color::Blue => Color::LightBlue,
        Color::Magenta => Color::LightMagenta,
        Color::Cyan => Color::LightCyan,
        Color::Gray => Color::White,
        other => other,
    }
}

/// Everything the dashboard colours with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChartPalette {
    /// feat, fix, docs, test, refactor, others: the kinds of change only, authors
    /// and files are one series and wear the accent.
    pub categorical: [Color; SLOTS],
    /// Line chart, single-series bars.
    pub accent: Color,
    /// Lines added and removed.
    pub add: Color,
    pub del: Color,
    /// The `↑` ahead arrow and the `stale` alert.
    pub warn: Color,
    /// Quiet day, then four levels from dim to bright.
    pub heat: [Style; 5],
    /// `NO_COLOR`: the heat map's level is a glyph (`· ░ ▒ ▓ █`), not only a colour.
    pub density: bool,
    pub branch_current: Style,
    pub branch_active: Style,
    pub branch_merged: Style,
    pub branch_stale: Style,
    /// Secondary figures (counts in brackets), empty bar cells, captions.
    pub dim: Style,
    /// The page border and the thin rule under each section title: `Palette.idle`, dim.
    pub rule: Style,
}

impl ChartPalette {
    pub fn for_palette(p: &Palette) -> Self {
        let (yellow, cyan, gray) = if p.light {
            (LIGHT_YELLOW, LIGHT_CYAN, LIGHT_GRAY)
        } else {
            (Color::Yellow, Color::Cyan, Color::Gray)
        };
        let dim = Style::new().add_modifier(Modifier::DIM);
        let accent = Style::new().fg(p.focus);
        Self {
            categorical: [
                Color::Green,
                yellow,
                Color::Blue,
                cyan,
                Color::Magenta,
                gray,
            ],
            accent: p.focus,
            add: p.add,
            del: p.del,
            warn: p.warn,
            heat: [
                Style::new().fg(p.idle).add_modifier(Modifier::DIM),
                accent.add_modifier(Modifier::DIM),
                accent,
                accent.add_modifier(Modifier::BOLD),
                Style::new()
                    .fg(bright(p.focus))
                    .add_modifier(Modifier::BOLD),
            ],
            density: false,
            branch_current: accent.add_modifier(Modifier::BOLD),
            branch_active: Style::new(),
            branch_merged: Style::new().fg(gray),
            branch_stale: Style::new().fg(p.warn),
            dim,
            rule: Style::new().fg(p.idle).add_modifier(Modifier::DIM),
        }
    }

    /// Colour of categorical slot `slot`; past the last it is the "others" gray.
    pub fn slot_color(&self, slot: usize) -> Color {
        self.categorical
            .get(slot.min(OTHERS))
            .copied()
            .unwrap_or(Color::Gray)
    }

    pub fn kind_color(&self, kind: Kind) -> Color {
        self.slot_color(kind_slot(kind))
    }
}

/// The categorical slot of a kind: the five named ones, every other kind is
/// "others".
pub const fn kind_slot(kind: Kind) -> usize {
    match kind {
        Kind::Feat => 0,
        Kind::Fix => 1,
        Kind::Docs => 2,
        Kind::Test => 3,
        Kind::Refactor => 4,
        _ => OTHERS,
    }
}

/// The legend marker of categorical slot `slot` (`○` for others and beyond).
pub fn slot_marker(slot: usize) -> &'static str {
    MARKERS.get(slot.min(OTHERS)).copied().unwrap_or("○")
}

pub fn kind_marker(kind: Kind) -> &'static str {
    slot_marker(kind_slot(kind))
}

#[cfg(test)]
#[allow(
    clippy::indexing_slicing,
    reason = "test scaffolding: an out-of-range index is the failed assertion"
)]
mod tests {
    use crate::theme::palette::Palette;
    use crate::widgets::chart_palette::{
        ChartMode, ChartPalette, OTHERS, charts_mode_auto, kind_marker, slot_marker,
    };
    use ferrit_domain::stats::kind::Kind;
    use ratatui::style::{Color, Modifier, Style};

    #[test]
    fn dark_uses_ansi_names() {
        let c = ChartPalette::for_palette(&Palette::DARK);
        assert_eq!(
            c.categorical,
            [
                Color::Green,
                Color::Yellow,
                Color::Blue,
                Color::Cyan,
                Color::Magenta,
                Color::Gray
            ]
        );
        assert_eq!(c.accent, Palette::DARK.focus);
        assert_eq!(c.add, Palette::DARK.add);
        assert_eq!(c.del, Palette::DARK.del);
    }

    #[test]
    fn light_uses_explicit_rgb_for_the_colours_that_wash_out() {
        let c = ChartPalette::for_palette(&Palette::LIGHT);
        assert!(matches!(c.slot_color(1), Color::Rgb(..)));
        assert!(matches!(c.slot_color(3), Color::Rgb(..)));
        assert_eq!(c.slot_color(0), Color::Green);
        assert_eq!(c.slot_color(2), Color::Blue);
    }

    #[test]
    fn a_theme_change_recolours_the_charts() {
        let mut p = Palette::DARK;
        p.focus = Color::Blue;
        p.warn = Color::Red;
        let c = ChartPalette::for_palette(&p);
        assert_eq!(c.accent, Color::Blue);
        assert_eq!(c.heat[2].fg, Some(Color::Blue));
        assert_eq!(c.heat[4].fg, Some(Color::LightBlue));
        assert_eq!(c.branch_current.fg, Some(Color::Blue));
        assert_eq!(c.branch_stale.fg, Some(Color::Red));
    }

    #[test]
    fn the_heat_ramp_goes_from_dim_to_bright() {
        let c = ChartPalette::for_palette(&Palette::DARK);
        assert!(c.heat[0].add_modifier.contains(Modifier::DIM));
        assert!(c.heat[1].add_modifier.contains(Modifier::DIM));
        assert!(!c.heat[2].add_modifier.contains(Modifier::DIM));
        assert!(c.heat[3].add_modifier.contains(Modifier::BOLD));
        assert_eq!(c.heat[4].fg, Some(Color::LightGreen));
    }

    #[test]
    fn branch_states() {
        let c = ChartPalette::for_palette(&Palette::DARK);
        assert!(c.branch_current.add_modifier.contains(Modifier::BOLD));
        assert_eq!(c.branch_active, Style::new());
        assert_eq!(c.branch_merged.fg, Some(Color::Gray));
        assert_eq!(c.branch_stale.fg, Some(Palette::DARK.warn));
    }

    #[test]
    fn the_five_named_kinds_have_their_own_colour_and_marker_the_rest_are_others() {
        let c = ChartPalette::for_palette(&Palette::DARK);
        let named = [
            Kind::Feat,
            Kind::Fix,
            Kind::Docs,
            Kind::Test,
            Kind::Refactor,
        ];
        for (i, &a) in named.iter().enumerate() {
            for &b in &named[i + 1..] {
                assert_ne!(c.kind_color(a), c.kind_color(b));
                assert_ne!(kind_marker(a), kind_marker(b));
            }
            assert_ne!(c.kind_color(a), c.slot_color(OTHERS));
            assert_ne!(kind_marker(a), slot_marker(OTHERS));
        }
        for kind in [
            Kind::Perf,
            Kind::Style,
            Kind::Build,
            Kind::Ci,
            Kind::Chore,
            Kind::Other,
        ] {
            assert_eq!(c.kind_color(kind), c.slot_color(OTHERS));
            assert_eq!(kind_marker(kind), "○");
        }
        assert_eq!(kind_marker(Kind::Feat), "●");
    }

    #[test]
    fn the_rule_is_the_idle_colour_dimmed() {
        let c = ChartPalette::for_palette(&Palette::DARK);
        assert_eq!(c.rule.fg, Some(Palette::DARK.idle));
        assert!(c.rule.add_modifier.contains(Modifier::DIM));
        assert_eq!(slot_marker(40), "○");
    }

    #[test]
    fn braille_unless_the_locale_is_not_utf8_or_the_console_is_linux() {
        use ChartMode::{Blocks, Braille};
        assert_eq!(
            charts_mode_auto(Some("en_US.UTF-8"), Some("xterm")),
            Braille
        );
        assert_eq!(charts_mode_auto(Some("C.utf8"), None), Braille);
        assert_eq!(charts_mode_auto(None, Some("xterm-256color")), Braille);
        assert_eq!(charts_mode_auto(Some(""), None), Braille);
        assert_eq!(charts_mode_auto(Some("C"), Some("xterm")), Blocks);
        assert_eq!(charts_mode_auto(Some("POSIX"), None), Blocks);
        assert_eq!(charts_mode_auto(Some("en_US.ISO-8859-1"), None), Blocks);
        assert_eq!(charts_mode_auto(Some("en_US.UTF-8"), Some("linux")), Blocks);
    }
}
