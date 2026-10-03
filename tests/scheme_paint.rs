#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "integration test scaffolding: a failed setup is the assertion"
)]
//! Painted themes (`docs/PLAN_18_THEMES.md`, P0): `[theme] scheme = "dark" |
//! "light"` paints the whole frame, `terminal` (the default) paints nothing.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use ferrit::app::config::{Config, ConfigLoad};
use ferrit::app::theme_config::SchemeChoice;
use ferrit::app::{App, screens as ui};
use ferrit::components::ui::scheme::{Scheme, contrast};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::Color;
use unicode_width::UnicodeWidthStr;

struct Repo(PathBuf);

impl Repo {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = fs::canonicalize(std::env::temp_dir())
            .unwrap()
            .join(format!("ferrit-{tag}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        let status = Command::new("git")
            .arg("-C")
            .arg(&path)
            .args(["init", "-q", "."])
            .status()
            .unwrap();
        assert!(status.success());
        fs::write(path.join("a.txt"), "one\n").unwrap();
        Self(path)
    }

    fn app(&self, toml: &str) -> App {
        let (config, issues) = Config::parse(toml);
        assert!(issues.is_empty(), "{issues:?}");
        App::open_with(
            &self.0,
            ConfigLoad {
                config,
                file: None,
                issues,
            },
        )
        .unwrap()
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn frame(app: &mut App) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    terminal.draw(|f| ui::draw(f, app)).unwrap();
    terminal.backend().buffer().clone()
}

fn is_named(color: Color) -> bool {
    !matches!(color, Color::Reset | Color::Rgb(..) | Color::Indexed(_))
}

/// The first cell that still has an unset or an ANSI-named colour, described.
fn unpainted(buf: &Buffer) -> Option<String> {
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            // The second cell of a wide glyph (an emoji) is not written by the
            // backend; a terminal gives it the first cell's colours.
            if x > 0 && buf[(x - 1, y)].symbol().width() > 1 {
                continue;
            }
            let cell = &buf[(x, y)];
            if cell.fg == Color::Reset
                || cell.bg == Color::Reset
                || is_named(cell.fg)
                || is_named(cell.bg)
            {
                return Some(format!(
                    "({x},{y}) {:?} fg {:?} bg {:?}",
                    cell.symbol(),
                    cell.fg,
                    cell.bg
                ));
            }
        }
    }
    None
}

fn press(app: &mut App, code: KeyCode) {
    app.feed_key(KeyEvent::from(code));
}

#[test]
fn a_painted_scheme_leaves_no_unset_and_no_ansi_colour_on_the_panes() {
    let repo = Repo::new("paint-panes");
    for scheme in ["dark", "light"] {
        let mut app = repo.app(&format!("[theme]\nscheme = \"{scheme}\"\n"));
        let buf = frame(&mut app);
        assert_eq!(unpainted(&buf), None, "{scheme}");
    }
}

#[test]
fn it_covers_what_clear_wipes_a_popup_the_help_the_toast_and_the_drawer() {
    let repo = Repo::new("paint-overlays");
    for scheme in ["dark", "light"] {
        let mut app = repo.app(&format!("[theme]\nscheme = \"{scheme}\"\n"));
        // The help screen.
        press(&mut app, KeyCode::Char('?'));
        assert_eq!(unpainted(&frame(&mut app)), None, "{scheme} help");
        press(&mut app, KeyCode::Esc);
        // A commit popup.
        press(&mut app, KeyCode::Char('c'));
        assert_eq!(unpainted(&frame(&mut app)), None, "{scheme} popup");
        press(&mut app, KeyCode::Esc);
        // The error toast.
        app.on_remote_done(ferrit::app::events::RemoteOp::Fetch, Err("boom".to_owned()));
        for _ in 0..40 {
            app.advance_clock(Duration::from_millis(16));
        }
        assert_eq!(unpainted(&frame(&mut app)), None, "{scheme} toast");
        press(&mut app, KeyCode::Esc);
        // The settings drawer.
        app.set_author_click_area(Rect::new(0, 0, 6, 1));
        app.feed_mouse(ratatui::crossterm::event::MouseEvent {
            kind: ratatui::crossterm::event::MouseEventKind::Down(
                ratatui::crossterm::event::MouseButton::Left,
            ),
            column: 1,
            row: 0,
            modifiers: ratatui::crossterm::event::KeyModifiers::NONE,
        });
        for _ in 0..30 {
            app.advance_clock(Duration::from_millis(16));
            frame(&mut app);
        }
        assert_eq!(unpainted(&frame(&mut app)), None, "{scheme} drawer");
    }
}

#[test]
fn the_dashboard_and_the_git_config_screen_are_painted_too() {
    let repo = Repo::new("paint-screens");
    let mut app = repo.app("[theme]\nscheme = \"light\"\n");
    press(&mut app, KeyCode::Char('C'));
    assert_eq!(unpainted(&frame(&mut app)), None, "git config");
    press(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::Char('D'));
    assert_eq!(unpainted(&frame(&mut app)), None, "dashboard");
}

#[test]
fn the_default_scheme_is_terminal_and_paints_nothing() {
    let repo = Repo::new("paint-terminal");
    let mut app = repo.app("");
    assert_eq!(Config::default().theme.scheme, SchemeChoice::Terminal);
    let buf = frame(&mut app);
    let reset = (0..buf.area.height)
        .flat_map(|y| (0..buf.area.width).map(move |x| (x, y)))
        .any(|(x, y)| buf[(x, y)].bg == Color::Reset);
    assert!(reset, "the terminal's own background is still there");
    let same = frame(&mut repo.app("[theme]\nscheme = \"terminal\"\n"));
    assert_eq!(buf, same, "naming it changes nothing");
}

#[test]
fn the_painted_background_and_text_are_the_schemes() {
    let repo = Repo::new("paint-colours");
    let mut app = repo.app("[theme]\nscheme = \"light\"\n");
    let buf = frame(&mut app);
    assert!(
        (0..buf.area.height)
            .flat_map(|y| (0..buf.area.width).map(move |x| (x, y)))
            .any(|(x, y)| buf[(x, y)].bg == Scheme::LIGHT.background),
        "white is painted"
    );
    let dark = frame(&mut repo.app("[theme]\nscheme = \"dark\"\n"));
    assert!(
        (0..dark.area.height)
            .flat_map(|y| (0..dark.area.width).map(move |x| (x, y)))
            .any(|(x, y)| dark[(x, y)].bg == Scheme::DARK.background),
        "the dark background is painted"
    );
}

#[test]
fn ansi_names_map_by_where_they_are_used_and_rgb_stays() {
    for scheme in [Scheme::DARK, Scheme::LIGHT] {
        assert_eq!(scheme.paint_foreground(Color::Reset), scheme.text);
        assert_eq!(scheme.paint_background(Color::Reset), scheme.background);
        assert_eq!(scheme.paint_foreground(Color::Green), scheme.foreground[2]);
        assert_eq!(scheme.paint_background(Color::Blue), scheme.fill[4]);
        assert_eq!(
            scheme.paint_background(Color::Black),
            scheme.fill[0],
            "black behind a popup is the dimming layer, not black"
        );
        let tint = Color::Rgb(20, 45, 20);
        assert_eq!(scheme.paint_foreground(tint), tint);
        assert_eq!(scheme.paint_background(tint), tint);
        assert_eq!(
            scheme.paint_background(Color::Indexed(33)),
            Color::Indexed(33)
        );
    }
}

#[test]
fn every_text_colour_reads_on_its_background_in_both_schemes() {
    // Names, in index order, as `Scheme` lays them out.
    let names = [
        "black",
        "red",
        "green",
        "yellow",
        "blue",
        "magenta",
        "cyan",
        "gray",
        "dark gray",
        "light red",
        "light green",
        "light yellow",
        "light blue",
        "light magenta",
        "light cyan",
        "white",
    ];
    for (label, scheme) in [("dark", Scheme::DARK), ("light", Scheme::LIGHT)] {
        for (index, name) in names.iter().enumerate() {
            // Black and white are text on coloured fills (a selection bar, a
            // badge), judged against those fills below, not the background.
            if matches!(*name, "black" | "white") {
                continue;
            }
            let needed = if *name == "dark gray" { 3.0 } else { 4.5 };
            let ratio = contrast(scheme.foreground[index], scheme.background).unwrap();
            assert!(
                ratio >= needed,
                "{label} {name}: {ratio:.2} on the background, wants {needed}"
            );
        }
        let text = contrast(scheme.text, scheme.background).unwrap();
        assert!(text >= 7.0, "{label} text: {text:.2}");
        // White text on the selection bar (the blue fill).
        let bar = contrast(Color::Rgb(255, 255, 255), scheme.fill[4]).unwrap();
        assert!(bar >= 4.5, "{label} selection bar: {bar:.2}");
    }
}

#[test]
fn the_scheme_is_a_config_key_with_a_default_and_a_reported_bad_value() {
    let (config, issues) = Config::parse("");
    assert!(issues.is_empty());
    assert_eq!(config.theme.scheme, SchemeChoice::Terminal);

    for (text, wanted) in [
        ("terminal", SchemeChoice::Terminal),
        ("dark", SchemeChoice::Dark),
        ("light", SchemeChoice::Light),
    ] {
        let (config, issues) = Config::parse(&format!("[theme]\nscheme = \"{text}\"\n"));
        assert!(issues.is_empty(), "{issues:?}");
        assert_eq!(config.theme.scheme, wanted);
    }

    let (config, issues) = Config::parse("[theme]\nscheme = \"sepia\"\n");
    assert_eq!(config.theme.scheme, SchemeChoice::Terminal, "falls back");
    assert!(
        issues.iter().any(|i| i.contains("[theme] ignored")),
        "{issues:?}"
    );
}

#[test]
fn a_painted_scheme_makes_base_irrelevant_and_terminal_keeps_it() {
    let (dark_painted, _) = Config::parse("[theme]\nscheme = \"dark\"\nbase = \"light\"\n");
    assert!(!dark_painted.theme.palette().light, "dark wins over base");
    let (light_painted, _) = Config::parse("[theme]\nscheme = \"light\"\nbase = \"dark\"\n");
    assert!(light_painted.theme.palette().light);
    let (terminal_light, _) = Config::parse("[theme]\nbase = \"light\"\n");
    assert!(terminal_light.theme.palette().light, "as before this phase");
    assert!(terminal_light.theme.scheme().is_none());
}

#[test]
fn the_scheme_survives_a_save_of_the_theme_with_the_rest_of_the_file() {
    let repo = Repo::new("paint-save");
    let file = repo.0.join("config.toml");
    fs::write(&file, "[from_the_future]\nanswer = 42\n").unwrap();
    let mut load = Config::load_from(&file);
    load.config.theme.scheme = SchemeChoice::Light;
    Config::save_sections(&file, &load.config, &[ferrit::app::config::Section::Theme]).unwrap();
    let text = fs::read_to_string(&file).unwrap();
    assert!(text.contains("scheme = \"light\""), "{text}");
    assert!(text.contains("answer = 42"), "{text}");
    assert_eq!(
        Config::load_from(&file).config.theme.scheme,
        SchemeChoice::Light
    );
}
