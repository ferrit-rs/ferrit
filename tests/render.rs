//! Mechanism 1 from `docs/PLAN_SELF_TESTING.md`: render `ui::draw` into a
//! `TestBackend` and assert on frame text. No terminal, no timing.

use std::collections::BTreeSet;

use ferrit::app::{App, Pane};
use ferrit::{mock, ui};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::style::Color;

fn frame(app: &mut App, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|f| ui::draw(f, app)).unwrap();
    terminal.backend().to_string()
}

/// Rows in the left column (x < 40 at width 120) that carry the blue selection
/// bar, i.e. at least one cell with a blue background.
fn selection_bar_rows(app: &mut App) -> BTreeSet<u16> {
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    terminal.draw(|f| ui::draw(f, app)).unwrap();
    let buf = terminal.backend().buffer();
    let mut rows = BTreeSet::new();
    for y in 0..buf.area.height {
        for x in 0..40 {
            if buf[(x, y)].style().bg == Some(Color::Blue) {
                rows.insert(y);
                break;
            }
        }
    }
    rows
}

#[test]
fn renders_every_region() {
    let out = frame(&mut App::mock(), 120, 40);
    for expected in [
        "[1] Status",
        "[2] Files",
        "[3] Local branches",
        "[4] Commits",
        "[5] Stash",
        "command log",
        "Stage:", // keybar label
        "Quit:",  // keybar label
    ] {
        assert!(out.contains(expected), "missing {expected:?}\n{out}");
    }
}

#[test]
fn right_pane_follows_focus() {
    let mut app = App::mock();

    app.focus = Pane::Files;
    assert!(frame(&mut app, 120, 40).contains("diff --git a/src/main.rs"));

    app.focus = Pane::Branches;
    assert!(frame(&mut app, 120, 40).contains("HEAD -> main"));

    app.focus = Pane::Stash;
    assert!(frame(&mut app, 120, 40).contains("(no stash entries)"));
}

#[test]
fn only_the_focused_pane_shows_the_selection_bar() {
    let mut app = App::mock();

    app.focus = Pane::Files;
    let files = selection_bar_rows(&mut app);
    app.focus = Pane::Branches;
    let branches = selection_bar_rows(&mut app);
    app.focus = Pane::Commits;
    let commits = selection_bar_rows(&mut app);

    // Each focused pane paints exactly one blue bar...
    assert_eq!(files.len(), 1, "Files bar rows: {files:?}");
    assert_eq!(branches.len(), 1, "Branches bar rows: {branches:?}");
    assert_eq!(commits.len(), 1, "Commits bar rows: {commits:?}");

    // ...and the bar follows focus instead of stacking across every pane.
    assert!(files.is_disjoint(&branches));
    assert!(branches.is_disjoint(&commits));
    assert!(files.is_disjoint(&commits));
}

#[test]
fn image_selection_takes_over_the_right_pane() {
    let mut app = App::mock();
    let png = mock::mock_files()
        .iter()
        .position(|f| f.path.extension().is_some_and(|e| e == "png"))
        .expect("mock has a .png entry");

    app.select(Pane::Files, png);
    let out = frame(&mut app, 120, 40);
    assert!(out.contains("Preview"), "image preview owns the right pane\n{out}");
    assert!(!out.contains("diff --git"), "the mock diff is gone\n{out}");
}

#[test]
fn help_overlay_toggles() {
    let mut app = App::mock();
    assert!(!frame(&mut app, 120, 40).contains("keybindings"));

    app.show_help = true;
    assert!(frame(&mut app, 120, 40).contains("keybindings"));
}

#[test]
fn survives_extremes_without_panicking() {
    for (w, h) in [(40, 20), (20, 8), (200, 60), (1, 1)] {
        let _ = frame(&mut App::mock(), w, h);
    }
}
