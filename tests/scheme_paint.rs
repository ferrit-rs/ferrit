#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "integration test scaffolding: a failed setup is the assertion"
)]
//! Painted themes (`docs/PLAN_18_THEMES.md`): `[theme] base = "dark" | "light"`
//! paints the whole frame, whichever the terminal's own background is.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use ferrit::app::config::{Config, ConfigLoad};
use ferrit::app::theme_config::Base;
use ferrit::app::{App, screens as ui};
use ferrit::components::ui::scheme::{ColorDepth, Scheme, contrast};
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
    terminal.draw(|f| ui::draw_painted(f, app)).unwrap();
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
        let mut app = repo.app(&format!("[theme]\nbase = \"{scheme}\"\n"));
        let buf = frame(&mut app);
        assert_eq!(unpainted(&buf), None, "{scheme}");
    }
}

#[test]
fn it_covers_what_clear_wipes_a_popup_the_help_the_toast_and_the_drawer() {
    let repo = Repo::new("paint-overlays");
    for scheme in ["dark", "light"] {
        let mut app = repo.app(&format!("[theme]\nbase = \"{scheme}\"\n"));
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
    let mut app = repo.app("[theme]\nbase = \"light\"\n");
    press(&mut app, KeyCode::Char('C'));
    assert_eq!(unpainted(&frame(&mut app)), None, "git config");
    press(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::Char('D'));
    assert_eq!(unpainted(&frame(&mut app)), None, "dashboard");
}

#[test]
fn with_no_config_ferrit_is_dark_and_painted() {
    let repo = Repo::new("paint-default");
    let mut app = repo.app("");
    assert_eq!(Config::default().theme.base, Base::Dark);
    let buf = frame(&mut app);
    assert_eq!(unpainted(&buf), None);
    assert!(
        (0..buf.area.height)
            .flat_map(|y| (0..buf.area.width).map(move |x| (x, y)))
            .any(|(x, y)| buf[(x, y)].bg == Scheme::DARK.background),
        "the dark background is there without any setting"
    );
}

#[test]
fn the_painted_background_and_text_are_the_schemes() {
    let repo = Repo::new("paint-colours");
    let mut app = repo.app("[theme]\nbase = \"light\"\n");
    let buf = frame(&mut app);
    assert!(
        (0..buf.area.height)
            .flat_map(|y| (0..buf.area.width).map(move |x| (x, y)))
            .any(|(x, y)| buf[(x, y)].bg == Scheme::LIGHT.background),
        "white is painted"
    );
    let dark = frame(&mut repo.app("[theme]\nbase = \"dark\"\n"));
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
fn the_theme_is_a_config_key_with_a_default_and_a_reported_bad_value() {
    let (config, issues) = Config::parse("");
    assert!(issues.is_empty());
    assert_eq!(config.theme.base, Base::Dark);

    for (text, wanted) in [("dark", Base::Dark), ("light", Base::Light)] {
        let (config, issues) = Config::parse(&format!("[theme]\nbase = \"{text}\"\n"));
        assert!(issues.is_empty(), "{issues:?}");
        assert_eq!(config.theme.base, wanted);
    }

    // "terminal" was a value of a plan that was dropped: it is a bad value now.
    for bad in ["sepia", "terminal"] {
        let (config, issues) = Config::parse(&format!("[theme]\nbase = \"{bad}\"\n"));
        assert_eq!(config.theme.base, Base::Dark, "falls back");
        assert!(
            issues.iter().any(|i| i.contains("[theme] ignored")),
            "{bad}: {issues:?}"
        );
    }
}

#[test]
fn the_palette_and_the_painted_scheme_both_follow_base() {
    let (dark, _) = Config::parse("[theme]\nbase = \"dark\"\n");
    assert!(!dark.theme.palette().light);
    assert_eq!(dark.theme.scheme(), Scheme::DARK);
    let (light, _) = Config::parse("[theme]\nbase = \"light\"\n");
    assert!(light.theme.palette().light);
    assert_eq!(light.theme.scheme(), Scheme::LIGHT);
}

#[test]
fn the_theme_survives_a_save_with_the_rest_of_the_file() {
    let repo = Repo::new("paint-save");
    let file = repo.0.join("config.toml");
    fs::write(&file, "[from_the_future]\nanswer = 42\n").unwrap();
    let mut load = Config::load_from(&file);
    load.config.theme.base = Base::Light;
    Config::save_sections(&file, &load.config, &[ferrit::app::config::Section::Theme]).unwrap();
    let text = fs::read_to_string(&file).unwrap();
    assert!(text.contains("base = \"light\""), "{text}");
    assert!(text.contains("answer = 42"), "{text}");
    assert_eq!(Config::load_from(&file).config.theme.base, Base::Light);
}

fn colours(buf: &Buffer) -> Vec<Color> {
    (0..buf.area.height)
        .flat_map(|y| (0..buf.area.width).map(move |x| (x, y)))
        .flat_map(|(x, y)| [buf[(x, y)].fg, buf[(x, y)].bg])
        .collect()
}

#[test]
fn nearest_256_picks_the_cube_or_the_grey_ramp_whichever_is_closer() {
    use ferrit::components::ui::scheme::nearest_256;
    assert_eq!(nearest_256(0, 0, 0), 16, "black is the cube's corner");
    assert_eq!(nearest_256(255, 255, 255), 231, "white is the other");
    assert_eq!(nearest_256(255, 0, 0), 196);
    assert_eq!(nearest_256(0, 255, 0), 46);
    assert_eq!(nearest_256(0, 0, 255), 21);
    assert_eq!(nearest_256(128, 128, 128), 244, "a mid grey is on the ramp");
    assert_eq!(nearest_256(8, 8, 8), 232, "the first grey");
    assert_eq!(nearest_256(238, 238, 238), 255, "the last grey");
    assert_eq!(nearest_256(95, 135, 175), 67, "an exact cube colour");
    // The two backgrounds stay a dark one and a light one.
    let Color::Rgb(r, g, b) = Scheme::DARK.background else {
        panic!("rgb")
    };
    assert_eq!(
        nearest_256(r, g, b),
        233,
        "a near-black grey, not the cube's black"
    );
    let Color::Rgb(r, g, b) = Scheme::LIGHT.background else {
        panic!("rgb")
    };
    assert_eq!(nearest_256(r, g, b), 231);
}

#[test]
fn a_terminal_says_it_speaks_24_bit_colour_with_colorterm() {
    for yes in ["truecolor", "24bit", "TrueColor"] {
        assert_eq!(
            ColorDepth::detect(Some(yes)),
            ColorDepth::TrueColor,
            "{yes}"
        );
    }
    for no in [Some(""), Some("yes"), None] {
        assert_eq!(ColorDepth::detect(no), ColorDepth::Indexed, "{no:?}");
    }
}

#[test]
fn without_24_bit_colour_the_frame_has_no_rgb_and_is_still_painted() {
    let repo = Repo::new("paint-256");
    for theme in ["dark", "light"] {
        let mut app = repo.app(&format!("[theme]\nbase = \"{theme}\"\n"));
        app.set_color_depth(ColorDepth::Indexed);
        let buf = frame(&mut app);
        assert_eq!(unpainted(&buf), None, "{theme}");
        assert!(
            !colours(&buf).iter().any(|c| matches!(c, Color::Rgb(..))),
            "{theme}: an Rgb is left for a terminal that cannot show it"
        );
        assert!(
            colours(&buf).iter().any(|c| matches!(c, Color::Indexed(_))),
            "{theme}"
        );
    }
    let mut app = repo.app("");
    let buf = frame(&mut app);
    assert!(colours(&buf).iter().any(|c| matches!(c, Color::Rgb(..))));
    assert!(
        !colours(&buf).iter().any(|c| matches!(c, Color::Indexed(_))),
        "with 24-bit colour nothing is approximated"
    );
}

#[test]
fn what_a_popup_dims_is_the_dimming_layer_of_the_theme_not_black() {
    let repo = Repo::new("paint-dim");
    for (theme, scheme) in [("dark", Scheme::DARK), ("light", Scheme::LIGHT)] {
        let mut app = repo.app(&format!("[theme]\nbase = \"{theme}\"\n"));
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
        let buf = frame(&mut app);
        // Left of the drawer: the screen behind it, dimmed.
        let dimmed = &buf[(2, 20)];
        assert_eq!(dimmed.bg, scheme.fill[0], "{theme}: the dimming layer");
        assert_ne!(dimmed.bg, Color::Rgb(0, 0, 0), "{theme}: not black");
        assert_eq!(dimmed.fg, scheme.foreground[8], "{theme}: dim text");
    }
}

#[test]
fn dimmed_text_is_still_faintly_readable_in_both_themes() {
    for (label, scheme) in [("dark", Scheme::DARK), ("light", Scheme::LIGHT)] {
        let ratio = contrast(scheme.foreground[8], scheme.fill[0]).unwrap();
        assert!(ratio >= 2.0, "{label}: dimmed text {ratio:.2}");
        let back = contrast(scheme.fill[0], scheme.background).unwrap();
        assert!(
            back < 1.5,
            "{label}: the dim layer is close to the screen: {back:.2}"
        );
    }
}
