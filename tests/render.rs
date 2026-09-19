#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "integration test: a failed setup or a bad slice is the assertion"
)]
//! Mechanism 1 from `docs/PLAN_SELF_TESTING.md`: render `ui::draw` into a
//! `TestBackend` and assert on frame text. No terminal, no timing.

use std::collections::BTreeSet;
use std::path::Path;

use ferrit::app::{App, Pane};
use ferrit::{mock, ui};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
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

/// `docs/PLAN_8_BRANCHES.md`: the keybar is context-sensitive on the
/// Branches pane — its own keys, not the Files-oriented default (`d` there
/// deletes a branch, not a worktree change), and back to the default while
/// drilled into a branch's own commit log (`Esc`/`j`/`k` apply there, not
/// checkout/new/delete).
#[test]
fn keybar_swaps_for_the_branches_pane() {
    let mut app = App::mock();
    assert!(
        frame(&mut app, 120, 40).contains("Stage:"),
        "default by default"
    );

    app.select(Pane::Branches, 0);
    let branches_out = frame(&mut app, 120, 40);
    assert!(branches_out.contains("Checkout:"), "{branches_out}");
    assert!(branches_out.contains("Merge:"), "{branches_out}");
    assert!(!branches_out.contains("Stage:"), "{branches_out}");

    app.focus = Pane::Files;
    assert!(
        frame(&mut app, 120, 40).contains("Stage:"),
        "back to default"
    );
}

#[test]
fn right_pane_follows_focus() {
    let mut app = App::mock();

    app.focus = Pane::Files;
    assert!(frame(&mut app, 120, 40).contains("diff --git a/src/main.rs"));

    app.focus = Pane::Branches;
    assert!(
        !frame(&mut app, 120, 40).contains("diff --git"),
        "Branches has no mock body since G7: Enter drills into a real branch log instead"
    );

    app.focus = Pane::Stash;
    assert!(frame(&mut app, 120, 40).contains("(no stash entries)"));
}

/// Status's welcome screen (`docs/PLAN_1_LAYOUT.md`, "Welcome screen"): no
/// repo data, so `App::mock()` shows the same thing a real repo would.
/// lazygit grows its own banner as the terminal grows rather than showing
/// one fixed size, so ferrit picks the biggest of three wordmark tiers that
/// fits the right pane's `(width, height)` instead of one fixed logo.
/// Markers below are each unique to one tier (checked against the other
/// two's source art): small alone uses `▐`, only medium has a 4-wide `▄`
/// run, only large uses `▒`.
#[test]
fn status_shows_the_welcome_screen() {
    let mut app = App::mock();
    app.focus = Pane::Status;
    let small = "\u{2590}\u{2588}\u{2588}\u{2588}"; // ▐███
    let medium = "\u{2584}\u{2584}\u{2584}\u{2584}"; // ▄▄▄▄
    let large = "\u{2592}\u{2588}\u{2588}\u{2588}\u{2588}"; // ▒████

    // side = (width / 3).max(24); right pane height = total height - 5
    // (command log + keybar). Large needs (70, 24), medium (70, 16), small
    // (40, 16); each case below clears exactly one tier's bar.
    let huge = frame(&mut app, 160, 50); // right (107, 45)
    assert!(huge.contains(large), "large tier\n{huge}");
    assert!(huge.contains(env!("CARGO_PKG_VERSION")));
    assert!(huge.contains("Press ? for keybindings"));

    let wide = frame(&mut app, 110, 25); // right (74, 20): too short for large
    assert!(
        !wide.contains(large),
        "too short for the large tier\n{wide}"
    );
    assert!(wide.contains(medium), "medium tier\n{wide}");

    let modest = frame(&mut app, 80, 25); // right (54, 20): too narrow for medium
    assert!(
        !modest.contains(medium),
        "too narrow for the medium tier\n{modest}"
    );
    assert!(modest.contains(small), "small tier\n{modest}");

    // Right pane width = area.width - side: at 60 that's 60 - 24 = 36,
    // under every tier's minimum width (40).
    let narrow = frame(&mut app, 60, 30);
    assert!(
        !narrow.contains(small),
        "too narrow for any wordmark tier, falls back to a plain label\n{narrow}"
    );
    assert!(narrow.contains("ferrit"));
    assert!(narrow.contains(env!("CARGO_PKG_VERSION")));
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

/// Rows in the left column (x < 40 at width 120) whose border is drawn in
/// the focused colour, i.e. the bordered rect of whichever pane has focus.
fn focused_border_rows(app: &mut App) -> BTreeSet<u16> {
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    terminal.draw(|f| ui::draw(f, app)).unwrap();
    let buf = terminal.backend().buffer();
    let mut rows = BTreeSet::new();
    for y in 0..buf.area.height {
        if buf[(0, y)].style().fg == Some(Color::Green) {
            rows.insert(y);
        }
    }
    rows
}

#[test]
fn click_moves_focus_and_selection_on_screen() {
    let mut app = App::mock();
    // A first frame lays out the panes, populating `left_areas` /
    // `list_offset` the way a real draw does before any click lands.
    let out = frame(&mut app, 120, 40);
    assert!(
        focused_border_rows(&mut app).iter().all(|&y| y < 4),
        "Status starts out focused"
    );

    let hash = mock::mock_commits()
        .get(1)
        .expect("mock has at least two commits")
        .short_hash
        .clone();
    let row = u16::try_from(
        out.lines()
            .position(|l| l.contains(hash.as_str()))
            .unwrap_or_else(|| panic!("commit {hash} not found in the rendered frame\n{out}")),
    )
    .unwrap();

    app.feed_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 5,
        row,
        modifiers: KeyModifiers::NONE,
    });

    assert_eq!(app.focus, Pane::Commits, "the click moved focus to Commits");
    assert_eq!(
        app.selected(Pane::Commits),
        1,
        "the click selected the clicked commit"
    );

    // Focus moved to Commits, so the accordion now gives it most of the
    // column: the pane reflows and the clicked commit can land on a
    // different row than before, so re-find it rather than reusing `row`.
    let out = frame(&mut app, 120, 40);
    let new_row = u16::try_from(
        out.lines()
            .position(|l| l.contains(hash.as_str()))
            .unwrap_or_else(|| panic!("commit {hash} not found after the click\n{out}")),
    )
    .unwrap();
    assert_eq!(
        selection_bar_rows(&mut app),
        BTreeSet::from([new_row]),
        "the highlight sits on the clicked commit's row"
    );
    assert!(
        focused_border_rows(&mut app).contains(&new_row),
        "the focused border now covers the clicked row"
    );
}

#[test]
fn click_on_the_command_log_or_keybar_is_a_no_op() {
    let mut app = App::mock();
    frame(&mut app, 120, 40); // a real draw, so left_areas / list_offset are set

    // src/ui.rs `draw` reserves the bottom of the screen for the command
    // log (4 rows) then the keybar (1 row): at height 40 that is rows
    // 35..39 and row 39. Neither is a left pane's rect.
    for row in [36, 39] {
        app.feed_mouse(MouseEvent {
            kind: MouseEventKind::Down(MouseButton::Left),
            column: 5,
            row,
            modifiers: KeyModifiers::NONE,
        });
    }

    assert_eq!(
        app.focus,
        Pane::Status,
        "a click on the log or the keybar moves nothing"
    );
}

#[test]
fn image_selection_takes_over_the_right_pane() {
    let mut app = App::mock();
    // Row index in the tree, not a flat index into `mock_files()`: the
    // fixture spans several directories, so a directory header row can sit
    // ahead of the file this test is after.
    let png = (0..app.row_count(Pane::Files))
        .find(|&i| {
            Path::new(&app.file_display(i))
                .extension()
                .is_some_and(|e| e == "png")
        })
        .expect("mock has a .png entry");

    app.select(Pane::Files, png);
    let out = frame(&mut app, 120, 40);
    assert!(
        out.contains("Preview"),
        "image preview owns the right pane\n{out}"
    );
    assert!(!out.contains("diff --git"), "the mock diff is gone\n{out}");
}

#[test]
fn help_overlay_toggles() {
    let mut app = App::mock();
    // Not "keybindings": the welcome screen (Status is the default focus)
    // legitimately mentions it in its own one-liner ("Press ? for
    // keybindings"), so check for text unique to the help overlay's body.
    assert!(!frame(&mut app, 120, 40).contains("toggle this help"));

    app.show_help = true;
    assert!(frame(&mut app, 120, 40).contains("toggle this help"));
}

#[test]
fn survives_extremes_without_panicking() {
    for (w, h) in [(40, 20), (20, 8), (200, 60), (1, 1)] {
        let _ = frame(&mut App::mock(), w, h);
    }
}

/// `docs/PLAN_8_BRANCHES.md` S4: the new-branch popup reuses
/// `draw_commit_popup`'s shape — title, one input line, and a plain hints
/// footer (no sign-off/verify toggle row, unlike the commit popup).
#[test]
fn new_branch_popup_renders_title_and_hints() {
    let mut app = App::mock();
    app.select(Pane::Branches, 1);
    app.feed_key(KeyEvent::from(KeyCode::Char('n')));

    let out = frame(&mut app, 120, 40);
    assert!(out.contains("New branch"), "popup title shows:\n{out}");
    assert!(out.contains("Create: Enter"), "hints show:\n{out}");
    assert!(out.contains("Cancel: Esc"), "hints show:\n{out}");
    assert!(
        !out.contains("sign-off"),
        "no toggle row on this popup:\n{out}"
    );
}

/// `docs/PLAN_8_BRANCHES.md` S4/S3: `d` on a non-checked-out branch shows
/// the delete confirm in the keybar, same `confirm_line` rendering the
/// discard prompt already used.
#[test]
fn branch_delete_confirm_renders_in_the_keybar() {
    let mut app = App::mock();
    app.select(Pane::Branches, 1); // "feat/tui-skeleton", not the head
    app.feed_key(KeyEvent::from(KeyCode::Char('d')));

    let out = frame(&mut app, 120, 40);
    assert!(
        out.contains("delete branch feat/tui-skeleton?"),
        "confirm message shows:\n{out}"
    );
    assert!(out.contains("yes"), "y/n hints show:\n{out}");
    assert!(out.contains("cancel"), "y/n hints show:\n{out}");
}
