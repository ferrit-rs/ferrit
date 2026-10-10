#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "integration test scaffolding: a failed setup is the assertion"
)]
//! The dashboard as a sheet (`docs/PLAN_19_DASHBOARD_SHEET.md`): the panes stay in
//! sight behind it, a click outside closes it, it excludes the settings sheet,
//! and its worker does not outlive it. The page's own content is
//! `tests/dashboard_screen.rs`.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc;
use std::time::Duration;

use ferrit_app::ui::App;
use ferrit_app::ui::draw as ui;
use ferrit_config::{Config, ConfigLoad};
use ferrit_tui::theme::scheme::Scheme;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;

struct Repo(PathBuf);

impl Repo {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = fs::canonicalize(std::env::temp_dir())
            .unwrap()
            .join(format!("ferrit-{tag}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let git = |args: &[&str]| {
            let out = Command::new("git")
                .arg("-C")
                .arg(&dir)
                .args(args)
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .env("GIT_CONFIG_SYSTEM", "/dev/null")
                .output()
                .unwrap();
            assert!(out.status.success(), "git {args:?}");
        };
        git(&["init", "-q", "-b", "main"]);
        for (file, message) in [("a.txt", "feat: one"), ("b.txt", "fix: two")] {
            fs::write(dir.join(file), "x\n").unwrap();
            git(&["add", "-A"]);
            git(&[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.com",
                "commit",
                "-q",
                "-m",
                message,
            ]);
        }
        Self(dir)
    }

    fn app(&self, toml: &str) -> App {
        let (config, issues) = Config::parse(toml);
        App::open_with(
            &self.0,
            ConfigLoad {
                config,
                file: None,
                issues,
            },
        )
        .unwrap()
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
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

fn press(app: &mut App, c: char) {
    app.feed_key(KeyEvent::from(KeyCode::Char(c)));
}

/// Frames while the drawer slides, then the last one as text.
fn settle(app: &mut App, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    for _ in 0..30 {
        app.advance_clock(Duration::from_millis(16));
        terminal.draw(|f| ui::draw(f, app)).unwrap();
    }
    terminal.backend().to_string()
}

#[test]
fn the_panes_stay_in_sight_behind_the_sheet_and_are_dimmed_by_the_theme() {
    let repo = Repo::new("dsheet-dim");
    for (theme, scheme) in [("dark", Scheme::DARK), ("light", Scheme::LIGHT)] {
        let mut app = repo.app(&format!("[theme]\nbase = \"{theme}\"\n"));
        app.open_dashboard();
        let text = settle(&mut app, 140, 40);
        assert!(
            text.contains("[1] Status") && text.contains("Stash"),
            "{theme}: the panes are on screen\n{text}"
        );
        assert!(text.contains("Dashboard"), "{theme}");

        let mut terminal = Terminal::new(TestBackend::new(140, 40)).unwrap();
        terminal.draw(|f| ui::draw_painted(f, &mut app)).unwrap();
        let buf = terminal.backend().buffer().clone();
        // Left of the drawer: the panes, dimmed by the theme's own layer.
        assert_eq!(buf[(2, 20)].bg, scheme.fill[0], "{theme}: dimmed");
        // Inside the drawer: the theme's background.
        assert_eq!(buf[(70, 38)].bg, scheme.background, "{theme}: the page");
    }
}

#[test]
fn a_click_outside_slides_it_out_and_a_click_inside_leaves_it_up() {
    let repo = Repo::new("dsheet-click");
    let mut app = repo.app("");
    app.open_dashboard();
    settle(&mut app, 140, 40);

    click(&mut app, 80, 20); // inside the drawer
    assert!(app.dashboard_is_open());

    click(&mut app, 3, 20); // over the dimmed panes
    assert!(!app.dashboard_is_open(), "closing at once");
    assert!(app.sheet_is_open(), "but still sliding out");
    settle(&mut app, 140, 40);
    assert!(!app.sheet_is_open(), "and gone when the slide is over");
}

#[test]
fn a_click_on_the_panes_while_it_is_up_reaches_no_pane() {
    let repo = Repo::new("dsheet-nopane");
    let mut app = repo.app("");
    app.open_dashboard();
    settle(&mut app, 140, 40);
    let before = app.selected(ferrit_app::ui::components::panes::nav::Pane::Commits);
    click(&mut app, 3, 20);
    settle(&mut app, 140, 40);
    assert_eq!(
        app.selected(ferrit_app::ui::components::panes::nav::Pane::Commits),
        before
    );
    assert_eq!(
        app.nav.focus,
        ferrit_app::ui::components::panes::nav::Pane::default(),
        "no focus moved"
    );
}

#[test]
fn the_settings_sheet_and_the_dashboard_never_show_together() {
    let repo = Repo::new("dsheet-exclude");
    let mut app = repo.app("");
    let author = Rect::new(0, 0, 6, 1);

    // The settings are up: D is the settings sheet's key, not the dashboard's.
    app.set_author_click_area(author);
    click(&mut app, 1, 0);
    settle(&mut app, 140, 40);
    press(&mut app, 'D');
    assert!(!app.dashboard_is_open());
    assert!(
        settle(&mut app, 140, 40).contains("Wheel step"),
        "still the settings"
    );
    app.feed_key(KeyEvent::from(KeyCode::Esc));
    settle(&mut app, 140, 40);
    assert!(!app.sheet_is_open());

    // The dashboard is up: a click on the author's name is outside it, so it
    // closes the dashboard, and only the next click opens the settings.
    app.open_dashboard();
    settle(&mut app, 140, 40);
    app.set_author_click_area(author);
    click(&mut app, 1, 0);
    settle(&mut app, 140, 40);
    assert!(!app.sheet_is_open(), "the dashboard closed, nothing opened");
    app.set_author_click_area(author);
    click(&mut app, 1, 0);
    assert!(settle(&mut app, 140, 40).contains("Wheel step"));
}

#[test]
fn closing_it_cancels_the_worker_and_leaves_nothing_running() {
    let repo = Repo::new("dsheet-worker");
    let mut app = repo.app("");
    let (tx, rx) = mpsc::channel();
    app.set_event_sender(tx);
    app.open_dashboard();
    assert!(!app.is_idle(), "the statistics are being read");

    app.close_dashboard();
    while !app.is_idle() {
        let event = rx
            .recv_timeout(Duration::from_secs(20))
            .expect("the worker ends");
        app.deliver_event(event);
    }
    assert!(!app.dashboard_is_open());
    settle(&mut app, 140, 40);
    assert!(!app.sheet_is_open());
}

#[test]
fn the_key_bar_is_the_dashboards_while_it_is_up_and_the_panes_after() {
    let repo = Repo::new("dsheet-keybar");
    let mut app = repo.app("");
    let before = settle(&mut app, 140, 40);
    assert!(before.lines().last().unwrap().contains("Commit: c"));

    app.open_dashboard();
    let up = settle(&mut app, 140, 40);
    let bar = up.lines().last().unwrap();
    assert!(
        bar.contains("Back: esc") && bar.contains("Window: t") && !bar.contains("Commit: c"),
        "{bar:?}"
    );

    press(&mut app, 'q');
    let after = settle(&mut app, 140, 40);
    assert!(after.lines().last().unwrap().contains("Commit: c"));
    assert!(!after.lines().any(|line| line.contains("Back: esc")));
}

#[test]
fn clicking_the_dashboard_info_trigger_opens_the_dashboard() {
    let repo = Repo::new("dsheet-info-trigger");
    let mut app = repo.app("");
    app.set_dashboard_click_area(Rect::new(10, 0, 12, 1));

    click(&mut app, 12, 0);

    assert!(app.dashboard_is_open());
}

#[test]
fn a_key_pressed_while_it_slides_out_does_not_reopen_or_reach_a_pane() {
    let repo = Repo::new("dsheet-slide-key");
    let mut app = repo.app("");
    app.open_dashboard();
    settle(&mut app, 140, 40);
    press(&mut app, 'q');
    press(&mut app, 'j'); // while sliding out: still the sheet's keys
    assert!(!app.dashboard_is_open());
    settle(&mut app, 140, 40);
    assert!(!app.sheet_is_open());
}

#[test]
fn small_terminals_do_not_panic_with_the_dashboard_up() {
    let repo = Repo::new("dsheet-tiny");
    let mut app = repo.app("");
    app.open_dashboard();
    for (w, h) in [
        (1, 1),
        (10, 3),
        (30, 5),
        (50, 8),
        (80, 12),
        (40, 100),
        (200, 4),
    ] {
        settle(&mut app, w, h);
    }
}

#[test]
fn the_sheet_has_one_frame_and_one_title_not_a_frame_inside_a_frame() {
    let repo = Repo::new("dsheet-oneframe");
    let mut app = repo.app("");
    app.open_dashboard();
    let text = settle(&mut app, 140, 40);
    // The drawer is 112 cells wide at the right: columns 28 and on.
    let drawer: Vec<String> = text.lines().map(|l| l.chars().skip(28).collect()).collect();
    let corners = drawer.iter().map(|l| l.matches('╭').count()).sum::<usize>();
    assert_eq!(
        corners, 1,
        "the drawer's own top-left corner and no other\n{text}"
    );
    let titles = drawer
        .iter()
        .map(|l| l.matches("Dashboard").count())
        .sum::<usize>();
    assert_eq!(
        titles, 1,
        "the drawer's title, not repeated by the page\n{text}"
    );
    assert!(
        drawer
            .iter()
            .any(|l| l.contains("· main") || l.contains("· ")),
        "the page's header is there"
    );
}
