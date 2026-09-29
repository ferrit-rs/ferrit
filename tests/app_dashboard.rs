#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::pathbuf_init_then_push,
    reason = "integration test scaffolding: a failed setup is the assertion"
)]
//! The dashboard's state, its worker and its keys (`docs/PLAN_13_DASHBOARD.md`,
//! D2): no drawing yet, so these tests read `App::dashboard()` and drive the
//! events by hand, the way `tests/app_remote.rs` does for the remote operations.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use std::time::Duration;

use ferrit::app::events::AppEvent;
use ferrit::app::{App, FullScreen, Pane};
use ferrit::domain::git::stats::Window;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn key(c: char) -> KeyEvent {
    KeyEvent::from(KeyCode::Char(c))
}

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let mut path = std::env::temp_dir();
        path.push(format!("ferrit-{tag}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn commit(dir: &Path, file: &str, body: &str, message: &str) {
    fs::write(dir.join(file), body).unwrap();
    git(dir, &["add", "-A"]);
    git(
        dir,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "commit",
            "-q",
            "-m",
            message,
        ],
    );
}

/// Three commits with Conventional Commits subjects, on `main`.
fn history(tag: &str) -> TempDir {
    let tmp = TempDir::new(tag);
    git(tmp.path(), &["init", "-q", "-b", "main"]);
    commit(tmp.path(), "a.txt", "a\n", "feat: one");
    commit(tmp.path(), "a.txt", "a\nb\n", "fix: two");
    commit(tmp.path(), "b.txt", "b\n", "docs: three");
    tmp
}

/// Hand every event the worker sent to the app until nothing is running.
fn drain(app: &mut App, rx: &mpsc::Receiver<AppEvent>) {
    while !app.is_idle() {
        let event = rx
            .recv_timeout(Duration::from_secs(20))
            .expect("the worker answers");
        app.deliver_event(event);
    }
}

#[test]
fn without_an_event_loop_the_statistics_are_read_at_once() {
    let tmp = history("dash-sync");
    let mut app = App::open(tmp.path()).unwrap();
    assert_eq!(app.full_screen(), FullScreen::None);

    app.open_dashboard();

    assert_eq!(app.full_screen(), FullScreen::Dashboard);
    let dashboard = app.dashboard();
    let stats = dashboard.stats().expect("computed in the call");
    assert_eq!(stats.totals.commits, 3);
    assert!(stats.hot_files.is_some(), "the full pass ran too");
    assert!(!dashboard.churn_pending());
    assert!(!dashboard.computing());
    assert!(app.is_idle());
}

#[test]
fn with_an_event_loop_the_quick_pass_paints_before_the_full_one() {
    let tmp = history("dash-async");
    let mut app = App::open(tmp.path()).unwrap();
    let (tx, rx) = mpsc::channel();
    app.set_event_sender(tx);

    app.open_dashboard();
    assert!(app.dashboard().computing(), "nothing has arrived yet");
    assert!(!app.is_idle(), "a worker is running");

    let quick = rx.recv_timeout(Duration::from_secs(20)).unwrap();
    assert!(matches!(quick, AppEvent::StatsDone(_)));
    app.deliver_event(quick);
    let stats = app.dashboard().stats().expect("the quick pass is in");
    assert_eq!(stats.totals.commits, 3);
    assert!(stats.hot_files.is_none(), "the numstat is still coming");
    assert!(app.dashboard().churn_pending());

    drain(&mut app, &rx);
    let stats = app.dashboard().stats().unwrap();
    assert!(stats.hot_files.is_some());
    assert!(!app.dashboard().churn_pending());
}

#[test]
fn a_window_changed_meanwhile_drops_the_old_result() {
    let tmp = history("dash-stale");
    let mut app = App::open(tmp.path()).unwrap();
    let (tx, rx) = mpsc::channel();
    app.set_event_sender(tx);

    app.open_dashboard();
    assert_eq!(app.dashboard().window(), Window::Days90);
    app.feed_key(key('t'));
    assert_eq!(app.dashboard().window(), Window::Year);

    drain(&mut app, &rx);
    // Anything the first request sent was of an older generation and was ignored:
    // what is shown is the window asked for last.
    let stats = app.dashboard().stats().unwrap();
    assert_eq!(stats.window, Window::Year);
    assert!(stats.hot_files.is_some());
}

#[test]
fn keys_stay_in_the_dashboard_and_q_or_esc_leave_it() {
    let tmp = history("dash-keys");
    let mut app = App::open(tmp.path()).unwrap();
    app.open_dashboard();
    let before = app.selected(Pane::Files);

    app.feed_key(key('j'));
    assert_eq!(
        app.selected(Pane::Files),
        before,
        "j did not reach the panes"
    );
    app.feed_key(key('q'));
    assert_eq!(app.full_screen(), FullScreen::None);
    assert!(
        !app.is_quitting(),
        "q closes the dashboard, it does not quit"
    );

    app.open_dashboard();
    app.feed_key(KeyEvent::from(KeyCode::Esc));
    assert_eq!(app.full_screen(), FullScreen::None);

    app.open_dashboard();
    app.feed_key(key('D'));
    assert_eq!(app.full_screen(), FullScreen::None);

    app.open_dashboard();
    app.feed_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
    assert!(app.is_quitting(), "Ctrl-c still quits from the dashboard");
}

#[test]
fn t_cycles_the_windows_n_swaps_the_figures_and_j_k_scroll() {
    let tmp = history("dash-cycle");
    let mut app = App::open(tmp.path()).unwrap();
    app.open_dashboard();

    let mut seen = vec![app.dashboard().window()];
    for _ in 0..5 {
        app.feed_key(key('t'));
        seen.push(app.dashboard().window());
    }
    assert_eq!(
        seen,
        [
            Window::Days90,
            Window::Year,
            Window::All,
            Window::Days7,
            Window::Days30,
            Window::Days90
        ]
    );
    app.feed_key(key('T'));
    assert_eq!(app.dashboard().window(), Window::Days30);
    for _ in 0..4 {
        app.feed_key(key('T'));
    }
    assert_eq!(
        app.dashboard().window(),
        Window::Days90,
        "T goes the other way"
    );

    assert!(!app.dashboard().show_counts());
    app.feed_key(key('n'));
    assert!(app.dashboard().show_counts());
    app.feed_key(key('n'));
    assert!(!app.dashboard().show_counts());

    app.feed_key(key('j'));
    app.feed_key(key('j'));
    assert_eq!(app.dashboard().scroll(), 2);
    app.feed_key(key('k'));
    assert_eq!(app.dashboard().scroll(), 1);
    app.feed_key(KeyEvent::from(KeyCode::PageDown));
    assert_eq!(app.dashboard().scroll(), 11);
    app.feed_key(KeyEvent::from(KeyCode::Home));
    assert_eq!(app.dashboard().scroll(), 0);
    app.feed_key(key('k'));
    assert_eq!(app.dashboard().scroll(), 0, "no scrolling above the top");
    app.feed_key(key('t'));
    app.feed_key(KeyEvent::from(KeyCode::End));
    app.feed_key(key('t'));
    assert_eq!(
        app.dashboard().scroll(),
        0,
        "a new window starts at the top"
    );
}

#[test]
fn reopening_uses_the_cache_until_a_ref_moves() {
    let tmp = history("dash-cache");
    let mut app = App::open(tmp.path()).unwrap();
    app.open_dashboard();
    assert_eq!(app.dashboard().stats().unwrap().totals.commits, 3);
    app.close_dashboard();

    // A commit the app has not seen yet: same refs as far as it knows, so the
    // cached numbers are shown as they were.
    commit(tmp.path(), "c.txt", "c\n", "chore: four");
    app.open_dashboard();
    assert_eq!(
        app.dashboard().stats().unwrap().totals.commits,
        3,
        "the cache is used while nothing the app can see changed"
    );
    app.close_dashboard();

    // Once the panes refresh, the fingerprint moves and the cache is dropped.
    app.refresh();
    app.open_dashboard();
    assert_eq!(app.dashboard().stats().unwrap().totals.commits, 4);
}

#[test]
fn r_recomputes_even_when_the_cache_is_fresh() {
    let tmp = history("dash-force");
    let mut app = App::open(tmp.path()).unwrap();
    app.open_dashboard();
    commit(tmp.path(), "c.txt", "c\n", "chore: four");
    app.feed_key(key('r'));
    assert_eq!(app.dashboard().stats().unwrap().totals.commits, 4);
}

#[test]
fn the_mock_app_has_no_repository_and_no_statistics() {
    let mut app = App::mock();
    app.open_dashboard();
    assert_eq!(app.full_screen(), FullScreen::Dashboard);
    assert!(app.dashboard().stats().is_none());
    assert!(!app.dashboard().computing());
    assert!(app.dashboard().error().is_none());
    app.close_dashboard();
    assert_eq!(app.full_screen(), FullScreen::None);
}
