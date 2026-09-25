#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    elided_lifetimes_in_paths,
    reason = "integration test scaffolding: a failed setup is the assertion, helper ergonomics beat lint-cleanliness here"
)]
//! Every colour ferrit draws comes from the app's `Palette`
//! (`docs/PLAN_12_POLISH.md` P5): a builder handed another palette paints with
//! it, and the default palette is the one ferrit always had.

use ferrit::app::config::Config;
use ferrit::app::mock::{mock_commits, mock_files};
use ferrit::app::theme;
use ferrit::app::{App, Pane};
use ferrit::components::ui::key_bar::KeyBar;
use ferrit::components::ui::palette::Palette;
use ferrit::domain::git::diff::parse_diff;
use ratatui::style::Color;

/// A palette where every colour is different from `Palette::DARK`'s and from
/// the others, so a builder that reads the wrong field shows up.
fn loud() -> Palette {
    Palette {
        focus: Color::Indexed(1),
        idle: Color::Indexed(2),
        selection: Color::Indexed(3),
        selection_fg: Color::Indexed(4),
        add: Color::Indexed(5),
        del: Color::Indexed(6),
        hunk: Color::Indexed(7),
        hash: Color::Indexed(8),
        author: Color::Indexed(9),
        warn: Color::Indexed(10),
        key: Color::Indexed(11),
        focus_box: Color::Indexed(12),
        add_line_bg: Color::Indexed(13),
        del_line_bg: Color::Indexed(14),
        light: false,
    }
}

#[test]
fn the_default_palette_is_the_dark_one() {
    assert_eq!(Palette::default(), Palette::DARK);
    assert_eq!(App::mock().palette(), Palette::DARK);
}

#[test]
fn a_commit_row_takes_its_hash_and_author_colours_from_the_palette() {
    let p = loud();
    let line = theme::commit_line(&p, &mock_commits()[0]);
    let colours: Vec<_> = line.spans.iter().filter_map(|s| s.style.fg).collect();
    assert_eq!(colours, [p.hash, p.author, p.hash]);
}

#[test]
fn a_file_row_takes_its_status_colour_from_the_palette() {
    let p = loud();
    let files = mock_files();
    let colour_of = |code: &str| {
        let entry = files
            .iter()
            .find(|f| f.display().starts_with(code))
            .unwrap_or_else(|| panic!("a mock file with status {code}"));
        theme::file_line(&p, entry, 0).spans[1].style.fg
    };
    assert_eq!(colour_of(" M"), Some(p.warn));
}

#[test]
fn the_selection_bar_and_the_counter_use_the_palette() {
    let p = loud();
    let style = theme::selection_style(&p, true);
    assert_eq!(
        (style.bg, style.fg),
        (Some(p.selection), Some(p.selection_fg))
    );
    assert_eq!(theme::counter_line(&p, 1, 4).style.fg, Some(p.idle));
}

#[test]
fn the_keybar_takes_its_key_colour_from_the_palette() {
    let p = loud();
    let line = KeyBar::hints("Stage: <space>", &p).line();
    assert_eq!(line.spans[0].style.fg, Some(p.idle));
    assert_eq!(line.spans[1].style.fg, Some(p.key));
}

#[test]
fn a_frame_on_the_default_palette_paints_borders_in_its_idle_colour() {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;
    let mut app = App::mock();
    app.focus = Pane::Files;
    let mut terminal = Terminal::new(TestBackend::new(100, 30)).unwrap();
    terminal
        .draw(|f| ferrit::app::screens::draw(f, &mut app))
        .unwrap();
    let buffer = terminal.backend().buffer();
    assert!(
        buffer
            .content
            .iter()
            .any(|cell| cell.fg == Palette::DARK.idle),
        "unfocused borders"
    );
}

#[test]
fn light_differs_from_dark_only_where_dark_assumes_a_dark_terminal() {
    let (dark, light) = (Palette::DARK, Palette::LIGHT);
    assert_ne!(light.selection_fg, dark.selection_fg);
    assert_ne!(light.focus_box, dark.focus_box);
    assert_ne!(light.add_line_bg, dark.add_line_bg);
    assert_ne!(light.del_line_bg, dark.del_line_bg);
    assert!(light.light && !dark.light);
    // Everything else is the same ANSI names, which follow the terminal.
    let same = |a: Palette, b: Palette| {
        (
            a.focus,
            a.idle,
            a.selection,
            a.add,
            a.del,
            a.hunk,
            a.hash,
            a.author,
            a.warn,
            a.key,
        ) == (
            b.focus,
            b.idle,
            b.selection,
            b.add,
            b.del,
            b.hunk,
            b.hash,
            b.author,
            b.warn,
            b.key,
        )
    };
    assert!(same(dark, light));
}

#[test]
fn the_base_picks_the_palette() {
    let (config, issues) = Config::parse("[theme]\nbase = \"light\"\n");
    assert!(issues.is_empty(), "{issues:?}");
    assert_eq!(config.theme.palette(), Palette::LIGHT);
    assert_eq!(Config::default().theme.palette(), Palette::DARK);
}

#[test]
fn an_unknown_base_is_reported_and_the_theme_goes_back_to_its_defaults() {
    let (config, issues) = Config::parse("[theme]\nbase = \"sepia\"\npreset = \"blue\"\n");
    assert_eq!(config.theme.palette(), Palette::DARK);
    assert!(
        issues.iter().any(|i| i.starts_with("[theme] ignored")),
        "{issues:?}"
    );
}

#[test]
fn a_diff_uses_the_light_tints_and_a_light_syntax_theme_on_a_light_base() {
    let text = "diff --git a/a.rs b/a.rs\n--- a/a.rs\n+++ b/a.rs\n@@ -1,2 +1,2 @@\n fn main() {}\n-let old = 1;\n+let new = 2;\n";
    let diff = parse_diff(text);
    let backgrounds = |p: &Palette| -> Vec<Option<Color>> {
        let rendered = theme::render_diff(p, &diff, None, 40);
        rendered
            .lines
            .iter()
            .map(|l| l.spans.last().and_then(|s| s.style.bg))
            .collect()
    };
    let light = backgrounds(&Palette::LIGHT);
    assert!(light.contains(&Some(Palette::LIGHT.add_line_bg)));
    assert!(light.contains(&Some(Palette::LIGHT.del_line_bg)));
    assert!(!light.contains(&Some(Palette::DARK.add_line_bg)));

    let code_colours = |p: &Palette| -> Vec<Option<Color>> {
        let rendered = theme::render_diff(p, &diff, None, 40);
        // The context line `fn main() {}` is tokenized by syntect.
        rendered.lines[4].spans.iter().map(|s| s.style.fg).collect()
    };
    assert_ne!(code_colours(&Palette::LIGHT), code_colours(&Palette::DARK));
}
