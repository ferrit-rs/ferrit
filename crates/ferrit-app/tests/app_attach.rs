#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "integration test scaffolding: a failed setup is the assertion"
)]
//! `App::attach_repository` and `Events::watch` (`docs/PLAN_16_START_WITHOUT_REPO.md`,
//! W1): an app with no repository becomes one on a folder, keeping what only
//! `main` and `run` had set, and the filesystem watch can be pointed at it late.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc;
use std::time::Duration;

use ferrit_app::ui::App;
use ferrit_app::ui::events::{AppEvent, Events};
use ratatui::crossterm::event::{KeyCode, KeyEvent};

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = fs::canonicalize(std::env::temp_dir())
            .unwrap()
            .join(format!("ferrit-{tag}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn init(&self) {
        let status = Command::new("git")
            .arg("-C")
            .arg(&self.0)
            .args(["init", "-q", "."])
            .status()
            .unwrap();
        assert!(status.success());
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn status_text(app: &App) -> String {
    app.status_lines()
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn an_app_with_no_repository_becomes_one_on_the_folder() {
    let dir = TempDir::new("attach-ok");
    dir.init();
    fs::write(dir.0.join("notes.txt"), "hello").unwrap();
    let mut app = App::mock();
    assert!(!status_text(&app).contains("attach-ok"));

    app.attach_repository(&dir.0).unwrap();
    assert!(
        status_text(&app).contains("attach-ok"),
        "{}",
        status_text(&app)
    );
    assert!(
        app.file_lines()
            .iter()
            .any(|l| l.to_string().contains("notes.txt")),
        "the first snapshot is already in"
    );
}

#[test]
fn the_event_sender_survives_the_rebuild() {
    let dir = TempDir::new("attach-sender");
    dir.init();
    let mut app = App::mock();
    let (tx, rx) = mpsc::channel();
    app.set_event_sender(tx);

    app.attach_repository(&dir.0).unwrap();
    // `r` refreshes in the background, which only works through the sender.
    app.feed_key(KeyEvent::from(KeyCode::Char('r')));
    match rx.recv_timeout(Duration::from_secs(20)) {
        Ok(AppEvent::RefreshDone(_)) => {},
        other => panic!("expected RefreshDone, got {other:?}"),
    }
}

#[test]
fn a_folder_that_is_not_a_repository_changes_nothing() {
    let dir = TempDir::new("attach-fail");
    let mut app = App::mock();
    let before = status_text(&app);
    assert!(app.attach_repository(&dir.0).is_err());
    assert_eq!(status_text(&app), before, "still the same app");
}

#[test]
fn the_watch_can_be_pointed_at_a_worktree_after_the_start() {
    let dir = TempDir::new("attach-watch");
    let mut events = Events::new(None, Duration::from_secs(3600)).unwrap();
    assert!(events.watch_error().is_none());
    events.watch(&dir.0);
    assert!(events.watch_error().is_none());

    fs::write(dir.0.join("a.txt"), "x").unwrap();
    let mut saw_refresh = false;
    for _ in 0..20 {
        match events.next_batch_timeout(Duration::from_millis(500)) {
            Ok(Some(batch)) => {
                if batch.iter().any(|e| matches!(e, AppEvent::Refresh)) {
                    saw_refresh = true;
                    break;
                }
            },
            Ok(None) => {},
            Err(error) => panic!("events ended: {error}"),
        }
    }
    assert!(saw_refresh, "a change in the folder was noticed");
}

#[test]
fn a_watch_on_a_folder_that_is_gone_is_a_note_not_a_failure() {
    let dir = TempDir::new("attach-nowatch");
    let mut events = Events::new(None, Duration::from_secs(3600)).unwrap();
    events.watch(&dir.0.join("not-there"));
    assert!(events.watch_error().is_some(), "polling stays the fallback");
}
