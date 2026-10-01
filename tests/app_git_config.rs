#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::pathbuf_init_then_push,
    reason = "integration test scaffolding: a failed setup is the assertion"
)]
//! The git config screen's state and keys (`docs/PLAN_14_GIT_CONFIG.md`, G3):
//! no drawing yet, so these tests read `App::git_config()`. The global file is
//! a throwaway one (`App::isolate_git_config`), never the user's.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use ferrit::app::{App, FullScreen};
use ferrit::domain::git::config::{Scope, WriteScope};
use ratatui::crossterm::event::{KeyCode, KeyEvent};

fn key(c: char) -> KeyEvent {
    KeyEvent::from(KeyCode::Char(c))
}

fn press(app: &mut App, code: KeyCode) {
    app.feed_key(KeyEvent::from(code));
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

    /// An app on the fixture whose config calls stay in its sandbox.
    fn app(&self) -> App {
        let mut app = App::open(&self.dir).unwrap();
        app.isolate_git_config(&self.global);
        app
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn keys(app: &App) -> Vec<String> {
    app.git_config()
        .rows
        .iter()
        .map(|r| r.entry.key.clone())
        .collect()
}

#[test]
fn opening_lists_every_value_ordered_by_key_and_marks_the_winner() {
    let fx = Fixture::new("gc-open");
    let mut app = fx.app();
    app.open_git_config();

    assert_eq!(app.full_screen(), FullScreen::GitConfig);
    let screen = app.git_config();
    assert_eq!(screen.scope, WriteScope::Local);
    assert!(screen.total() >= 4);
    let mut sorted = keys(&app);
    sorted.sort();
    assert_eq!(keys(&app), sorted);

    let rebase: Vec<_> = screen
        .rows
        .iter()
        .filter(|r| r.entry.key == "pull.rebase")
        .collect();
    assert_eq!(rebase.len(), 2);
    assert_eq!(
        (rebase[0].entry.scope, rebase[0].shadowed, rebase[0].winner),
        (Scope::Global, true, false)
    );
    assert_eq!(
        (rebase[1].entry.scope, rebase[1].shadowed, rebase[1].winner),
        (Scope::Local, false, true)
    );
    let editor = screen
        .rows
        .iter()
        .find(|r| r.entry.key == "core.editor")
        .unwrap();
    assert!(!editor.shadowed && !editor.winner);
}

#[test]
fn navigation_filter_scope_and_close_keys() {
    let fx = Fixture::new("gc-keys");
    let mut app = fx.app();
    app.open_git_config();
    assert_eq!(app.git_config().selected, 0);

    app.feed_key(key('j'));
    assert_eq!(app.git_config().selected, 1);
    app.feed_key(key('k'));
    app.feed_key(key('k'));
    assert_eq!(app.git_config().selected, 0);
    press(&mut app, KeyCode::End);
    assert_eq!(app.git_config().selected, app.git_config().rows.len() - 1);
    press(&mut app, KeyCode::Home);
    assert_eq!(app.git_config().selected, 0);

    app.feed_key(key('s'));
    assert_eq!(app.git_config().scope, WriteScope::Global);
    app.feed_key(key('s'));
    assert_eq!(app.git_config().scope, WriteScope::Local);

    // `/` filters by key; the letters go to the filter, not to the bindings.
    app.feed_key(key('/'));
    type_text(&mut app, "pull");
    assert_eq!(keys(&app), ["pull.rebase", "pull.rebase"]);
    press(&mut app, KeyCode::Enter);
    assert!(!app.git_config().filtering);
    assert_eq!(app.git_config().filter, "pull");
    app.feed_key(key('/'));
    press(&mut app, KeyCode::Backspace);
    type_text(&mut app, "L");
    assert_eq!(app.git_config().filter, "pulL");
    press(&mut app, KeyCode::Esc);
    assert_eq!(app.git_config().filter, "");
    assert!(app.git_config().rows.len() > 2);

    app.feed_key(key('q'));
    assert_eq!(app.full_screen(), FullScreen::None);
}

#[test]
fn a_secret_value_is_never_matched_by_the_filter() {
    let fx = Fixture::new("gc-secret");
    let mut app = fx.app();
    app.open_git_config();
    app.feed_key(key('/'));
    type_text(&mut app, "ghp_secret");
    assert!(app.git_config().rows.is_empty());
}

#[test]
fn a_reread_keeps_the_selection_on_the_same_value() {
    let fx = Fixture::new("gc-reread");
    let mut app = fx.app();
    app.open_git_config();
    let at = app
        .git_config()
        .rows
        .iter()
        .position(|r| r.entry.key == "core.editor")
        .unwrap();
    for _ in 0..at {
        app.feed_key(key('j'));
    }
    // Something else changes the file: a key that sorts before the selection.
    fx.git(&["config", "--local", "a.first", "x"]);
    app.feed_key(key('r'));
    let row = app.git_config().selected_row().unwrap();
    assert_eq!(row.entry.key, "core.editor");
    assert_eq!(app.git_config().selected, at + 1);
}
