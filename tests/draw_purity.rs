#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "integration test scaffolding: a failed setup is the assertion"
)]
//! Drawing is a function of the state: it shows the app and records where things
//! landed (for the mouse), and it changes nothing else. These pin that on every
//! screen before `screens::draw` is reshaped; see `docs/PLAN_24_DRAW_VIEW.md`.
//!
//! Two properties, each on every screen and at two terminal sizes:
//! - drawing twice gives the same frame, so a frame never depends on the one
//!   before it (a read of an animation that went missing would show here);
//! - drawing leaves the state a user can observe as it was.

mod common;

use std::fs;
use std::time::Duration;

use common::TempDir;
use ferrit::app::config::ConfigLoad;
use ferrit::app::{App, Pane, screens as ui};
use ferrit::domain::git::error::GitError;
use ferrit::domain::git::fake::FakeGit;
use ferrit::domain::git::model::Change;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;

const SIZES: [(u16, u16); 2] = [(120, 40), (60, 20)];
const PANES: [Pane; 5] = [
    Pane::Status,
    Pane::Files,
    Pane::Branches,
    Pane::Commits,
    Pane::Stash,
];

fn frame(app: &mut App, (width, height): (u16, u16)) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|f| ui::draw_painted(f, app)).unwrap();
    terminal.backend().to_string()
}

fn settle(app: &mut App) {
    for _ in 0..40 {
        app.advance_clock(Duration::from_millis(16));
    }
}

fn key(app: &mut App, c: char) {
    app.feed_key(KeyEvent::from(KeyCode::Char(c)));
}

fn fake_app() -> App {
    let fake = FakeGit::new("purity")
        .with_commit("init")
        .with_commit("second")
        .with_branch("feat")
        .with_file("a.txt", Change::Modified, Change::None)
        .with_file("src/b.rs", Change::None, Change::Untracked);
    App::with_git(Box::new(fake))
}

/// What a user can observe without looking at pixels.
fn observable(app: &mut App) -> String {
    let status: Vec<String> = app.status_lines().iter().map(ToString::to_string).collect();
    let rows: Vec<(usize, usize)> = PANES
        .iter()
        .map(|p| (app.row_count(*p), app.selected(*p)))
        .collect();
    let popup = app.commit_popup().is_some();
    format!(
        "{status:?} {rows:?} focus={:?} popup={popup} confirm={:?} sheet={} help={} full={:?}",
        app.nav.focus,
        app.confirm_message(),
        app.sheet_is_open(),
        app.help.open,
        app.full_screen(),
    )
}

/// Every screen the drawing code has a branch for, and the folder the welcome
/// screen is about, which must outlive them.
fn screens() -> (Vec<(&'static str, App)>, TempDir) {
    let mut all = Vec::new();

    all.push(("panes", fake_app()));

    let mut app = fake_app();
    key(&mut app, '?');
    settle(&mut app);
    all.push(("help", app));

    let mut app = fake_app();
    key(&mut app, 'c');
    settle(&mut app);
    all.push(("commit popup", app));

    let fake = FakeGit::new("purity")
        .with_commit("init")
        .with_file("a.txt", Change::Modified, Change::None)
        .fail_next("commit", GitError::CommitFailed("hook said no".to_owned()));
    let mut app = App::with_git(Box::new(fake));
    key(&mut app, 'c');
    for c in "feat: x".chars() {
        key(&mut app, c);
    }
    app.feed_key(KeyEvent::from(KeyCode::Enter));
    settle(&mut app);
    all.push(("commit popup and error toast", app));

    let mut app = fake_app();
    app.nav.focus = Pane::Branches;
    app.select(Pane::Branches, 1);
    key(&mut app, 'd');
    all.push(("key-bar question", app));

    let mut app = fake_app();
    key(&mut app, 'D');
    settle(&mut app);
    all.push(("dashboard sheet", app));

    let mut app = fake_app();
    app.set_author_click_area(Rect::new(0, 0, 6, 1));
    app.feed_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 1,
        row: 0,
        modifiers: KeyModifiers::NONE,
    });
    settle(&mut app);
    all.push(("settings sheet", app));

    let mut app = fake_app();
    key(&mut app, 'C');
    all.push(("git config", app));

    let mut app = fake_app();
    key(&mut app, '@');
    all.push(("command log", app));

    let dir = TempDir::new("purity-welcome");
    fs::create_dir_all(dir.path()).unwrap();
    all.push(("welcome", App::welcome(dir.path(), ConfigLoad::default())));
    (all, dir)
}

#[test]
fn drawing_twice_gives_the_same_frame_on_every_screen() {
    let (all, _folder) = screens();
    for (name, mut app) in all {
        for size in SIZES {
            let first = frame(&mut app, size);
            let second = frame(&mut app, size);
            assert_eq!(
                first, second,
                "{name} at {size:?}: the second frame differs"
            );
        }
    }
}

#[test]
fn drawing_changes_nothing_a_user_can_observe() {
    let (all, _folder) = screens();
    for (name, mut app) in all {
        for size in SIZES {
            let before = observable(&mut app);
            let _ = frame(&mut app, size);
            let after = observable(&mut app);
            assert_eq!(
                before, after,
                "{name} at {size:?}: drawing changed the state"
            );
        }
    }
}

#[test]
fn what_a_frame_learned_reaches_the_mouse() {
    // Draw, then click on the middle of the Branches pane: the app routes the click
    // by the rectangles the frame recorded, so focus must move there.
    let mut app = fake_app();
    let _ = frame(&mut app, (120, 40));
    app.feed_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 5,
        row: 22,
        modifiers: KeyModifiers::NONE,
    });
    assert_ne!(
        app.nav.focus,
        Pane::Files,
        "a click low in the left column did not change the focus"
    );
}
