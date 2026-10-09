#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::pathbuf_init_then_push,
    clippy::iter_on_single_items,
    clippy::format_collect,
    elided_lifetimes_in_paths,
    reason = "integration test scaffolding: a failed setup is the assertion, helper ergonomics beat lint-cleanliness here"
)]
//! The command log on screen (`docs/PLAN_12_POLISH.md` P0c): the panel shows
//! the newest commands ferrit ran, failures are marked, and `@` opens a
//! scrollable viewer.
//!
//! The log is process-wide and every test here writes to it, so they run one
//! at a time behind `serial()`.

mod common;

use common::{TempDir, commit_all, configure_identity, git};
use std::fs;
use std::sync::{Mutex, MutexGuard, PoisonError};

use ferrit::config::{Config, ConfigLoad};
use ferrit::git::repo::Repo;
use ferrit::tui::App;
use ferrit::tui::draw as ui;
use git2::Repository;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent};

fn char_key(c: char) -> KeyEvent {
    KeyEvent::from(KeyCode::Char(c))
}

fn serial() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(PoisonError::into_inner)
}

fn frame(app: &mut App, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|f| ui::draw(f, app)).unwrap();
    terminal.backend().to_string()
}

/// `a.txt` committed, then edited so the Files pane has a row to stage.
fn dirty_app(tag: &str) -> (TempDir, App) {
    let dir = TempDir::new(tag);
    let repo = Repository::init(dir.path()).unwrap();
    configure_identity(dir.path());
    git(dir.path(), &["checkout", "-q", "-b", "main"]);
    fs::write(dir.path().join("a.txt"), "one\n").unwrap();
    commit_all(&repo, "init");
    fs::write(dir.path().join("a.txt"), "one\ntwo\n").unwrap();
    let mut app = App::open(dir.path()).unwrap();
    app.feed_key(char_key('2'));
    (dir, app)
}

#[test]
fn the_panel_shows_the_command_ferrit_just_ran() {
    let _serial = serial();
    let (_dir, mut app) = dirty_app("log-panel");
    app.feed_key(char_key('j')); // off the root row
    app.feed_key(char_key(' ')); // stage a.txt

    let out = frame(&mut app, 120, 40);
    assert!(out.contains("$ git add -- a.txt"), "{out}");
    assert!(
        !out.contains("git status --porcelain"),
        "the hard-coded sample is gone for a real repo:\n{out}"
    );
}

#[test]
fn a_failed_command_is_marked_with_its_exit_code() {
    let _serial = serial();
    let (_dir, mut app) = dirty_app("log-failed");
    app.feed_key(char_key('3')); // Branches, `main` checked out
    app.feed_key(char_key('d')); // git refuses to delete the current branch

    let out = frame(&mut app, 120, 40);
    assert!(out.contains("git branch -d main"), "{out}");
    assert!(out.contains("(exit 1)"), "{out}");
}

#[test]
fn the_author_label_shows_even_with_no_command_to_its_left() {
    let _serial = serial();
    let (_dir, mut app) = dirty_app("log-author");
    let out = frame(&mut app, 120, 40);
    assert!(out.contains("Test"), "{out}");
}

#[test]
fn at_opens_the_viewer_and_esc_closes_it() {
    let _serial = serial();
    let (_dir, mut app) = dirty_app("log-viewer");
    app.feed_key(char_key('j')); // off the root row
    app.feed_key(char_key(' '));
    app.feed_key(char_key('@'));

    let out = frame(&mut app, 120, 40);
    assert!(out.contains("command log"), "{out}");
    assert!(out.contains("$ git add -- a.txt"), "{out}");
    assert!(out.contains("Esc close"), "{out}");

    app.feed_key(KeyEvent::from(KeyCode::Esc));
    assert!(!frame(&mut app, 120, 40).contains("Esc close"));
    app.feed_key(char_key('@'));
    app.feed_key(char_key('@'));
    assert!(
        !frame(&mut app, 120, 40).contains("Esc close"),
        "@ again closes it"
    );
}

#[test]
fn the_viewer_lists_reads_the_panel_hides() {
    let _serial = serial();
    let (_dir, mut app) = dirty_app("log-reads");
    // Selecting a file loads its diff, a `git diff` read.
    app.feed_key(char_key('j'));
    app.feed_key(char_key('k'));
    let panel = frame(&mut app, 120, 40);
    assert!(
        !panel.contains("$ git diff"),
        "reads stay out of the panel:\n{panel}"
    );

    app.feed_key(char_key('@'));
    let viewer = frame(&mut app, 120, 40);
    assert!(viewer.contains("$ git diff"), "{viewer}");
}

#[test]
fn the_viewer_scrolls_from_the_newest_to_the_oldest() {
    let _serial = serial();
    let (dir, mut app) = dirty_app("log-scroll");
    let repo = Repo::open(dir.path()).unwrap();
    for i in 0..60 {
        let _ = repo.checkout(&format!("scrollmark-{i:02}"));
    }
    app.feed_key(char_key('@'));

    // The marker is not part of the temp directory name, which the Status pane
    // shows and which carries the process id (a pid starting with 30 once made
    // `log-scroll-30` match it).
    // The panel behind the viewer always shows the two newest commands
    // (58 and 59), so these checks use entries only the viewer can show.
    let newest = frame(&mut app, 100, 24);
    assert!(newest.contains("scrollmark-50"), "{newest}");
    assert!(!newest.contains("scrollmark-30"), "{newest}");

    for _ in 0..25 {
        app.feed_key(char_key('k'));
    }
    let scrolled = frame(&mut app, 100, 24);
    assert!(scrolled.contains("scrollmark-30"), "{scrolled}");
    assert!(!scrolled.contains("scrollmark-50"), "{scrolled}");

    app.feed_key(char_key('G'));
    assert!(frame(&mut app, 100, 24).contains("scrollmark-50"));
    app.feed_key(char_key('g'));
    let oldest = frame(&mut app, 100, 24);
    assert!(!oldest.contains("scrollmark-50"), "{oldest}");
}

#[test]
fn other_keys_are_swallowed_while_the_viewer_is_up() {
    let _serial = serial();
    let (dir, mut app) = dirty_app("log-swallow");
    app.feed_key(char_key('@'));
    app.feed_key(char_key(' ')); // would stage a.txt on Files

    assert!(
        git(dir.path(), &["diff", "--cached", "--name-only"]).is_empty(),
        "the viewer owns input"
    );
}

#[test]
fn show_reads_lists_read_only_commands_in_the_panel() {
    let _serial = serial();
    let (dir, _app) = dirty_app("log-show-reads");
    let mut config = Config::default();
    config.log.show_reads = true;
    let mut app = App::open_with(
        dir.path(),
        ConfigLoad {
            config,
            file: None,
            issues: Vec::new(),
        },
    )
    .unwrap();
    app.feed_key(char_key('2'));
    app.feed_key(char_key('j'));
    app.feed_key(char_key('k')); // selecting a file loads its diff, a read

    let panel = frame(&mut app, 120, 40);
    assert!(
        panel.contains("$ git diff"),
        "reads shown when asked for:\n{panel}"
    );
}
