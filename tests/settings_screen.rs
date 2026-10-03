#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "integration test scaffolding: a failed setup is the assertion"
)]
//! The settings sheet rendered into a `TestBackend` and driven by keys and
//! clicks (`docs/PLAN_17_SETTINGS.md`, U2). Ferrit's own settings only.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use ferrit::app::config::Config;
use ferrit::app::{App, screens as ui};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;

struct Fixture {
    dir: PathBuf,
}

impl Fixture {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = fs::canonicalize(std::env::temp_dir())
            .unwrap()
            .join(format!("ferrit-{tag}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(dir.join("repo")).unwrap();
        let status = Command::new("git")
            .arg("-C")
            .arg(dir.join("repo"))
            .args(["init", "-q", "."])
            .status()
            .unwrap();
        assert!(status.success());
        Self { dir }
    }

    fn file(&self) -> PathBuf {
        self.dir.join("config.toml")
    }

    /// An app whose sheet is open, its config in `config.toml` here.
    fn app_with_sheet(&self) -> App {
        let mut app =
            App::open_with(&self.dir.join("repo"), Config::load_from(&self.file())).unwrap();
        app.set_author_click_area(Rect::new(0, 0, 6, 1));
        click(&mut app, 1, 0);
        settle(&mut app);
        app
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn render(app: &mut App, width: u16, height: u16) -> Vec<String> {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|f| ui::draw(f, app)).unwrap();
    let buf = terminal.backend().buffer().clone();
    (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
        })
        .collect()
}

fn shown(app: &mut App) -> String {
    render(app, 120, 50).join("\n")
}

/// Let the slide-in finish.
fn settle(app: &mut App) {
    for _ in 0..30 {
        app.advance_clock(Duration::from_millis(16));
        render(app, 120, 50);
    }
}

fn click(app: &mut App, column: u16, row: u16) {
    app.feed_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    });
}

fn press(app: &mut App, code: KeyCode) {
    app.feed_key(KeyEvent::from(code));
}

/// Where `needle` first shows on screen: (column, row).
fn find(app: &mut App, needle: &str) -> (u16, u16) {
    for (row, line) in render(app, 120, 50).iter().enumerate() {
        if let Some(byte) = line.find(needle) {
            let column = line[..byte].chars().count();
            return (u16::try_from(column).unwrap(), u16::try_from(row).unwrap());
        }
    }
    panic!("{needle:?} not on screen:\n{}", shown(app));
}

#[test]
fn the_sheet_holds_ferrits_settings_and_nothing_of_git_or_the_dashboard() {
    let fx = Fixture::new("sheet-content");
    let mut app = fx.app_with_sheet();
    let text = shown(&mut app);
    for wanted in [
        "Settings",
        "Appearance",
        "Theme",
        "Accent",
        "Interface",
        "Mouse",
        "Wheel step",
        "Refresh every",
        "Diff",
        "Context lines",
        "Ignore whitespace",
        "Commit",
        "Sign-off by default",
        "Command log",
        "Show read commands",
        "Pick a color",
    ] {
        assert!(text.contains(wanted), "{wanted} in {text}");
    }
    for gone in [
        "Global Git users",
        "Git config identity",
        "Contributors",
        "Recent",
        "Profile",
        "[ Save ]",
    ] {
        assert!(!text.contains(gone), "{gone} in {text}");
    }
    assert!(
        text.contains("\u{25b8} Theme"),
        "the first row is selected: {text}"
    );
    assert!(
        text.contains("(\u{2022}) Dark") && text.contains("( ) Light"),
        "{text}"
    );
    assert!(text.contains("10 s"), "the refresh interval: {text}");
}

#[test]
fn the_arrows_move_between_rows_and_change_the_value_and_the_file_follows() {
    let fx = Fixture::new("sheet-keys");
    let mut app = fx.app_with_sheet();
    press(&mut app, KeyCode::Right); // Theme -> light
    let text = shown(&mut app);
    assert!(text.contains("(\u{2022}) Light"), "{text}");
    assert!(
        text.contains("Saved \u{b7}"),
        "the footer says it is saved: {text}"
    );
    assert!(app.palette().light);
    assert!(
        fs::read_to_string(fx.file())
            .unwrap()
            .contains("base = \"light\"")
    );

    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Down);
    assert!(shown(&mut app).contains("\u{25b8} Mouse"));
    press(&mut app, KeyCode::Char(' '));
    assert!(shown(&mut app).contains("[ ]"), "the mouse toggle is off");
    assert!(!Config::load_from(&fx.file()).config.ui.mouse);

    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Left);
    assert_eq!(Config::load_from(&fx.file()).config.ui.wheel_step, 2);
    assert!(shown(&mut app).contains("\u{2039} 2 \u{203a}"));
}

#[test]
fn a_click_on_a_radio_a_checkbox_and_an_arrow_sets_that_value() {
    let fx = Fixture::new("sheet-clicks");
    let mut app = fx.app_with_sheet();

    let (x, y) = find(&mut app, "( ) Light");
    click(&mut app, x + 1, y);
    assert!(app.palette().light, "the Light radio");

    let (x, y) = find(&mut app, "\u{25cb} Purple");
    click(&mut app, x, y);
    assert_eq!(
        Config::load_from(&fx.file()).config.theme.preset,
        ferrit::app::theme_config::Preset::Purple
    );

    let (x, y) = find(&mut app, "Context lines");
    click(&mut app, x + 30, y); // on the row's line, away from its value: highlights it
    assert!(shown(&mut app).contains("\u{25b8} Context lines"));
}

#[test]
fn the_number_arrows_step_the_value() {
    let fx = Fixture::new("sheet-steps");
    let mut app = fx.app_with_sheet();
    let (_, row) = find(&mut app, "Wheel step");
    let line = render(&mut app, 120, 50)[usize::from(row)].clone();
    let up = u16::try_from(line.chars().position(|c| c == '\u{203a}').unwrap()).unwrap();
    click(&mut app, up, row);
    assert_eq!(Config::load_from(&fx.file()).config.ui.wheel_step, 4);
    let down = u16::try_from(line.chars().position(|c| c == '\u{2039}').unwrap()).unwrap();
    click(&mut app, down, row);
    click(&mut app, down, row);
    assert_eq!(Config::load_from(&fx.file()).config.ui.wheel_step, 2);
}

#[test]
fn enter_on_the_accent_opens_the_picker_and_a_picked_colour_is_kept() {
    let fx = Fixture::new("sheet-picker");
    let mut app = fx.app_with_sheet();
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Enter);
    assert!(
        shown(&mut app).contains("arrows colour"),
        "the picker's hint"
    );

    press(&mut app, KeyCode::Right);
    press(&mut app, KeyCode::Right);
    let saved = Config::load_from(&fx.file()).config.theme;
    assert!(
        saved.accent.is_some(),
        "a picked colour is on disk: {saved:?}"
    );

    press(&mut app, KeyCode::Char('e'));
    assert!(shown(&mut app).contains("RGB: [R"), "the RGB editor");
    press(&mut app, KeyCode::Up);
    let after = Config::load_from(&fx.file()).config.theme;
    assert_ne!(
        after.accent, saved.accent,
        "the channel moved and was saved"
    );

    press(&mut app, KeyCode::Esc); // RGB -> picker
    press(&mut app, KeyCode::Esc); // picker -> rows
    assert!(
        shown(&mut app).contains("Settings"),
        "the sheet is still up"
    );
    press(&mut app, KeyCode::Esc); // closes
    settle(&mut app);
    assert!(!shown(&mut app).contains("Wheel step"), "the sheet closed");
}

#[test]
fn a_click_outside_the_sheet_closes_it() {
    let fx = Fixture::new("sheet-outside");
    let mut app = fx.app_with_sheet();
    click(&mut app, 2, 20);
    settle(&mut app);
    assert!(!shown(&mut app).contains("Wheel step"));
}

#[test]
fn an_invalid_config_file_is_left_alone_and_the_footer_says_so() {
    let fx = Fixture::new("sheet-broken");
    fs::write(fx.file(), "[oops\n").unwrap();
    let mut app = fx.app_with_sheet();
    press(&mut app, KeyCode::Right);
    let text = shown(&mut app);
    assert!(text.contains("Not saved:"), "{text}");
    assert_eq!(fs::read_to_string(fx.file()).unwrap(), "[oops\n");
    assert!(app.palette().light, "it still applies for this run");
}

#[test]
fn without_a_config_file_the_footer_says_changes_last_for_this_run() {
    let fx = Fixture::new("sheet-nofile");
    let mut app = App::open(&fx.dir.join("repo")).unwrap();
    app.set_author_click_area(Rect::new(0, 0, 6, 1));
    click(&mut app, 1, 0);
    settle(&mut app);
    assert!(shown(&mut app).contains("changes last for this run only"));
}

#[test]
fn small_terminals_do_not_panic_and_a_short_one_scrolls_to_the_selected_row() {
    let fx = Fixture::new("sheet-small");
    let mut app = fx.app_with_sheet();
    for (w, h) in [(1, 1), (10, 3), (30, 5), (50, 8), (80, 12)] {
        render(&mut app, w, h);
    }
    for _ in 0..8 {
        press(&mut app, KeyCode::Down);
    }
    let text = render(&mut app, 80, 12).join("\n");
    assert!(text.contains("\u{25b8} Show read commands"), "{text}");
}
