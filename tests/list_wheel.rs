#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::pathbuf_init_then_push,
    reason = "integration test scaffolding: a failed setup is the assertion"
)]
//! The mouse wheel over a left pane (lazygit): it scrolls the list under the
//! pointer, whichever pane is focused, and leaves the focus, the selection and
//! the right pane alone. Found by comparing a flow with lazygit
//! (`test/flows/ui-mouse.flow`); see `docs/PLAN_4_SCROLL_BEHAVIOR.md`.

use std::fs;
use std::path::{Path, PathBuf};

use ferrit::app::{App, Pane, screens};
use git2::{Repository, Signature};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};

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

/// A repository with `count` commits, `c00` first and `c{count-1}` at the top.
fn repo_with_commits(tag: &str, count: usize) -> (TempDir, App) {
    let dir = TempDir::new(tag);
    let repo = Repository::init(dir.path()).unwrap();
    let sig = Signature::now("Test", "test@example.com").unwrap();
    let mut parent: Option<git2::Commit<'_>> = None;
    for n in 0..count {
        fs::write(dir.path().join("f.txt"), format!("{n}\n")).unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("f.txt")).unwrap();
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let parents: Vec<&git2::Commit<'_>> = parent.iter().collect();
        let oid = repo
            .commit(
                Some("HEAD"),
                &sig,
                &sig,
                &format!("c{n:02}"),
                &tree,
                &parents,
            )
            .unwrap();
        parent = Some(repo.find_commit(oid).unwrap());
    }
    let app = App::open(dir.path()).unwrap();
    (dir, app)
}

fn frame(app: &mut App) -> String {
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    terminal.draw(|f| screens::draw(f, app)).unwrap();
    terminal.backend().to_string()
}

/// Wheel `ticks` times at the middle of `pane`, as drawn in the last frame.
fn wheel(app: &mut App, out: &str, pane_title: &str, kind: MouseEventKind, ticks: usize) {
    let row = out
        .lines()
        .position(|line| line.contains(pane_title))
        .expect("pane title on screen");
    for _ in 0..ticks {
        app.feed_mouse(MouseEvent {
            kind,
            column: 5,
            row: u16::try_from(row + 2).unwrap(),
            modifiers: KeyModifiers::NONE,
        });
    }
}

#[test]
fn wheel_over_an_unfocused_list_scrolls_it_and_leaves_focus_and_selection() {
    let (_dir, mut app) = repo_with_commits("wheel-unfocused", 60);
    let out = frame(&mut app);
    app.select(Pane::Branches, 0);
    let right_before = app.right_scroll();

    wheel(&mut app, &out, "[4] Commits", MouseEventKind::ScrollDown, 3);

    assert_eq!(app.focus, Pane::Branches, "the focus did not move");
    assert_eq!(app.selected(Pane::Branches), 0, "Branches did not move");
    assert_eq!(app.selected(Pane::Commits), 0, "nor did the Commits row");
    assert_eq!(app.list_offset(Pane::Commits), 6, "two rows a tick");
    assert_eq!(app.right_scroll(), right_before);
}

#[test]
fn wheel_scrolls_the_view_and_the_selection_stays_off_screen() {
    let (_dir, mut app) = repo_with_commits("wheel-view", 60);
    app.select(Pane::Commits, 0);
    let out = frame(&mut app);

    wheel(
        &mut app,
        &out,
        "[4] Commits",
        MouseEventKind::ScrollDown,
        10,
    );
    let out = frame(&mut app);

    assert_eq!(app.selected(Pane::Commits), 0, "the selection stayed");
    assert!(
        app.list_offset(Pane::Commits) > 0,
        "the view left the top, and a draw did not pull it back to the selection"
    );
    assert!(
        !out.contains("T o c59"),
        "the newest commit (selected) scrolled out of the list\n{out}"
    );

    // Any other selection brings the view back to it.
    app.feed_key(KeyEvent::from(KeyCode::Char('j')));
    let out = frame(&mut app);
    assert_eq!(app.selected(Pane::Commits), 1);
    assert!(
        out.contains("c58"),
        "the selected row is visible again\n{out}"
    );
}

#[test]
fn wheel_up_at_the_top_and_down_past_the_end_clamp() {
    let (_dir, mut app) = repo_with_commits("wheel-clamp", 30);
    app.select(Pane::Commits, 0);
    let out = frame(&mut app);

    wheel(&mut app, &out, "[4] Commits", MouseEventKind::ScrollUp, 5);
    frame(&mut app);
    assert_eq!(app.list_offset(Pane::Commits), 0, "not above the first row");

    wheel(
        &mut app,
        &out,
        "[4] Commits",
        MouseEventKind::ScrollDown,
        200,
    );
    let out = frame(&mut app);
    assert!(
        out.contains("T o c00"),
        "the last commit is still on screen at the bottom\n{out}"
    );
    assert!(
        app.list_offset(Pane::Commits) < 30,
        "the offset stops at the last full page"
    );
}

#[test]
fn wheel_outside_every_left_pane_does_nothing() {
    let (_dir, mut app) = repo_with_commits("wheel-outside", 30);
    app.select(Pane::Commits, 3);
    frame(&mut app);
    app.feed_mouse(MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: 119,
        row: 39,
        modifiers: KeyModifiers::NONE,
    });
    assert_eq!(app.selected(Pane::Commits), 3);
    assert_eq!(app.list_offset(Pane::Commits), 0);
}
