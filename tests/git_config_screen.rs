#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::pathbuf_init_then_push,
    reason = "integration test scaffolding: a failed setup is the assertion"
)]
//! The git config screen (`docs/PLAN_14_GIT_CONFIG.md`, G4) rendered into a
//! `TestBackend`, through `App` on a fixture repository. The global file is a
//! throwaway one (`App::isolate_git_config`).

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use ferrit::app::App;
use ferrit::interface::screens as ui;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::style::{Color, Modifier};

fn key(c: char) -> KeyEvent {
    KeyEvent::from(KeyCode::Char(c))
}

fn type_text(app: &mut App, text: &str) {
    for c in text.chars() {
        app.feed_key(key(c));
    }
}

struct Fixture {
    dir: PathBuf,
    global: PathBuf,
}

impl Fixture {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("ferrit-{tag}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let fixture = Self {
            global: dir.join("sandbox-global"),
            dir,
        };
        fixture.git(&["init", "-q", "."]);
        fixture.git(&["config", "--local", "pull.rebase", "merges"]);
        fixture.git(&["config", "--global", "pull.rebase", "true"]);
        fixture.git(&["config", "--global", "core.editor", "nvim"]);
        fixture.git(&["config", "--global", "github.token", "ghp_secret"]);
        fixture
    }

    fn git(&self, args: &[&str]) {
        let out = Command::new("git")
            .arg("-C")
            .arg(&self.dir)
            .args(args)
            .env("GIT_CONFIG_GLOBAL", &self.global)
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn app(&self) -> App {
        let mut app = App::open(&self.dir).unwrap();
        app.isolate_git_config(&self.global);
        app.open_git_config();
        app
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn render(app: &mut App, width: u16, height: u16) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|f| ui::draw(f, app)).unwrap();
    terminal.backend().buffer().clone()
}

fn rows(buf: &Buffer) -> Vec<String> {
    (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
        })
        .collect()
}

fn text(buf: &Buffer) -> String {
    rows(buf).join("\n")
}

/// The screen row whose text contains `needle`.
fn row_of(buf: &Buffer, needle: &str) -> u16 {
    u16::try_from(
        rows(buf)
            .iter()
            .position(|r| r.contains(needle))
            .unwrap_or_else(|| panic!("no row with {needle:?} in:\n{}", text(buf))),
    )
    .unwrap()
}

#[test]
fn wide_shows_the_title_the_rules_the_rows_and_the_full_marker() {
    let fx = Fixture::new("gcs-wide");
    let mut app = fx.app();
    let buf = render(&mut app, 120, 30);
    let shown = text(&buf);

    assert!(shown.contains("Git config"));
    assert!(shown.contains("scope for changes: [L]ocal"));
    assert!(shown.contains(&format!("{} keys", app.git_config().total())));
    assert!(shown.contains("── core "));
    assert!(shown.contains("── pull "));
    assert!(shown.contains("← wins (local)"), "{shown}");
    assert!(shown.contains("Edit: e | Add: a | Unset: d | Scope: s | Filter: / | Back: esc"));
    // Two `pull.rebase` rows, the global one first (git's order).
    let first = row_of(&buf, "pull.rebase");
    let line = &rows(&buf)[usize::from(first)];
    assert!(
        line.contains(" G ") || line.contains("G pull.rebase"),
        "{line}"
    );
    assert!(shown.contains("merges") && shown.contains("nvim"));
}

#[test]
fn the_winner_is_green_a_shadowed_value_is_dim_and_the_selection_is_a_bar() {
    let fx = Fixture::new("gcs-style");
    let mut app = fx.app();
    // Select a row that is neither of the two `pull.rebase` ones.
    let at = app
        .git_config()
        .rows
        .iter()
        .position(|r| r.entry.key == "core.editor")
        .unwrap();
    for _ in 0..at {
        app.feed_key(key('j'));
    }
    let buf = render(&mut app, 120, 30);

    let editor = row_of(&buf, "core.editor");
    assert_eq!(
        buf[(5, editor)].style().bg,
        Some(Color::Blue),
        "selection bar"
    );

    let global = row_of(&buf, "G pull.rebase");
    let local = row_of(&buf, "L pull.rebase");
    assert!(
        buf[(5, global)]
            .style()
            .add_modifier
            .contains(Modifier::DIM)
    );
    assert_eq!(buf[(5, local)].style().fg, Some(Color::Green));
}

#[test]
fn a_secret_value_is_never_drawn() {
    let fx = Fixture::new("gcs-secret");
    let mut app = fx.app();
    let buf = render(&mut app, 120, 30);
    let shown = text(&buf);
    assert!(!shown.contains("ghp_secret"));
    let token = &rows(&buf)[usize::from(row_of(&buf, "github.token"))];
    assert!(token.contains("***"), "{token}");
}

#[test]
fn medium_keeps_a_compact_marker_and_narrow_drops_it() {
    let fx = Fixture::new("gcs-widths");
    let mut app = fx.app();

    let medium = text(&render(&mut app, 80, 30));
    assert!(medium.contains('←') && !medium.contains("wins"), "{medium}");
    assert!(medium.contains("pull.rebase") && medium.contains("merges"));

    let narrow = text(&render(&mut app, 50, 30));
    assert!(!narrow.contains('←'), "{narrow}");
    assert!(narrow.contains("pull.rebase") && narrow.contains("merges"));
    assert!(narrow.contains("Git config"));
}

#[test]
fn the_filter_shows_in_the_title_and_an_empty_result_says_so() {
    let fx = Fixture::new("gcs-filter");
    let mut app = fx.app();
    app.feed_key(key('/'));
    type_text(&mut app, "pull");
    let shown = text(&render(&mut app, 120, 30));
    assert!(shown.contains("filter: pull▏"), "{shown}");
    assert!(shown.contains(&format!("2 of {} keys", app.git_config().total())));
    assert!(!shown.contains("core.editor"));

    app.feed_key(KeyEvent::from(KeyCode::Enter));
    app.feed_key(key('/'));
    type_text(&mut app, "zzz");
    let shown = text(&render(&mut app, 120, 30));
    assert!(
        shown.contains("no key or value matches \"pullzzz\""),
        "{shown}"
    );
}

#[test]
fn the_write_scope_and_the_last_action_show() {
    let fx = Fixture::new("gcs-note");
    fx.git(&["config", "--local", "commit.gpgsign", "true"]);
    let mut app = fx.app();
    app.feed_key(key('s'));
    assert!(text(&render(&mut app, 120, 30)).contains("[G]lobal"));
    app.feed_key(key('s'));

    let at = app
        .git_config()
        .rows
        .iter()
        .position(|r| r.entry.key == "commit.gpgsign")
        .unwrap();
    for _ in 0..at {
        app.feed_key(key('j'));
    }
    app.feed_key(key(' '));
    let shown = text(&render(&mut app, 120, 30));
    assert!(shown.contains("commit.gpgsign changed in local"), "{shown}");
}

#[test]
fn a_long_list_scrolls_with_the_selection_and_keeps_it_in_view() {
    let fx = Fixture::new("gcs-scroll");
    for i in 0..60 {
        fx.git(&["config", "--local", &format!("zz.key{i:02}"), "v"]);
    }
    let mut app = fx.app();
    let top = render(&mut app, 100, 14);
    assert!(text(&top).contains("── "));

    app.feed_key(KeyEvent::from(KeyCode::End));
    let bottom = render(&mut app, 100, 14);
    let last = &app.git_config().rows.last().unwrap().entry.key;
    let line = row_of(&bottom, last);
    assert_eq!(
        bottom[(5, line)].style().bg,
        Some(Color::Blue),
        "the last row is selected and visible"
    );

    app.feed_key(KeyEvent::from(KeyCode::Home));
    let top = render(&mut app, 100, 14);
    let first = &app.git_config().rows.first().unwrap().entry.key;
    assert_eq!(top[(5, row_of(&top, first))].style().bg, Some(Color::Blue));
    // Moving down one row at a time never loses the selection.
    for _ in 0..30 {
        app.feed_key(key('j'));
        let buf = render(&mut app, 100, 14);
        assert!(
            (0..buf.area.height).any(|y| buf[(5, y)].style().bg == Some(Color::Blue)),
            "selection out of view at row {}",
            app.git_config().selected
        );
    }
}

#[test]
fn an_included_value_says_so_and_a_long_value_is_cut() {
    let fx = Fixture::new("gcs-include");
    fs::write(fx.dir.join(".git/extra"), "[inc]\n\tk = v\n").unwrap();
    fx.git(&["config", "--local", "include.path", "extra"]);
    let long = "x".repeat(300);
    fx.git(&["config", "--local", "zz.long", &long]);
    let mut app = fx.app();
    let shown = text(&render(&mut app, 120, 30));
    assert!(shown.contains("inherited"), "{shown}");
    assert!(shown.contains('…'));
    assert!(!shown.contains(&"x".repeat(200)));
}

#[test]
fn a_terminal_too_small_to_draw_a_list_does_not_panic() {
    let fx = Fixture::new("gcs-tiny");
    let mut app = fx.app();
    for (w, h) in [(1, 1), (10, 2), (20, 3), (30, 1)] {
        let _ = render(&mut app, w, h);
    }
}
