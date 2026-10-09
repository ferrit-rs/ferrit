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

mod common;

use common::TempDir;
use std::fs;
use std::path::Path;
use std::process::Command;
use std::sync::mpsc;
use std::time::Duration;

use ferrit::app::App;
use ferrit::app::events::AppEvent;
use ferrit::config::{Config, ConfigLoad};
use ferrit::git::stats::Window;
use ferrit::interface::state::pane::Pane;
use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};

fn key(c: char) -> KeyEvent {
    KeyEvent::from(KeyCode::Char(c))
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
    assert!(!app.dashboard_is_open());

    app.open_dashboard();

    assert!(app.dashboard_is_open());
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
    assert!(!app.dashboard_is_open());
    assert!(
        !app.is_quitting(),
        "q closes the dashboard, it does not quit"
    );

    app.open_dashboard();
    app.feed_key(KeyEvent::from(KeyCode::Esc));
    assert!(!app.dashboard_is_open());

    app.open_dashboard();
    app.feed_key(key('D'));
    assert!(!app.dashboard_is_open());

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
    assert!(app.dashboard_is_open());
    assert!(app.dashboard().stats().is_none());
    assert!(!app.dashboard().computing());
    assert!(app.dashboard().error().is_none());
    app.close_dashboard();
    assert!(!app.dashboard_is_open());
}

#[test]
fn d_opens_the_dashboard_and_closes_it_again() {
    let tmp = history("dash-d");
    let mut app = App::open(tmp.path()).unwrap();
    app.feed_key(key('D'));
    assert!(app.dashboard_is_open());
    assert_eq!(app.dashboard().stats().unwrap().totals.commits, 3);
    app.feed_key(key('D'));
    assert!(!app.dashboard_is_open());
}

#[test]
fn the_key_that_opens_the_dashboard_can_be_rebound_and_still_closes_it() {
    let tmp = history("dash-rebind");
    let (config, issues) = Config::parse("[keys.global]\ndashboard = \"B\"\n");
    assert!(issues.is_empty(), "{issues:?}");
    let load = ConfigLoad {
        config,
        file: None,
        issues,
    };
    let mut app = App::open_with(tmp.path(), load).unwrap();

    app.feed_key(key('D'));
    assert!(!app.dashboard_is_open(), "D is no longer bound");
    app.feed_key(key('B'));
    assert!(app.dashboard_is_open());
    app.feed_key(key('B'));
    assert!(!app.dashboard_is_open(), "the rebound key closes it");
}

fn mouse(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

#[test]
fn a_click_reaches_no_pane_behind_the_dashboard_and_the_wheel_scrolls_it() {
    let tmp = history("dash-mouse");
    let mut app = App::open(tmp.path()).unwrap();
    // A frame of the panes first, so their click areas exist.
    let mut terminal = ratatui::Terminal::new(ratatui::backend::TestBackend::new(120, 40)).unwrap();
    terminal
        .draw(|f| ferrit::interface::screens::draw(f, &mut app))
        .unwrap();
    app.open_dashboard();
    // Let the drawer slide in, so it has an area a click can be inside of.
    for _ in 0..30 {
        app.advance_clock(Duration::from_millis(16));
        terminal
            .draw(|f| ferrit::interface::screens::draw(f, &mut app))
            .unwrap();
    }
    let selected = app.selected(Pane::Commits);

    // Inside the drawer (112 of the 120 columns, from the right).
    for row in [3, 12, 20, 30] {
        app.feed_mouse(mouse(MouseEventKind::Down(MouseButton::Left), 60, row));
    }
    assert!(app.dashboard_is_open());
    assert_eq!(
        app.selected(Pane::Commits),
        selected,
        "no pane heard a click"
    );

    app.feed_mouse(mouse(MouseEventKind::ScrollDown, 10, 10));
    assert_eq!(app.dashboard().scroll(), 3);
    app.feed_mouse(mouse(MouseEventKind::ScrollUp, 10, 10));
    assert_eq!(app.dashboard().scroll(), 0);
    app.feed_mouse(mouse(MouseEventKind::ScrollUp, 10, 10));
    assert_eq!(app.dashboard().scroll(), 0, "no scrolling above the top");
}
