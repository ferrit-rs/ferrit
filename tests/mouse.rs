//! A left click focuses the pane it lands in and, on a real list row,
//! moves that pane's selection cursor there. `App::mock()` (no repo, no
//! terminal) plus the `set_left_area` / `set_list_offset` test seams stand
//! in for a real frame. See `docs/PLAN_5_CLICK_BEHAVIOR.md` milestone C1.

use ferrit::app::{App, Pane};
use ratatui::crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

fn left_click(column: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

#[test]
fn click_selects_a_row_in_files() {
    let mut app = App::mock();
    let files = Rect {
        x: 0,
        y: 4,
        width: 24,
        height: 5,
    };
    app.set_left_area(Pane::Files, files);
    app.set_list_offset(Pane::Files, 0);

    // row 6: inner_row = 6 - 4 - 1 = 1
    app.feed_mouse(left_click(10, 6));

    assert_eq!(app.focus, Pane::Files);
    assert_eq!(app.selected(Pane::Files), 1);
}

#[test]
fn click_focuses_commits_and_selects_its_row() {
    let mut app = App::mock();
    let commits = Rect {
        x: 0,
        y: 20,
        width: 24,
        height: 6,
    };
    app.set_left_area(Pane::Commits, commits);
    app.set_list_offset(Pane::Commits, 0);

    // row 22: inner_row = 22 - 20 - 1 = 1
    app.feed_mouse(left_click(5, 22));

    assert_eq!(app.focus, Pane::Commits);
    assert_eq!(app.selected(Pane::Commits), 1);
}

#[test]
fn click_honours_a_scrolled_list_offset() {
    let mut app = App::mock();
    let files = Rect {
        x: 0,
        y: 0,
        width: 24,
        height: 5,
    };
    app.set_left_area(Pane::Files, files);
    app.set_list_offset(Pane::Files, 1); // list scrolled down by one row

    // row 1: inner_row = 1 - 0 - 1 = 0, so index = list_offset + 0 = 1
    app.feed_mouse(left_click(5, 1));

    assert_eq!(
        app.selected(Pane::Files),
        1,
        "the first visible row is model index 1, not 0, once scrolled"
    );
}

#[test]
fn click_on_the_border_focuses_without_moving_the_cursor() {
    let mut app = App::mock();
    app.select(Pane::Commits, 2);
    app.select(Pane::Files, 0); // move focus away; Commits keeps its cursor

    let commits = Rect {
        x: 0,
        y: 20,
        width: 24,
        height: 6,
    };
    app.set_left_area(Pane::Commits, commits);

    app.feed_mouse(left_click(5, commits.y)); // the title/border row itself

    assert_eq!(
        app.focus,
        Pane::Commits,
        "a border click still focuses the pane"
    );
    assert_eq!(
        app.selected(Pane::Commits),
        2,
        "the cursor is untouched by a border click"
    );
}

#[test]
fn click_past_the_last_row_focuses_without_moving_the_cursor() {
    let mut app = App::mock();
    app.select(Pane::Branches, 1);
    app.select(Pane::Files, 0);

    let branches = Rect {
        x: 0,
        y: 10,
        width: 24,
        height: 6,
    };
    app.set_left_area(Pane::Branches, branches);
    app.set_list_offset(Pane::Branches, 0);

    // row 15: inner_row = 15 - 10 - 1 = 4, past mock_branches' 2 entries.
    app.feed_mouse(left_click(5, 15));

    assert_eq!(app.focus, Pane::Branches);
    assert_eq!(
        app.selected(Pane::Branches),
        1,
        "the cursor is untouched by a click past the last row"
    );
}

#[test]
fn click_outside_every_pane_is_a_no_op() {
    let mut app = App::mock();
    // No `set_left_area` call: every pane's rect is still `Rect::ZERO`, so
    // nothing on screen contains a click, whatever the coordinates.
    app.feed_mouse(left_click(50, 50));

    assert_eq!(app.focus, Pane::Status, "the default focus is unchanged");
}

#[test]
fn any_click_dismisses_the_help_overlay_and_nothing_else() {
    let mut app = App::mock();
    app.show_help = true;
    let focus_before = app.focus;

    app.feed_mouse(left_click(5, 5));

    assert!(!app.show_help, "the click dismissed the overlay");
    assert_eq!(app.focus, focus_before, "the same click did nothing else");
}

#[test]
fn only_a_left_click_routes_to_a_pane() {
    let mut app = App::mock();
    let files = Rect {
        x: 0,
        y: 4,
        width: 24,
        height: 5,
    };
    app.set_left_area(Pane::Files, files);

    let ev = |kind: MouseEventKind| MouseEvent {
        kind,
        column: 10,
        row: 6,
        modifiers: KeyModifiers::NONE,
    };

    app.feed_mouse(ev(MouseEventKind::Down(MouseButton::Right)));
    app.feed_mouse(ev(MouseEventKind::Down(MouseButton::Middle)));
    app.feed_mouse(ev(MouseEventKind::Drag(MouseButton::Left)));
    app.feed_mouse(ev(MouseEventKind::Moved));

    assert_eq!(app.focus, Pane::Status, "none of these focus Files");
    assert_eq!(app.selected(Pane::Files), 0);
}

#[test]
fn extreme_coordinates_and_offsets_never_panic() {
    let mut app = App::mock();
    let tiny = Rect {
        x: 0,
        y: u16::MAX - 1,
        width: 5,
        height: 2,
    };
    app.set_left_area(Pane::Files, tiny);
    // A naive `list_offset + inner_row` would overflow here; click_row must
    // saturate instead, same as `area.y + 1` must saturate for a pane
    // pinned at the very bottom of `u16`'s range.
    app.set_list_offset(Pane::Files, usize::MAX);

    for (column, row) in [
        (0, 0),
        (u16::MAX, u16::MAX),
        (0, u16::MAX),
        (u16::MAX, 0),
        (tiny.y, tiny.y), // exactly the border row of a pane near u16::MAX
    ] {
        app.feed_mouse(left_click(column, row));
    }

    // Reaching this line without a panic is the real assertion; none of
    // those clicks landed on a real row, so nothing should have moved.
    assert_eq!(app.selected(Pane::Files), 0);
}

#[test]
fn click_on_a_files_directory_row_toggles_it() {
    let mut app = App::mock();
    let before = app.row_count(Pane::Files);
    let files = Rect {
        x: 0,
        y: 0,
        width: 24,
        height: 10,
    };
    app.set_left_area(Pane::Files, files);
    app.set_list_offset(Pane::Files, 0);

    // `mock_files()` spans several directories, so this is a tree; find a
    // directory row (empty `file_display`) rather than assuming one. Row 0
    // (the root "/") is always one when nested, so it's a sound fallback.
    let dir_row = (0..before)
        .find(|&i| app.file_display(i).is_empty())
        .unwrap_or(0);
    // row = area.y + 1 (border) + dir_row (no scroll offset)
    app.feed_mouse(left_click(
        5,
        u16::try_from(dir_row).unwrap_or(u16::MAX) + 1,
    ));

    assert_eq!(app.focus, Pane::Files, "the click also focused Files");
    assert_eq!(app.selected(Pane::Files), dir_row);
    assert!(
        app.row_count(Pane::Files) < before,
        "clicking a directory row collapses it, same as Enter"
    );
}
