#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "integration test scaffolding: a failed setup is the assertion"
)]
//! The error toast in the bottom-right corner: it closes by itself, on `Esc`
//! and on its `x`; an error is shown once, not again each time a refresh sees
//! it; and a long message is not cut off.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc;
use std::time::Duration;

use ferrit::app::events::{AppEvent, RemoteOp};
use ferrit::app::{App, screens as ui};
use ferrit::components::ui::toast::Toast;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent};

const FRAME: Duration = Duration::from_millis(16);

/// Tick `toast` in frames until `done`, at most `limit`.
fn tick_until(toast: &mut Toast, limit: Duration, done: impl Fn(&Toast) -> bool) -> Duration {
    let mut spent = Duration::ZERO;
    while spent < limit && !done(toast) {
        toast.tick(FRAME);
        spent += FRAME;
    }
    spent
}

#[test]
fn a_toast_closes_by_itself_after_a_while_not_before() {
    let mut toast = Toast::error(std::io::Error::other("boom"));
    // Slides in, then stays.
    tick_until(&mut toast, Duration::from_secs(1), |t| !t.is_animating());
    assert!(!toast.is_closing());
    let stayed = tick_until(&mut toast, Duration::from_secs(7), Toast::is_closing);
    assert!(!toast.is_closing(), "still up after {stayed:?}");
    // It closes within a little more, and then is gone.
    tick_until(&mut toast, Duration::from_secs(2), Toast::is_closing);
    assert!(toast.is_closing(), "closing on its own after about 8 s");
    tick_until(&mut toast, Duration::from_secs(1), Toast::is_closed);
    assert!(toast.is_closed());
}

#[test]
fn dismiss_closes_it_now_and_twice_is_harmless() {
    let mut toast = Toast::error(std::io::Error::other("boom"));
    tick_until(&mut toast, Duration::from_secs(1), |t| !t.is_animating());
    toast.dismiss();
    toast.dismiss();
    assert!(toast.is_closing());
    tick_until(&mut toast, Duration::from_secs(1), Toast::is_closed);
    assert!(toast.is_closed());
}

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
        Self(path)
    }

    fn app(&self) -> App {
        App::open(&self.0).unwrap()
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn frame(app: &mut App) -> String {
    let mut terminal = Terminal::new(TestBackend::new(140, 40)).unwrap();
    terminal.draw(|f| ui::draw(f, app)).unwrap();
    terminal.backend().to_string()
}

/// Whether the toast's own frame is on screen (its title is `Error`).
fn toast_up(app: &mut App) -> bool {
    frame(app).contains(" Error ")
}

fn fail_a_fetch(app: &mut App, message: &str) {
    app.on_remote_done(RemoteOp::Fetch, Err(message.to_owned()));
}

/// Let the slide-out finish.
fn settle(app: &mut App) {
    for _ in 0..40 {
        app.advance_clock(FRAME);
    }
}

#[test]
fn escape_closes_the_toast_and_the_message_stays_in_the_status_pane() {
    let repo = Repo::new("toast-esc");
    let mut app = repo.app();
    fail_a_fetch(&mut app, "boom");
    settle(&mut app);
    assert!(toast_up(&mut app));

    app.feed_key(KeyEvent::from(KeyCode::Esc));
    settle(&mut app);
    assert!(!toast_up(&mut app), "the toast is gone");
    assert!(
        frame(&mut app).contains("boom"),
        "the Status pane still says it"
    );
}

#[test]
fn it_goes_away_on_its_own_without_a_key() {
    let repo = Repo::new("toast-timeout");
    let mut app = repo.app();
    fail_a_fetch(&mut app, "boom");
    settle(&mut app);
    for _ in 0..40 {
        app.advance_clock(Duration::from_millis(250));
    }
    assert!(!toast_up(&mut app), "ten seconds later it has timed out");
}

#[test]
fn escape_does_not_steal_the_key_from_a_popup() {
    let repo = Repo::new("toast-popup");
    let mut app = repo.app();
    fail_a_fetch(&mut app, "boom");
    settle(&mut app);
    app.feed_key(KeyEvent::from(KeyCode::Char('@')));
    assert!(
        frame(&mut app).contains("command log"),
        "the command log popup is up"
    );

    app.feed_key(KeyEvent::from(KeyCode::Esc));
    assert!(
        !frame(&mut app).contains("command log"),
        "Esc closed the popup"
    );
    assert!(toast_up(&mut app), "and left the toast alone");
    app.feed_key(KeyEvent::from(KeyCode::Esc));
    settle(&mut app);
    assert!(!toast_up(&mut app), "the next Esc closes the toast");
}

#[test]
fn a_failed_remote_operation_is_toasted_once_not_again_when_the_refresh_ends() {
    let repo = Repo::new("toast-once");
    let mut app = repo.app();
    let (tx, rx) = mpsc::channel();
    app.set_event_sender(tx);
    fail_a_fetch(&mut app, "boom");
    settle(&mut app);
    app.feed_key(KeyEvent::from(KeyCode::Esc));
    settle(&mut app);
    assert!(!toast_up(&mut app));

    // The refresh that the failure asked for ends now: it used to raise a
    // second toast, "repository refresh failed: ...", right after the first
    // was closed.
    while let Ok(event) = rx.recv_timeout(Duration::from_secs(3)) {
        let done = matches!(event, AppEvent::RefreshDone(_));
        app.deliver_event(event);
        if done {
            break;
        }
    }
    settle(&mut app);
    assert!(!toast_up(&mut app), "no second toast");
    let shown = frame(&mut app);
    assert!(
        shown.contains("background operation failed: boom"),
        "{shown}"
    );
    assert!(!shown.contains("repository refresh failed"), "{shown}");
}

#[test]
fn a_refresh_that_keeps_failing_is_toasted_once_and_again_only_after_it_worked() {
    let repo = Repo::new("toast-repeat");
    let mut app = repo.app();
    let git = repo.0.join(".git");
    let hidden = repo.0.join(".git-hidden");

    fs::rename(&git, &hidden).unwrap();
    app.refresh();
    settle(&mut app);
    assert!(toast_up(&mut app), "the first failure is shown");
    app.feed_key(KeyEvent::from(KeyCode::Esc));
    settle(&mut app);

    app.refresh();
    app.refresh();
    settle(&mut app);
    assert!(!toast_up(&mut app), "the same failure is not shown again");

    fs::rename(&hidden, &git).unwrap();
    app.refresh();
    fs::rename(&git, &hidden).unwrap();
    app.refresh();
    settle(&mut app);
    assert!(
        toast_up(&mut app),
        "after a good refresh, a new failure is shown again"
    );
    fs::rename(&hidden, &git).unwrap();
}

#[test]
fn a_long_error_is_shown_whole() {
    let repo = Repo::new("toast-long");
    let mut app = repo.app();
    let words: Vec<String> = (1..=24).map(|i| format!("word{i:02}")).collect();
    fail_a_fetch(&mut app, &words.join(" "));
    settle(&mut app);
    let shown = frame(&mut app);
    assert!(shown.contains("word24"), "the end of the message: {shown}");
}
