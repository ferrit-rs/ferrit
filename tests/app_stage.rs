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
//! `App`-level wiring for staging (`docs/PLAN_6_STAGING.md`): `Enter` focuses
//! the diff, `j`/`k` skip context lines, `<space>` stages/unstages the
//! cursor's hunk or a V-selection, `d` discards after a confirm, and the
//! cursor tracks the same hunk across the refresh a stage triggers.

mod common;

use common::{TempDir, commit_all};
use std::fs;

use ferrit::git::diff::DiffSide;
use ferrit::tui::App;
use ferrit::tui::state::pane::Pane;
use ferrit::tui::state::views::DiffView;
use git2::Repository;
use ratatui::crossterm::event::{KeyCode, KeyEvent};

fn files_row(app: &App, name: &str) -> usize {
    (0..app.row_count(Pane::Files))
        .find(|&i| app.file_display(i).ends_with(name))
        .unwrap_or_else(|| panic!("no Files row for {name}"))
}

fn char_key(c: char) -> KeyEvent {
    KeyEvent::from(KeyCode::Char(c))
}

/// Text of the diff line the cursor sits on, or the empty string outside
/// `Mode::Diff` — read straight off the same `DiffView::Files` the render
/// layer uses, never a private field.
fn cursor_line_text(app: &App) -> String {
    let Some((side, line, _)) = app.diff_cursor() else {
        return String::new();
    };
    let DiffView::Files(files) = app.diff_view() else {
        panic!("expected a Files split");
    };
    let diff = match side {
        DiffSide::Worktree => &files.unstaged,
        DiffSide::Staged => &files.staged,
    };
    diff.text.lines().nth(line).unwrap_or_default().to_owned()
}

/// A repo with one file (`f.txt`) carrying two separate hunks close enough
/// together to land in the same `git diff` output, plus a second untouched
/// file so the Files pane has more than one row.
fn two_hunk_repo(tag: &str) -> (TempDir, App) {
    let dir = TempDir::new(tag);
    let repo = Repository::init(dir.path()).unwrap();
    let base: String = (0..40).map(|n| format!("line {n}\n")).collect();
    fs::write(dir.path().join("f.txt"), &base).unwrap();
    fs::write(dir.path().join("z_other.txt"), "keep\n").unwrap();
    commit_all(&repo, "init");

    let edited = base
        .replace("line 3\n", "line 3 CHANGED\n")
        .replace("line 30\n", "line 30 CHANGED\n");
    fs::write(dir.path().join("f.txt"), &edited).unwrap();

    let mut app = App::open(dir.path()).unwrap();
    app.select(Pane::Files, files_row(&app, "f.txt"));
    (dir, app)
}

#[test]
fn enter_focuses_the_diff_on_the_first_selectable_line() {
    let (_dir, mut app) = two_hunk_repo("app-stage-enter");
    assert!(app.diff_cursor().is_none(), "Mode::Nav to start");

    app.feed_key(KeyEvent::from(KeyCode::Enter));

    let (side, _, anchor) = app.diff_cursor().expect("Enter focused the diff");
    assert_eq!(
        side,
        DiffSide::Worktree,
        "there's an unstaged change to stage"
    );
    assert!(anchor.is_none(), "no V-selection yet");
    let line = cursor_line_text(&app);
    assert!(
        line.starts_with('+') || line.starts_with('-'),
        "cursor starts on a real change, not context: {line:?}"
    );
}

#[test]
fn j_and_k_step_over_context_without_ever_landing_on_it() {
    let (_dir, mut app) = two_hunk_repo("app-stage-jk");
    app.feed_key(KeyEvent::from(KeyCode::Enter));
    let start = cursor_line_text(&app);

    let mut seen = std::collections::HashSet::new();
    let mut positions = Vec::new();
    for _ in 0..30 {
        app.feed_key(char_key('j'));
        let line = cursor_line_text(&app);
        assert!(
            line.starts_with('+') || line.starts_with('-'),
            "j landed on a context line: {line:?}"
        );
        seen.insert(line);
        let (_, pos, _) = app.diff_cursor().expect("still in Mode::Diff");
        positions.push(pos);
    }
    assert!(
        seen.len() > 1,
        "j actually moved the cursor across more than one line"
    );
    assert!(
        positions.windows(2).all(|w| w[1] >= w[0]),
        "repeated j never moves backward, e.g. by snapping back to the \
         first hunk once the cursor crosses into a later one: {positions:?}"
    );

    for _ in 0..30 {
        app.feed_key(char_key('k'));
        let line = cursor_line_text(&app);
        assert!(
            line.starts_with('+') || line.starts_with('-'),
            "k landed on a context line: {line:?}"
        );
    }
    assert_eq!(cursor_line_text(&app), start, "k walked all the way back");
}

/// Regression: `move_diff_cursor` used to leave `DiffCursor::hunk_id`
/// pointing at the *old* hunk when the cursor crossed into a new one.
/// `update_diff` calls `resync_diff_cursor` after every key (not just a
/// stage), so the very next `j`/`k` saw "this line isn't in the hunk the id
/// names" and snapped the cursor straight back to the first hunk — from the
/// user's side, `j` looked stuck after the first change.
#[test]
fn the_cursor_settles_in_the_second_hunk_instead_of_snapping_back() {
    let (_dir, mut app) = two_hunk_repo("app-stage-cross-hunk");
    app.feed_key(KeyEvent::from(KeyCode::Enter));

    for _ in 0..10 {
        app.feed_key(char_key('j'));
    }
    let (_, forward_line, _) = app.diff_cursor().expect("still in Mode::Diff");

    for _ in 0..10 {
        app.feed_key(char_key('k'));
    }
    let (_, back_line, _) = app.diff_cursor().expect("still in Mode::Diff");

    assert!(
        forward_line > back_line,
        "10 j then 10 k should have made real forward progress before \
         coming back: forward={forward_line}, back={back_line}"
    );
}

#[test]
fn h_and_esc_leave_diff_mode() {
    let (_dir, mut app) = two_hunk_repo("app-stage-leave");
    app.feed_key(KeyEvent::from(KeyCode::Enter));
    assert!(app.diff_cursor().is_some());

    app.feed_key(char_key('h'));
    assert!(app.diff_cursor().is_none(), "h backs out to Mode::Nav");

    app.feed_key(KeyEvent::from(KeyCode::Enter));
    assert!(app.diff_cursor().is_some());
    app.feed_key(KeyEvent::from(KeyCode::Esc));
    assert!(app.diff_cursor().is_none(), "Esc backs out too");
}

/// Staging one hunk out of two: the file goes from fully unstaged to
/// partially staged, and the cursor lands on the other hunk (still on the
/// worktree side, since it is still unstaged) rather than losing its place.
#[test]
fn space_stages_the_cursors_hunk_and_the_cursor_tracks_the_remaining_one() {
    let (_dir, mut app) = two_hunk_repo("app-stage-space");
    app.feed_key(KeyEvent::from(KeyCode::Enter));

    app.feed_key(char_key(' '));

    let display = app.file_display(files_row(&app, "f.txt"));
    assert!(
        display.starts_with('M') && display.contains('M'),
        "partially staged (M in both the staged and worktree columns): {display}"
    );

    let (side, _, _) = app
        .diff_cursor()
        .expect("cursor still resolves to a present hunk");
    assert_eq!(
        side,
        DiffSide::Worktree,
        "the second hunk is still unstaged"
    );
    let line = cursor_line_text(&app);
    assert!(line.starts_with('+') || line.starts_with('-'));

    // Stage the rest: no more worktree changes on this file, so the diff
    // cursor has nothing left on its side and falls back to `Mode::Nav`.
    app.feed_key(char_key(' '));
    assert!(
        app.diff_cursor().is_none(),
        "no worktree changes left on this side: back to Mode::Nav"
    );
    let display = app.file_display(files_row(&app, "f.txt"));
    assert!(display.starts_with("M "), "fully staged: {display}");
}

#[test]
fn space_on_a_files_row_stages_the_whole_file_then_unstages_it() {
    let dir = TempDir::new("app-stage-file-toggle");
    let repo = Repository::init(dir.path()).unwrap();
    fs::write(dir.path().join("a.txt"), "one\n").unwrap();
    commit_all(&repo, "init");
    fs::write(dir.path().join("a.txt"), "one\ntwo\n").unwrap();

    let mut app = App::open(dir.path()).unwrap();
    app.select(Pane::Files, files_row(&app, "a.txt"));

    app.feed_key(char_key(' '));
    assert_eq!(app.file_display(files_row(&app, "a.txt")), "M  a.txt");

    app.feed_key(char_key(' '));
    assert_eq!(app.file_display(files_row(&app, "a.txt")), " M a.txt");
}

#[test]
fn v_select_stages_only_the_selected_lines() {
    let dir = TempDir::new("app-stage-vselect");
    let repo = Repository::init(dir.path()).unwrap();
    fs::write(dir.path().join("f.txt"), "base\n").unwrap();
    commit_all(&repo, "init");
    fs::write(dir.path().join("f.txt"), "base\none\ntwo\nthree\n").unwrap();

    let mut app = App::open(dir.path()).unwrap();
    app.select(Pane::Files, files_row(&app, "f.txt"));
    app.feed_key(KeyEvent::from(KeyCode::Enter));

    // Cursor starts on "+one"; move to "+two" and V-select just that line.
    app.feed_key(char_key('j'));
    assert_eq!(cursor_line_text(&app), "+two");
    app.feed_key(char_key('V'));
    app.feed_key(char_key(' '));

    let DiffView::Files(files) = app.diff_view() else {
        panic!("expected a Files split");
    };
    assert!(files.staged.text.contains("+two"));
    assert!(!files.staged.text.contains("+one"));
    assert!(!files.staged.text.contains("+three"));
    assert!(files.unstaged.text.contains("+one"));
    assert!(files.unstaged.text.contains("+three"));
}

#[test]
fn discard_asks_first_and_only_runs_on_y() {
    let dir = TempDir::new("app-stage-discard");
    let repo = Repository::init(dir.path()).unwrap();
    fs::write(dir.path().join("a.txt"), "one\n").unwrap();
    commit_all(&repo, "init");
    fs::write(dir.path().join("a.txt"), "one\ntwo\n").unwrap();

    let mut app = App::open(dir.path()).unwrap();
    app.select(Pane::Files, files_row(&app, "a.txt"));

    app.feed_key(char_key('d'));
    assert!(app.confirm_message().is_some(), "asks first");

    // 'n' cancels: the worktree change is untouched.
    app.feed_key(char_key('n'));
    assert!(app.confirm_message().is_none());
    assert_eq!(
        fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "one\ntwo\n"
    );

    app.feed_key(char_key('d'));
    app.feed_key(char_key('y'));
    assert!(app.confirm_message().is_none());
    assert_eq!(
        fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "one\n",
        "confirmed discard reverted the worktree"
    );
}

/// Lazygit parity: a flat change list still opens on the `▼ /` root row
/// ("1 of 3"), `j` reaches the first file, and Space on the root stages
/// everything under it.
#[test]
fn a_flat_file_list_starts_on_the_root_row() {
    let dir = TempDir::new("app-stage-root-first");
    let repo = Repository::init(dir.path()).unwrap();
    fs::write(dir.path().join("README.md"), "one\n").unwrap();
    commit_all(&repo, "init");
    fs::write(dir.path().join("README.md"), "one\ntwo\n").unwrap();
    fs::write(dir.path().join("FLOW_NOTES.md"), "notes\n").unwrap();

    let mut app = App::open(dir.path()).unwrap();
    app.feed_key(char_key('2'));
    assert_eq!(app.selected(Pane::Files), 0);
    assert_eq!(app.counter(Pane::Files), Some((1, 3)));
    assert!(app.files_selection_is_dir());

    app.feed_key(char_key('j'));
    assert_eq!(app.selected(Pane::Files), 1);
    assert!(!app.files_selection_is_dir());

    app.feed_key(char_key('k'));
    app.feed_key(char_key(' '));
    let staged = repo.statuses(None).unwrap();
    assert!(
        staged
            .iter()
            .all(|e| e.status().is_index_modified() || e.status().is_index_new()),
        "Space on the root stages every changed file"
    );
}

#[test]
fn a_lone_top_level_file_has_no_root_row_like_lazygit() {
    let dir = TempDir::new("app-stage-lone-file");
    let repo = Repository::init(dir.path()).unwrap();
    fs::write(dir.path().join("README.md"), "one\n").unwrap();
    commit_all(&repo, "init");
    fs::write(dir.path().join("README.md"), "one\ntwo\n").unwrap();

    let mut app = App::open(dir.path()).unwrap();
    app.feed_key(char_key('2'));
    assert_eq!(app.selected(Pane::Files), 0);
    assert_eq!(app.counter(Pane::Files), Some((1, 1)));
    assert!(!app.files_selection_is_dir());
}

fn file_labels(app: &App) -> Vec<String> {
    app.file_lines()
        .iter()
        .map(|line| {
            line.spans
                .iter()
                .map(|s| s.content.as_ref())
                .collect::<String>()
        })
        .map(|text| text.trim().to_owned())
        .collect()
}

fn changed_repo(tag: &str, files: &[&str]) -> (TempDir, Repository) {
    let dir = TempDir::new(tag);
    let repo = Repository::init(dir.path()).unwrap();
    fs::write(dir.path().join("seed.txt"), "seed\n").unwrap();
    commit_all(&repo, "init");
    for file in files {
        let path = dir.path().join(file);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, "x\n").unwrap();
    }
    (dir, repo)
}

/// Lazygit shape: one directory with one or two files has no root row, the
/// directory row comes first at depth 0.
#[test]
fn a_single_directory_has_no_root_row() {
    let (dir, _repo) = changed_repo("app-tree-one-dir", &["flow_dir/a.txt"]);
    let mut app = App::open(dir.path()).unwrap();
    app.feed_key(char_key('2'));
    assert_eq!(app.counter(Pane::Files), Some((1, 2)));
    assert!(app.files_selection_is_dir());
    assert!(file_labels(&app)[0].contains("flow_dir"));
    assert!(!file_labels(&app)[0].contains('/'));

    fs::write(dir.path().join("flow_dir/b.txt"), "y\n").unwrap();
    let app = App::open(dir.path()).unwrap();
    assert_eq!(app.row_count(Pane::Files), 3, "flow_dir, a.txt, b.txt");
}

/// A chain of single-child directories folds into one `x/y/z` row, and Space
/// on it stages the file underneath.
#[test]
fn a_single_child_chain_folds_into_one_row_and_space_stages_it() {
    let (dir, repo) = changed_repo("app-tree-chain", &["x/y/z/c.txt"]);
    let mut app = App::open(dir.path()).unwrap();
    app.feed_key(char_key('2'));
    assert_eq!(app.row_count(Pane::Files), 2, "x/y/z, c.txt");
    assert!(file_labels(&app)[0].contains("x/y/z"));
    assert!(app.files_selection_is_dir());

    app.feed_key(char_key(' '));
    let staged = repo.statuses(None).unwrap();
    assert!(
        staged
            .iter()
            .any(|e| e.path().ok() == Some("x/y/z/c.txt") && e.status().is_index_new()),
        "Space on the folded row stages the file under it"
    );
}

/// A root with a file and a folded chain keeps the root row.
#[test]
fn a_root_with_two_children_keeps_its_row_and_folds_the_chain() {
    let (dir, _repo) = changed_repo("app-tree-root-chain", &["TOP.txt", "x/y/z/c.txt"]);
    let mut app = App::open(dir.path()).unwrap();
    app.feed_key(char_key('2'));
    assert_eq!(app.row_count(Pane::Files), 4, "/, TOP.txt, x/y/z, c.txt");
    let labels = file_labels(&app);
    assert!(labels[0].contains('/') && !labels[0].contains("x/"));
    assert!(labels[2].contains("x/y/z"));
}
