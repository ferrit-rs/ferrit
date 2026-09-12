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
//! `App`-level wiring for the right-pane diff: selecting a file builds a real
//! `DiffView`, a background `refresh()` of the same selection rebuilds the text
//! but keeps the scroll, and moving to another file resets the scroll to the
//! top. See `docs/PLAN_3_DIFF_VIEW.md` milestone D3.

use std::fs;
use std::path::{Path, PathBuf};

use ferrit::app::{App, DiffView, Pane};
use git2::{IndexAddOption, Repository, Signature};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

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

fn diff_text(app: &App) -> String {
    match app.diff_view() {
        DiffView::Files(d) | DiffView::Commit(_, d) => d.text.clone(),
        other => panic!("expected a real diff, got {other:?}"),
    }
}

/// Index of the Files row whose path ends with `name`.
fn files_row(app: &App, name: &str) -> usize {
    (0..app.row_count(Pane::Files))
        .find(|&i| app.file_display(i).ends_with(name))
        .unwrap_or_else(|| panic!("no Files row for {name}"))
}

#[test]
fn refresh_keeps_the_scroll_for_an_unchanged_selection() {
    let dir = TempDir::new("app-diff-scroll");
    let repo = Repository::init(dir.path()).unwrap();
    let long: String = (0..60).map(|n| format!("line {n}\n")).collect();
    fs::write(dir.path().join("big.txt"), &long).unwrap();
    fs::write(dir.path().join("other.txt"), "x\n").unwrap();
    commit_all(&repo, "init");

    // A change near the bottom so there is something to scroll to.
    let edited = long.replace("line 55\n", "line 55 CHANGED\n");
    fs::write(dir.path().join("big.txt"), &edited).unwrap();

    let mut app = App::open(dir.path()).unwrap();
    app.select(Pane::Files, files_row(&app, "big.txt"));
    let before = diff_text(&app);
    assert!(before.contains("+line 55 CHANGED"));

    app.set_right_scroll(12);
    assert_eq!(app.right_scroll(), 12);

    // A second edit lands from "another shell"; the event loop calls refresh().
    let edited2 = edited.replace("line 10\n", "line 10 ALSO\n");
    fs::write(dir.path().join("big.txt"), &edited2).unwrap();
    app.refresh();

    assert!(
        diff_text(&app).contains("+line 10 ALSO"),
        "diff text rebuilt"
    );
    assert_eq!(app.right_scroll(), 12, "scroll survives a refresh");
}

/// A repo with a tall two-hunk diff on `a_tall.txt` (row 0) and a second
/// changed file `z_other.txt` (row 1), Files focused on row 0.
fn tall_repo(tag: &str) -> (TempDir, App) {
    let dir = TempDir::new(tag);
    let repo = Repository::init(dir.path()).unwrap();
    let base: String = (0..200).map(|n| format!("line {n}\n")).collect();
    fs::write(dir.path().join("a_tall.txt"), &base).unwrap();
    fs::write(dir.path().join("z_other.txt"), "keep\n").unwrap();
    commit_all(&repo, "init");

    let edited = base
        .replace("line 5\n", "line 5 CHANGED\n")
        .replace("line 60\n", "line 60 CHANGED\n")
        .replace("line 120\n", "line 120 CHANGED\n")
        .replace("line 180\n", "line 180 CHANGED\n");
    fs::write(dir.path().join("a_tall.txt"), &edited).unwrap();
    fs::write(dir.path().join("z_other.txt"), "keep\nmore\n").unwrap();

    let mut app = App::open(dir.path()).unwrap();
    app.select(Pane::Files, files_row(&app, "a_tall.txt"));
    (dir, app)
}

fn char_key(c: char) -> KeyEvent {
    KeyEvent::from(KeyCode::Char(c))
}

fn render(app: &mut App, w: u16, h: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(w, h)).unwrap();
    terminal.draw(|f| ferrit::ui::draw(f, app)).unwrap();
    terminal.backend().to_string()
}

#[test]
fn shift_j_k_scroll_the_diff_within_the_viewport() {
    let (_dir, mut app) = tall_repo("app-diff-jk");
    app.set_right_viewport(10);
    let count = diff_text(&app).lines().count();
    let max = count.saturating_sub(10);
    assert!(
        max > 3,
        "fixture diff should overflow a 10-row pane, got {count} lines"
    );

    for _ in 0..3 {
        app.feed_key(char_key('J'));
    }
    assert_eq!(app.right_scroll(), 3);

    for _ in 0..500 {
        app.feed_key(char_key('J'));
    }
    assert_eq!(
        app.right_scroll(),
        max,
        "stops with the last line at the bottom"
    );

    for _ in 0..500 {
        app.feed_key(char_key('K'));
    }
    assert_eq!(app.right_scroll(), 0);
}

#[test]
fn ctrl_d_u_and_angle_brackets_reach_the_ends() {
    let (_dir, mut app) = tall_repo("app-diff-ends");
    app.set_right_viewport(8);
    let max = diff_text(&app).lines().count().saturating_sub(8);

    app.feed_key(KeyEvent::new(KeyCode::Char('d'), KeyModifiers::CONTROL));
    assert_eq!(app.right_scroll(), 4, "Ctrl-d is half of an 8-row pane");

    app.feed_key(char_key('>'));
    assert_eq!(app.right_scroll(), max);
    app.feed_key(char_key('<'));
    assert_eq!(app.right_scroll(), 0);
}

#[test]
fn lowercase_j_still_moves_the_left_selection() {
    let (_dir, mut app) = tall_repo("app-diff-lowj");
    app.set_right_viewport(8);
    app.select(Pane::Files, 0);
    let scroll = app.right_scroll();

    app.feed_key(char_key('j'));
    assert_eq!(app.right_scroll(), scroll, "the diff did not scroll");
    assert_eq!(app.selected(Pane::Files), 1, "the left selection moved");
}

#[test]
fn mouse_wheel_routes_by_column() {
    let (_dir, mut app) = tall_repo("app-diff-wheel");
    app.set_right_viewport(10);
    app.set_right_area(Rect {
        x: 40,
        y: 0,
        width: 80,
        height: 12,
    });
    app.select(Pane::Files, 0);

    let wheel = |column: u16| MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column,
        row: 3,
        modifiers: KeyModifiers::NONE,
    };

    app.feed_mouse(wheel(60));
    assert_eq!(
        app.right_scroll(),
        3,
        "wheel over the diff scrolls it three lines"
    );

    app.feed_mouse(wheel(5));
    assert_eq!(
        app.selected(Pane::Files),
        1,
        "wheel over the list moves the selection"
    );
    assert_eq!(
        app.right_scroll(),
        0,
        "the new file's diff starts at the top"
    );
}

#[test]
fn scroll_keys_are_inert_without_a_real_diff() {
    let dir = TempDir::new("app-diff-inert");
    let repo = Repository::init(dir.path()).unwrap();
    fs::write(dir.path().join("x.txt"), "x\n").unwrap();
    commit_all(&repo, "init"); // clean tree: nothing in Files, no diff

    let mut app = App::open(dir.path()).unwrap();
    app.set_right_viewport(10);
    for c in ['J', 'K', '<', '>', ']', '['] {
        app.feed_key(char_key(c));
    }
    assert_eq!(app.right_scroll(), 0, "no diff, no scroll, no panic");
}

#[test]
fn a_tall_diff_gets_a_scrollbar() {
    let (_dir, mut app) = tall_repo("app-diff-sb-yes");
    let out = render(&mut app, 100, 12);
    assert!(
        out.contains('\u{2588}') || out.contains('\u{25bc}'),
        "a diff taller than the pane draws a scrollbar:\n{out}"
    );
}

#[test]
fn a_short_diff_has_no_scrollbar() {
    let dir = TempDir::new("app-diff-sb-no");
    let repo = Repository::init(dir.path()).unwrap();
    fs::write(dir.path().join("s.txt"), "a\nb\nc\n").unwrap();
    commit_all(&repo, "init");
    fs::write(dir.path().join("s.txt"), "a\nB\nc\n").unwrap();

    let mut app = App::open(dir.path()).unwrap();
    app.select(Pane::Files, files_row(&app, "s.txt"));
    let out = render(&mut app, 100, 40);
    assert!(
        !out.contains('\u{2588}') && !out.contains('\u{25bc}'),
        "a diff that fits draws no scrollbar:\n{out}"
    );
}

#[test]
fn moving_to_another_file_resets_the_scroll() {
    let dir = TempDir::new("app-diff-reset");
    let repo = Repository::init(dir.path()).unwrap();
    fs::write(dir.path().join("a.txt"), "aaa\n").unwrap();
    fs::write(dir.path().join("b.txt"), "bbb\n").unwrap();
    commit_all(&repo, "init");
    fs::write(dir.path().join("a.txt"), "aaa changed\n").unwrap();
    fs::write(dir.path().join("b.txt"), "bbb changed\n").unwrap();

    let mut app = App::open(dir.path()).unwrap();
    app.select(Pane::Files, files_row(&app, "a.txt"));
    app.set_right_scroll(3);

    app.select(Pane::Files, files_row(&app, "b.txt"));
    assert_eq!(app.right_scroll(), 0, "new selection starts at the top");
    assert!(diff_text(&app).contains("+bbb changed"));
}

/// Scrolling the right pane while it passively previews a branch's log (G7)
/// must move that log, not the Branches selection — a regression once
/// caused by `right_is_diff` excluding `DiffView::BranchLog`, which let J/K
/// and the mouse wheel leak through to `select_down`/`select_up` instead.
#[test]
fn branch_log_preview_scrolls_instead_of_moving_the_branch_selection() {
    let dir = TempDir::new("app-branch-log-scroll");
    let repo = Repository::init(dir.path()).unwrap();
    for i in 0..15 {
        fs::write(dir.path().join("f.txt"), format!("{i}\n")).unwrap();
        commit_all(&repo, &format!("commit {i}"));
    }

    let mut app = App::open(dir.path()).unwrap();
    app.select(Pane::Branches, 0);
    app.set_right_viewport(5);
    assert_eq!(app.row_count(Pane::Branches), 1, "only one branch exists");

    app.feed_key(KeyEvent::from(KeyCode::Char('J')));
    app.feed_key(KeyEvent::from(KeyCode::Char('J')));

    assert_eq!(
        app.right_scroll(),
        2,
        "uppercase J scrolled the log preview"
    );
    assert_eq!(
        app.selected(Pane::Branches),
        0,
        "the (only) branch is still selected, not moved by J"
    );

    app.set_right_area(Rect {
        x: 40,
        y: 0,
        width: 80,
        height: 5,
    });
    app.feed_mouse(MouseEvent {
        kind: MouseEventKind::ScrollDown,
        column: 60,
        row: 3,
        modifiers: KeyModifiers::NONE,
    });

    assert!(
        app.right_scroll() > 2,
        "the wheel over the right pane scrolled the log preview too"
    );
    assert_eq!(
        app.selected(Pane::Branches),
        0,
        "still the only branch selected, not moved by the wheel"
    );
}

/// Just focusing/selecting in the Branches pane, no Enter (lazygit's live
/// branch -> log preview, `docs/PLAN_2_GIT_BACKEND.md` G7): the right pane
/// already shows the selected branch's own log, and the branch list itself
/// is untouched (that's the Enter drill-down's job, tested separately).
#[test]
fn branches_passively_previews_the_selected_branchs_log() {
    let dir = TempDir::new("app-branch-preview");
    let repo = Repository::init(dir.path()).unwrap();
    fs::write(dir.path().join("f.txt"), "0\n").unwrap();
    commit_all(&repo, "on the branch point");
    let base_oid = repo.head().unwrap().target().unwrap();
    let base_commit = repo.find_commit(base_oid).unwrap();
    repo.branch("zzz-feature", &base_commit, false).unwrap();

    fs::write(dir.path().join("f.txt"), "1\n").unwrap();
    commit_all(&repo, "only on head");

    let mut app = App::open(dir.path()).unwrap();
    let branch_count = app.row_count(Pane::Branches);
    assert_eq!(branch_count, 2);

    // HEAD sorts first, "zzz-feature" is the only other branch: index 1.
    app.select(Pane::Branches, 1);

    match app.diff_view() {
        DiffView::BranchLog(log) => {
            assert_eq!(log.branch, "zzz-feature");
            assert_eq!(log.commits.len(), 1, "the branch's own log, not HEAD's");
        },
        other => panic!("expected a passive branch log preview, got {other:?}"),
    }
    assert_eq!(
        app.row_count(Pane::Branches),
        branch_count,
        "no Enter: the branch list itself is untouched"
    );
}

/// Enter on a branch in the Branches pane (lazygit's branch -> log
/// drill-down, `docs/PLAN_2_GIT_BACKEND.md` G7): swaps that pane's own
/// branch list for the branch's commit list, in place — focus never leaves
/// Branches, and the separate Commits pane is untouched throughout.
/// Selecting a commit in the drilled list shows its diff; `Esc` backs out.
#[test]
fn enter_on_branches_drills_into_that_branchs_log() {
    let dir = TempDir::new("app-branch-log");
    let repo = Repository::init(dir.path()).unwrap();
    fs::write(dir.path().join("f.txt"), "0\n").unwrap();
    commit_all(&repo, "on the branch point");
    let base_oid = repo.head().unwrap().target().unwrap();
    let base_commit = repo.find_commit(base_oid).unwrap();
    repo.branch("zzz-feature", &base_commit, false).unwrap();

    fs::write(dir.path().join("f.txt"), "1\n").unwrap();
    commit_all(&repo, "only on head");

    let mut app = App::open(dir.path()).unwrap();
    let head_commit_count = app.row_count(Pane::Commits);
    assert_eq!(head_commit_count, 2);

    // HEAD sorts first, "zzz-feature" is the only other branch: index 1.
    app.select(Pane::Branches, 1);
    app.feed_key(KeyEvent::from(KeyCode::Enter));

    assert_eq!(app.focus, Pane::Branches, "Enter stays in the same panel");
    assert_eq!(
        app.row_count(Pane::Branches),
        1,
        "the branch's own log, not HEAD's, replaces the branch list"
    );
    assert!(app.branches_title().contains("zzz-feature"));
    assert_eq!(
        app.row_count(Pane::Commits),
        head_commit_count,
        "the separate Commits pane is untouched by the drill-down"
    );
    assert!(
        matches!(app.diff_view(), DiffView::Commit(..)),
        "selecting the (only) row in the drilled log shows its diff"
    );

    app.feed_key(KeyEvent::from(KeyCode::Esc));
    assert_eq!(app.focus, Pane::Branches);
    assert_eq!(
        app.selected(Pane::Branches),
        1,
        "Esc restores the branch-list cursor"
    );
    assert_eq!(app.branches_title(), "[3] Local branches - Remotes - Tags");
}
