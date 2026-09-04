//! Mechanism 1 from `docs/PLAN_SELF_TESTING.md`: render `ui::draw` into a
//! `TestBackend` and assert on frame text. No terminal, no timing.

use ferrit::app::{App, Pane};
use ferrit::{mock, ui};
use ratatui::Terminal;
use ratatui::backend::TestBackend;

fn frame(app: &App, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|f| ui::draw(f, app)).unwrap();
    terminal.backend().to_string()
}

#[test]
fn renders_every_region() {
    let out = frame(&App::mock(), 120, 40);
    for expected in [
        "1 Status",
        "2 Files",
        "3 Local Branches",
        "4 Commits",
        "5 Stash",
        "command log",
        "stage",  // keybar label
        "quit",   // keybar label
    ] {
        assert!(out.contains(expected), "missing {expected:?}\n{out}");
    }
}

#[test]
fn right_pane_follows_focus() {
    let mut app = App::mock();

    app.focus = Pane::Files;
    assert!(frame(&app, 120, 40).contains("diff --git a/src/main.rs"));

    app.focus = Pane::Branches;
    assert!(frame(&app, 120, 40).contains("HEAD -> main"));

    app.focus = Pane::Stash;
    assert!(frame(&app, 120, 40).contains("(no stash entries)"));
}

#[test]
fn image_selection_takes_over_the_right_pane() {
    let mut app = App::mock();
    let png = mock::mock_files()
        .iter()
        .position(|f| f.path.extension().is_some_and(|e| e == "png"))
        .expect("mock has a .png entry");

    app.select(Pane::Files, png);
    let out = frame(&app, 120, 40);
    assert!(out.contains("Preview"), "image preview owns the right pane\n{out}");
    assert!(!out.contains("diff --git"), "the mock diff is gone\n{out}");
}

#[test]
fn help_overlay_toggles() {
    let mut app = App::mock();
    assert!(!frame(&app, 120, 40).contains("keybindings"));

    app.show_help = true;
    assert!(frame(&app, 120, 40).contains("keybindings"));
}

#[test]
fn survives_extremes_without_panicking() {
    for (w, h) in [(40, 20), (20, 8), (200, 60), (1, 1)] {
        let _ = frame(&App::mock(), w, h);
    }
}
