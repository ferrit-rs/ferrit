#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::pathbuf_init_then_push,
    clippy::iter_on_single_items,
    clippy::format_collect,
    elided_lifetimes_in_paths,
    reason = "integration test scaffolding: a failed setup is the assertion, helper ergonomics beat lint-cleanliness here"
)]
//! Scrollbar coverage for both the left-column lists and the right-pane
//! diff: a scrollbar shows only when content overflows, its thumb tracks
//! `position / (total - viewport)` so it reaches both ends of the track,
//! and on the left column it is coloured like the pane's border (green
//! when focused, grey otherwise).

use std::fs;
use std::path::{Path, PathBuf};

use ferrit::components::mock;
use ferrit::domain::app::{App, Pane};
use git2::{IndexAddOption, Repository, Signature};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::style::Color;

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

fn commit_all(repo: &Repository, message: &str) {
    let mut index = repo.index().unwrap();
    index
        .add_all(["*"].iter(), IndexAddOption::DEFAULT, None)
        .unwrap();
    index.write().unwrap();
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
    let sig = Signature::now("Test", "test@example.com").unwrap();
    let parent = repo
        .head()
        .ok()
        .and_then(|h| h.target())
        .and_then(|oid| repo.find_commit(oid).ok());
    let parents: Vec<&git2::Commit> = parent.iter().collect();
    repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)
        .unwrap();
}

/// A repo with 40 commits, so the Commits pane overflows any reasonable
/// terminal height.
fn many_commits_repo(tag: &str) -> (TempDir, App) {
    let dir = TempDir::new(tag);
    let repo = Repository::init(dir.path()).unwrap();
    fs::write(dir.path().join("f.txt"), "0\n").unwrap();
    for n in 0..40 {
        fs::write(dir.path().join("f.txt"), format!("{n}\n")).unwrap();
        commit_all(&repo, &format!("commit {n}"));
    }
    let app = App::open(dir.path()).unwrap();
    (dir, app)
}

/// A repo with a tall two-hunk diff on `a_tall.txt`, `Files` focused on it.
fn tall_diff_repo(tag: &str) -> (TempDir, App) {
    let dir = TempDir::new(tag);
    let repo = Repository::init(dir.path()).unwrap();
    let base: String = (0..200).map(|n| format!("line {n}\n")).collect();
    fs::write(dir.path().join("a_tall.txt"), &base).unwrap();
    commit_all(&repo, "init");
    let edited = base
        .replace("line 5\n", "line 5 CHANGED\n")
        .replace("line 180\n", "line 180 CHANGED\n");
    fs::write(dir.path().join("a_tall.txt"), &edited).unwrap();

    let mut app = App::open(dir.path()).unwrap();
    app.select(Pane::Files, 0);
    (dir, app)
}

fn render_buffer(app: &mut App, w: u16, h: u16) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
    terminal
        .draw(|f| ferrit::components::screens::draw(f, app))
        .unwrap();
    terminal.backend().buffer().clone()
}

/// Every `(x, y)` in `buf` whose cell symbol is `symbol`, restricted to
/// `x_range`.
fn cells_with(buf: &Buffer, symbol: &str, x_range: std::ops::Range<u16>) -> Vec<(u16, u16)> {
    let mut found = Vec::new();
    for y in 0..buf.area.height {
        for x in x_range.clone() {
            if buf[(x, y)].symbol() == symbol {
                found.push((x, y));
            }
        }
    }
    found
}

const THUMB: &str = "\u{2588}"; // █

/// `(min_y, max_y)` over rows in `x_range` whose symbol is `THUMB`.
fn thumb_span(buf: &Buffer, x_range: std::ops::Range<u16>) -> (u16, u16) {
    let cells = cells_with(buf, THUMB, x_range);
    let min = cells.iter().map(|&(_, y)| y).min().expect("no thumb found");
    let max = cells.iter().map(|&(_, y)| y).max().expect("no thumb found");
    (min, max)
}

// --- Right pane (diff) -----------------------------------------------------

#[test]
fn a_tall_diff_gets_a_scrollbar() {
    let (_dir, mut app) = tall_diff_repo("sb-right-yes");
    let buf = render_buffer(&mut app, 100, 12);
    assert!(
        !cells_with(&buf, THUMB, 0..buf.area.width).is_empty(),
        "a diff taller than the pane draws a scrollbar\n{buf:?}"
    );
}

#[test]
fn a_short_diff_has_no_scrollbar() {
    let mut app = App::mock();
    app.focus = Pane::Files;
    let buf = render_buffer(&mut app, 120, 40);
    // Right pane only (`side = (120 / 3).max(24) == 40`): with Files focused
    // the accordion squashes Branches/Commits/Stash to their 3-row floor, so
    // their few mock rows legitimately overflow and draw their own left-column
    // scrollbars; this test is about the right-pane diff, not that.
    assert!(
        cells_with(&buf, THUMB, 40..buf.area.width).is_empty(),
        "the mock diff fits, no scrollbar expected\n{buf:?}"
    );
}

/// Regression test: `ScrollbarState` needs `viewport_content_length` set,
/// otherwise the thumb is sized against the raw line count instead of the
/// max scroll (`total - viewport`) and never reaches the track's end.
#[test]
fn right_pane_thumb_reaches_the_bottom_at_max_scroll() {
    let (_dir, mut app) = tall_diff_repo("sb-right-bottom");
    let buf = render_buffer(&mut app, 100, 14); // establishes the real viewport height
    // The diff track ends above the 5-row command log, 1-row keybar, the
    // pane's top and bottom borders, and its 1-row stat line.
    let track_bottom = buf.area.height - 5 - 1 - 2 - 1;
    for _ in 0..1000 {
        app.feed_key(ratatui::crossterm::event::KeyEvent::from(
            ratatui::crossterm::event::KeyCode::Char('J'),
        ));
    }

    let buf = render_buffer(&mut app, 100, 14);
    let (_, max_y) = thumb_span(&buf, 0..buf.area.width);
    assert_eq!(
        max_y, track_bottom,
        "thumb should touch the track's bottom once scrolled to the max\n{buf:?}"
    );
}

#[test]
fn right_pane_thumb_starts_at_the_top_at_zero_scroll() {
    let (_dir, mut app) = tall_diff_repo("sb-right-top");
    let buf = render_buffer(&mut app, 100, 14);
    assert_eq!(app.right_scroll(), 0);

    let (min_y, _) = thumb_span(&buf, 0..buf.area.width);
    assert_eq!(
        min_y,
        2, // border (y=0) + stat line (y=1): the diff track starts at y=2
        "thumb should touch the track's top at zero scroll\n{buf:?}"
    );
}

// --- Left column (lists) ----------------------------------------------------

/// x < 33 at width 100 (`side = (100 / 3).max(24) == 33`) is the left column.
const LEFT_COLUMN: std::ops::Range<u16> = 0..33;

#[test]
fn an_overflowing_list_gets_a_scrollbar() {
    let (_dir, mut app) = many_commits_repo("sb-left-yes");
    app.focus = Pane::Commits;
    let buf = render_buffer(&mut app, 100, 20);
    assert!(
        !cells_with(&buf, THUMB, LEFT_COLUMN).is_empty(),
        "40 commits in a short pane should draw a scrollbar\n{buf:?}"
    );
}

#[test]
fn a_short_list_has_no_scrollbar() {
    // The mock fixtures (a handful of rows each) fit comfortably at height
    // 50: `mock_files()` spans several directories, so the Files pane is a
    // tree (root + dir headers + files) — 8 rows, not 4 — needing a little
    // more room than the other, still-flat panes.
    let buf = render_buffer(&mut App::mock(), 120, 50);
    assert!(
        cells_with(&buf, THUMB, 0..40).is_empty(),
        "short mock lists should draw no scrollbar\n{buf:?}"
    );
}

#[test]
fn left_scrollbar_is_green_when_its_pane_is_focused() {
    let (_dir, mut app) = many_commits_repo("sb-left-green");
    app.focus = Pane::Commits;
    let buf = render_buffer(&mut app, 100, 20);

    let coloured: Vec<_> = LEFT_COLUMN
        .flat_map(|x| (0..buf.area.height).map(move |y| (x, y)))
        .filter(|&(x, y)| {
            let cell = &buf[(x, y)];
            cell.symbol() == THUMB
        })
        .collect();
    assert!(
        !coloured.is_empty(),
        "expected scrollbar cells in the focused Commits pane\n{buf:?}"
    );
    for (x, y) in coloured {
        assert_eq!(
            buf[(x, y)].style().fg,
            Some(Color::Green),
            "focused pane's scrollbar should be green at ({x}, {y})\n{buf:?}"
        );
    }
}

#[test]
fn left_scrollbar_is_not_green_when_its_pane_is_unfocused() {
    let (_dir, mut app) = many_commits_repo("sb-left-grey");
    app.focus = Pane::Status; // Commits still overflows, but is not focused

    let buf = render_buffer(&mut app, 100, 20);
    let scrollbar_cells: Vec<_> = LEFT_COLUMN
        .flat_map(|x| (0..buf.area.height).map(move |y| (x, y)))
        .filter(|&(x, y)| {
            let cell = &buf[(x, y)];
            cell.symbol() == THUMB
        })
        .collect();
    assert!(
        !scrollbar_cells.is_empty(),
        "expected scrollbar cells in the unfocused Commits pane\n{buf:?}"
    );
    for (x, y) in scrollbar_cells {
        assert_ne!(
            buf[(x, y)].style().fg,
            Some(Color::Green),
            "unfocused pane's scrollbar should not be green at ({x}, {y})\n{buf:?}"
        );
    }
}

#[test]
fn commits_pane_still_renders_with_the_mock_fixture() {
    // Sanity: mock data is untouched by the scrollbar change.
    let out = {
        let mut app = App::mock();
        let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
        terminal
            .draw(|f| ferrit::components::screens::draw(f, &mut app))
            .unwrap();
        terminal.backend().to_string()
    };
    let hash = mock::mock_commits().first().unwrap().short_hash.clone();
    assert!(out.contains(&hash), "commit rows still render\n{out}");
}
