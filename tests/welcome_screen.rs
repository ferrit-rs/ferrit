#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "integration test scaffolding: a failed setup is the assertion"
)]
//! The welcome screen rendered into a `TestBackend`
//! (`docs/PLAN_16_START_WITHOUT_REPO.md`, W2).

use std::path::Path;

use ferrit::config::ConfigLoad;
use ferrit::tui::App;
use ferrit::tui::screens as ui;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::style::Color;

fn render(app: &mut App, width: u16, height: u16) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|f| ui::draw(f, app)).unwrap();
    terminal.backend().buffer().clone()
}

fn text(buf: &Buffer) -> String {
    (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn welcome(dir: &str) -> App {
    App::welcome(Path::new(dir), ConfigLoad::default())
}

#[test]
fn the_dialog_names_the_folder_and_lists_both_choices() {
    // A folder that does not exist keeps its name as given.
    let mut app = welcome("/no/such/folder/new-project");
    let shown = text(&render(&mut app, 100, 24));
    assert!(shown.contains("/no/such/folder/new-project"), "{shown}");
    assert!(shown.contains("is not a git repository."), "{shown}");
    assert!(
        shown.contains("i   Initialise a repository here (git init)"),
        "{shown}"
    );
    assert!(shown.contains("q   Quit"), "{shown}");
    assert!(
        shown.contains("Move: \u{2191}/\u{2193} | Choose: enter | Init: i | Quit: q"),
        "the key bar: {shown}"
    );
    assert!(!shown.contains("[2] Files"), "no pane is drawn behind it");
}

#[test]
fn the_dialog_is_centred() {
    let mut app = welcome("/no/such/folder");
    let buf = render(&mut app, 100, 24);
    let rows: Vec<String> = (0..buf.area.height)
        .map(|y| (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect())
        .collect();
    let top = rows.iter().position(|r| r.contains('\u{256d}')).unwrap();
    let bottom = rows.iter().rposition(|r| r.contains('\u{2570}')).unwrap();
    let above = top;
    let below = rows.len() - 1 - bottom - 1; // the key bar takes the last row
    assert!(above.abs_diff(below) <= 1, "above {above}, below {below}");
}

#[test]
fn a_long_folder_is_cut_in_the_middle_and_keeps_its_end() {
    let long = "/very/long/path/that/goes/on/and/on/and/on/through/many/nested/folders/new-project";
    let mut app = welcome(long);
    let shown = text(&render(&mut app, 60, 20));
    assert!(shown.contains('\u{2026}'), "{shown}");
    assert!(
        shown.contains("new-project"),
        "the end tells folders apart: {shown}"
    );
    assert!(!shown.contains(long));
}

#[test]
fn the_question_replaces_the_key_bar_and_names_the_folder() {
    let mut app = welcome("/no/such/folder/new-project");
    app.feed_key(KeyEvent::from(KeyCode::Char('i')));
    let shown = text(&render(&mut app, 120, 24));
    assert!(
        shown.contains("run git init in /no/such/folder/new-project?"),
        "{shown}"
    );
    assert!(!shown.contains("Choose: enter"));
}

#[test]
fn narrow_and_tiny_terminals_do_not_panic() {
    let mut app = welcome("/no/such/folder/new-project");
    for (w, h) in [(120, 40), (80, 24), (40, 12), (20, 6), (10, 3), (1, 1)] {
        let _ = render(&mut app, w, h);
    }
    app.feed_key(KeyEvent::from(KeyCode::Char('i')));
    for (w, h) in [(40, 12), (10, 3), (1, 1)] {
        let _ = render(&mut app, w, h);
    }
}

/// The row (screen line) holding `needle`.
fn row_of(buf: &Buffer, needle: &str) -> u16 {
    let at = text(buf).lines().position(|l| l.contains(needle)).unwrap();
    u16::try_from(at).unwrap()
}

#[test]
fn the_highlight_is_a_bar_with_the_marker_and_it_follows_the_arrows() {
    let mut app = welcome("/no/such/folder");
    let first = render(&mut app, 100, 24);
    let init = row_of(&first, "Initialise a repository");
    let quit = row_of(&first, "q   Quit");
    let bar = |buf: &Buffer, row: u16| {
        (0..buf.area.width).any(|x| buf[(x, row)].style().bg == Some(Color::Blue))
    };
    assert!(
        bar(&first, init) && !bar(&first, quit),
        "git init is highlighted first"
    );
    assert!(
        text(&first).contains("\u{25b8} i   Initialise"),
        "{}",
        text(&first)
    );
    assert!(!text(&first).contains("\u{25b8} q"));

    app.feed_key(KeyEvent::from(KeyCode::Down));
    let second = render(&mut app, 100, 24);
    assert!(
        bar(&second, quit) && !bar(&second, init),
        "the bar moved to quit"
    );
    assert!(
        text(&second).contains("\u{25b8} q   Quit"),
        "{}",
        text(&second)
    );
    assert!(!text(&second).contains("\u{25b8} i"));
}
