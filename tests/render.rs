#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::pathbuf_init_then_push,
    clippy::iter_on_single_items,
    reason = "integration test: a failed setup or a bad slice is the assertion"
)]
//! Mechanism 1 from `docs/PLAN_SELF_TESTING.md`: render `ui::draw` into a
//! `TestBackend` and assert on frame text. No terminal, no timing.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc;

use ferrit::app::events::RemoteOp;
use ferrit::app::{App, Pane};
use ferrit::app::{mock, screens as ui};
use git2::{IndexAddOption, Repository, Signature};
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
        "Infos",
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

#[test]
fn one_sided_file_diff_uses_one_full_width_panel() {
    let dir = TempDir::new("render-one-sided-file-diff");
    let repo = Repository::init(dir.path()).unwrap();
    for (key, value) in [("user.name", "Test"), ("user.email", "test@example.com")] {
        let mut config = repo.config().unwrap();
        config.set_str(key, value).unwrap();
    }
    fs::write(dir.path().join("a.txt"), "before\n").unwrap();
    let mut index = repo.index().unwrap();
    index
        .add_all(["*"].iter(), IndexAddOption::DEFAULT, None)
        .unwrap();
    index.write().unwrap();
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
    let sig = Signature::now("Test", "test@example.com").unwrap();
    repo.commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
        .unwrap();
    fs::write(dir.path().join("a.txt"), "after\n").unwrap();

    let mut app = App::open(dir.path()).unwrap();
    app.select(Pane::Files, 0);
    let out = frame(&mut app, 120, 40);

    assert!(
        out.contains("Unstaged Changes"),
        "shows the worktree diff:\n{out}"
    );
    assert!(out.contains("after"), "renders changed content:\n{out}");
    assert!(
        !out.contains("Staged Changes"),
        "does not waste half the screen on an empty staged diff:\n{out}"
    );
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

    // `app::screens::draw` reserves the bottom of the screen for the
    // command log (4 rows) then the keybar (1 row): at height 40 that is rows
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

/// `docs/PLAN_9_REMOTE.md` S3: `Ctrl-Right` switches the Branches pane to
/// its Remotes tab, a plain list of `name  fetch: <url>` rows (no
/// selection bar — this tab has no cursor of its own).
#[test]
fn remotes_tab_renders_name_and_urls() {
    let mut app = App::mock();
    app.select(Pane::Branches, 0);
    app.feed_key(KeyEvent::new(KeyCode::Right, KeyModifiers::CONTROL));

    let out = frame(&mut app, 120, 40);
    assert!(out.contains("origin"), "remote name shows:\n{out}");
    assert!(out.contains("fetch:"), "fetch label shows:\n{out}");
    assert!(out.contains("git@github.com"), "fetch url shows:\n{out}");

    // Ctrl-Left switches back.
    app.feed_key(KeyEvent::new(KeyCode::Left, KeyModifiers::CONTROL));
    assert!(
        !frame(&mut app, 120, 40).contains("fetch:"),
        "back to the Local branches list"
    );
}

/// `docs/PLAN_9_REMOTE.md` S3: a background op's busy label and a
/// completed op's success line both render as an extra Status-pane line,
/// under the main `ferrit -> branch` one, and are mutually exclusive.
#[test]
fn busy_and_status_note_render_on_the_status_pane_and_are_exclusive() {
    let dir = TempDir::new("render-remote-status");
    let repo = Repository::init(dir.path()).unwrap();
    for (key, value) in [("user.name", "Test"), ("user.email", "test@example.com")] {
        let mut config = repo.config().unwrap();
        config.set_str(key, value).unwrap();
    }
    fs::write(dir.path().join("a.txt"), "one\n").unwrap();
    let mut index = repo.index().unwrap();
    index
        .add_all(["*"].iter(), IndexAddOption::DEFAULT, None)
        .unwrap();
    index.write().unwrap();
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
    let sig = Signature::now("Test", "test@example.com").unwrap();
    repo.commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
        .unwrap();

    let mut app = App::open(dir.path()).unwrap();
    let (tx, _rx) = mpsc::channel();
    app.start_remote_op(RemoteOp::Fetch, None, tx);

    let busy_out = frame(&mut app, 120, 40);
    assert!(
        busy_out.contains("Fetching"),
        "busy label shows:\n{busy_out}"
    );
    assert!(
        !busy_out.contains("Fetched origin"),
        "no success line while busy:\n{busy_out}"
    );
    assert!(
        busy_out.lines().any(|line| {
            line.contains("Fetching")
                && ["●∙∙", "∙●∙", "∙∙●"]
                    .iter()
                    .any(|spinner| line.contains(spinner))
        }),
        "checked-out branch renders LazyGit-style activity spinner:\n{busy_out}"
    );

    app.on_remote_done(RemoteOp::Fetch, Ok("Fetched origin".to_owned()));
    let done_out = frame(&mut app, 120, 40);
    assert!(
        done_out.contains("Fetched origin"),
        "success line shows:\n{done_out}"
    );
    assert!(
        !done_out.contains("Fetching"),
        "busy label cleared:\n{done_out}"
    );
}

/// `docs/PLAN_10_STASH.md` S4: the keybar swaps for the Stash pane, same as
/// Branches.
#[test]
fn keybar_swaps_for_the_stash_pane() {
    let mut app = App::mock();
    app.focus = Pane::Stash;
    let out = frame(&mut app, 120, 40);
    assert!(out.contains("Apply:"), "{out}");
    assert!(out.contains("Drop:"), "{out}");
    assert!(!out.contains("Stage:"), "{out}");
}

/// `docs/PLAN_10_STASH.md` S4: the stash popup reuses the single-input
/// popup shape, with its own title and hints.
#[test]
fn stash_popup_renders_title_and_hints() {
    let mut app = App::mock();
    app.focus = Pane::Files;
    app.feed_key(KeyEvent::from(KeyCode::Char('s')));

    let out = frame(&mut app, 120, 40);
    assert!(out.contains("Stash changes"), "popup title shows:\n{out}");
    assert!(out.contains("Stash: Enter"), "hints show:\n{out}");
    assert!(out.contains("Cancel: Esc"), "hints show:\n{out}");
}
