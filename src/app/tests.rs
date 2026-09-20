//! `App` unit tests.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "unit test: a failed setup or a bad index is the assertion"
)]

use super::*;

fn press(app: &mut App, code: KeyCode) {
    app.on_key(KeyEvent::from(code));
}

#[test]
fn arrows_cycle_panes_and_wrap() {
    let mut app = App::mock();
    press(&mut app, KeyCode::Right);
    assert_eq!(app.focus, Pane::Files);
    press(&mut app, KeyCode::Left);
    assert_eq!(app.focus, Pane::Status);
    press(&mut app, KeyCode::Left);
    assert_eq!(app.focus, Pane::Stash, "Left from the first pane wraps");
}

#[test]
fn selection_clamps_at_both_ends() {
    let mut app = App::mock();
    // Not `mock_files().len() - 1`: the mock fixture spans several
    // directories, so the Files pane is a tree (root + dir headers +
    // files), more rows than files.
    let last = app.row_count(Pane::Files) - 1;
    press(&mut app, KeyCode::Char('2')); // focus Files
    for _ in 0..20 {
        press(&mut app, KeyCode::Down);
    }
    assert_eq!(app.selected(Pane::Files), last);
    for _ in 0..20 {
        press(&mut app, KeyCode::Up);
    }
    assert_eq!(app.selected(Pane::Files), 0);
}

#[test]
fn image_selection_builds_an_image_preview() {
    let mut app = App::mock();
    // Row index in the tree, not a flat index into `mock_files()`: the
    // fixture spans several directories, so a directory header row can
    // sit ahead of the file this test is after.
    let png = (0..app.row_count(Pane::Files))
        .find(|&i| {
            Path::new(&app.file_display(i))
                .extension()
                .is_some_and(|e| e == "png")
        })
        .expect("mock has a .png entry");

    press(&mut app, KeyCode::Char('2')); // focus Files
    assert!(
        matches!(app.preview(), Preview::None),
        "src/main.rs is not an image"
    );

    for _ in 0..png {
        press(&mut app, KeyCode::Down);
    }
    assert!(
        matches!(app.preview(), Preview::Image(_)),
        "the embedded PNG decodes on the half-block picker"
    );

    press(&mut app, KeyCode::Char('1')); // leave Files
    assert!(matches!(app.preview(), Preview::None));
}

#[test]
fn help_overlay_swallows_navigation() {
    let mut app = App::mock();
    press(&mut app, KeyCode::Char('?'));
    assert!(app.show_help);
    press(&mut app, KeyCode::Right);
    assert_eq!(app.focus, Pane::Status, "nav is inert while help is up");
    press(&mut app, KeyCode::Char('?'));
    assert!(!app.show_help);
}

#[test]
fn refresh_without_repo_is_a_noop() {
    let mut app = App::mock();
    let before = app.file_lines().len();
    app.refresh();
    assert_eq!(app.file_lines().len(), before);
}
