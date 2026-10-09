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
//! `App`-level wiring for the Branches pane's five actions
//! (`docs/PLAN_8_BRANCHES.md`): `<space>` checks out, `n` creates from a
//! popup, `d` deletes (two-step when unmerged), `u` fast-forwards, `M`
//! opens the Merge menu.

mod common;

use common::{TempDir, commit_all, configure_identity, git};
use std::fs;
use std::path::Path;

use ferrit::app::App;
use ferrit::interface::panes::pane::Pane;
use git2::Repository;
use ratatui::crossterm::event::{KeyCode, KeyEvent};

fn char_key(c: char) -> KeyEvent {
    KeyEvent::from(KeyCode::Char(c))
}

fn type_text(app: &mut App, text: &str) {
    for c in text.chars() {
        app.feed_key(char_key(c));
    }
}

/// `base` (renamed off whatever `git2::Repository::init` defaulted to, see
/// `tests/git_branch.rs`) at one commit, `feat` branched off it with its
/// own commit, `base` checked out. Focuses the Branches pane before
/// returning, `base` selected first (`refs.rs` sorts `HEAD` first).
fn two_branch_app(tag: &str) -> (TempDir, App) {
    let dir = TempDir::new(tag);
    let repo = Repository::init(dir.path()).unwrap();
    configure_identity(dir.path());
    fs::write(dir.path().join("a.txt"), "one\n").unwrap();
    commit_all(&repo, "init");
    git(dir.path(), &["branch", "-m", "base"]);
    git(dir.path(), &["checkout", "-q", "-b", "feat"]);
    fs::write(dir.path().join("a.txt"), "one\nfeat-change\n").unwrap();
    commit_all(&repo, "feat work");
    git(dir.path(), &["checkout", "-q", "base"]);

    let mut app = App::open(dir.path()).unwrap();
    app.feed_key(char_key('3')); // focus Branches
    (dir, app)
}

fn select_branch(app: &mut App, name: &str) {
    let idx = (0..app.row_count(Pane::Branches))
        .find(|&i| app.branch_lines()[i].to_string().contains(name))
        .unwrap_or_else(|| panic!("branch {name} not in the pane"));
    app.select(Pane::Branches, idx);
}

#[test]
fn space_checks_out_the_selected_branch() {
    let (dir, mut app) = two_branch_app("app-branch-checkout");
    select_branch(&mut app, "feat");
    app.feed_key(char_key(' '));

    assert_eq!(
        git(dir.path(), &["symbolic-ref", "--short", "HEAD"]),
        "feat"
    );
    assert!(
        app.status_lines()
            .iter()
            .any(|l| l.to_string().contains("feat")),
        "the Status pane reflects the new HEAD"
    );
}

#[test]
fn n_opens_types_and_enter_creates_and_switches() {
    let (dir, mut app) = two_branch_app("app-branch-create");
    app.feed_key(char_key('n'));
    assert!(app.new_branch_popup().is_some(), "n opened the popup");
    type_text(&mut app, "wip/replay");
    app.feed_key(KeyEvent::from(KeyCode::Enter));

    assert!(app.new_branch_popup().is_none(), "popup closed on success");
    assert!(app.note_popup().is_none());
    assert_eq!(
        git(dir.path(), &["symbolic-ref", "--short", "HEAD"]),
        "wip/replay"
    );
}

#[test]
fn n_starts_the_new_branch_from_the_selected_branch() {
    let (dir, mut app) = two_branch_app("app-branch-create-from-selected");
    select_branch(&mut app, "feat"); // base is checked out, feat is one commit ahead
    app.feed_key(char_key('n'));
    assert!(
        app.new_branch_popup()
            .unwrap()
            .title
            .contains("off of 'feat'"),
        "the prompt names the selected branch"
    );
    type_text(&mut app, "wip/from-feat");
    app.feed_key(KeyEvent::from(KeyCode::Enter));

    assert_eq!(
        git(dir.path(), &["symbolic-ref", "--short", "HEAD"]),
        "wip/from-feat"
    );
    assert_eq!(
        git(dir.path(), &["rev-parse", "HEAD"]),
        git(dir.path(), &["rev-parse", "feat"]),
        "the new branch starts at feat's commit"
    );
    assert_ne!(
        git(dir.path(), &["rev-parse", "HEAD"]),
        git(dir.path(), &["rev-parse", "base"])
    );
}

#[test]
fn n_with_the_current_branch_selected_starts_from_head() {
    let (dir, mut app) = two_branch_app("app-branch-create-from-current");
    select_branch(&mut app, "base");
    app.feed_key(char_key('n'));
    assert!(
        app.new_branch_popup()
            .unwrap()
            .title
            .contains("off of 'base'")
    );
    type_text(&mut app, "wip/from-base");
    app.feed_key(KeyEvent::from(KeyCode::Enter));

    assert_eq!(
        git(dir.path(), &["rev-parse", "HEAD"]),
        git(dir.path(), &["rev-parse", "base"])
    );
}

#[test]
fn the_new_branch_is_the_selected_row_whichever_row_the_cursor_was_on() {
    let (_dir, mut app) = two_branch_app("app-branch-create-selects");
    select_branch(&mut app, "feat"); // the cursor is elsewhere when `n` is pressed
    app.feed_key(char_key('n'));
    type_text(&mut app, "wip/replay");
    app.feed_key(KeyEvent::from(KeyCode::Enter));

    let selected = app.selected(Pane::Branches);
    assert!(
        app.branch_lines()[selected]
            .to_string()
            .contains("wip/replay"),
        "the selection follows the branch just created, row {selected}"
    );
}

#[test]
fn n_with_a_name_already_taken_keeps_the_popup_open_with_the_typed_text() {
    let (dir, mut app) = two_branch_app("app-branch-create-taken");
    let head_before = git(dir.path(), &["symbolic-ref", "--short", "HEAD"]);

    app.feed_key(char_key('n'));
    type_text(&mut app, "feat"); // already exists
    app.feed_key(KeyEvent::from(KeyCode::Enter));

    let view = app
        .new_branch_popup()
        .expect("the popup stays open so the name can be fixed and retried");
    assert_eq!(view.lines.join("\n"), "feat", "typed text kept");
    assert_eq!(
        git(dir.path(), &["symbolic-ref", "--short", "HEAD"]),
        head_before,
        "no branch was created or checked out"
    );
}

#[test]
fn esc_cancels_the_new_branch_popup_with_no_draft_kept() {
    let (_dir, mut app) = two_branch_app("app-branch-cancel");
    app.feed_key(char_key('n'));
    type_text(&mut app, "wip-name");
    app.feed_key(KeyEvent::from(KeyCode::Esc));
    assert!(app.new_branch_popup().is_none());

    app.feed_key(char_key('n'));
    let view = app.new_branch_popup().expect("n reopened the popup");
    assert_eq!(view.lines.join("\n"), "", "no draft carried over");
}

#[test]
fn d_on_the_checked_out_branch_shows_last_error_with_no_confirm() {
    let (dir, mut app) = two_branch_app("app-branch-delete-head");
    select_branch(&mut app, "base");
    app.feed_key(char_key('d'));

    assert!(app.confirm_message().is_none(), "no confirm was opened");
    assert!(
        git(dir.path(), &["branch", "--list"]).contains("base"),
        "base is untouched"
    );
}

#[test]
fn d_on_an_unmerged_branch_asks_twice_then_force_deletes() {
    let (dir, mut app) = two_branch_app("app-branch-delete-unmerged");
    select_branch(&mut app, "feat");

    app.feed_key(char_key('d'));
    let first = app
        .confirm_message()
        .expect("first confirm is up")
        .to_owned();
    assert!(first.contains("feat"));

    app.feed_key(char_key('y'));
    let second = app
        .confirm_message()
        .expect("refused as unmerged, second confirm is up")
        .to_owned();
    assert!(second.contains("not fully merged"), "got: {second}");

    app.feed_key(char_key('y'));
    assert!(app.confirm_message().is_none());
    assert!(!git(dir.path(), &["branch", "--list"]).contains("feat"));
}

#[test]
fn enter_confirms_a_branch_delete_without_opening_the_commits() {
    let (dir, mut app) = two_branch_app("app-branch-delete-enter");
    git(dir.path(), &["branch", "already-merged"]);
    app.refresh();
    select_branch(&mut app, "already-merged");

    app.feed_key(char_key('d'));
    app.feed_key(KeyEvent::from(KeyCode::Enter));
    assert!(app.confirm_message().is_none());
    assert!(!git(dir.path(), &["branch", "--list"]).contains("already-merged"));
    assert!(!app.branches_drilled(), "Enter did not drill");
}

#[test]
fn n_cancels_the_confirm_on_a_merged_branch() {
    let (dir, mut app) = two_branch_app("app-branch-delete-merged");
    git(dir.path(), &["branch", "already-merged"]);
    app.refresh();
    select_branch(&mut app, "already-merged");

    app.feed_key(char_key('d'));
    assert!(app.confirm_message().is_some());
    app.feed_key(char_key('n'));
    assert!(app.confirm_message().is_none());
    assert!(git(dir.path(), &["branch", "--list"]).contains("already-merged"));

    app.feed_key(char_key('d'));
    app.feed_key(char_key('y'));
    assert!(app.confirm_message().is_none(), "merged, no second prompt");
    assert!(!git(dir.path(), &["branch", "--list"]).contains("already-merged"));
}

#[test]
fn u_fast_forwards_a_branch_that_is_not_checked_out() {
    let origin = TempDir::new("app-branch-ff-origin");
    let origin_repo = Repository::init(origin.path()).unwrap();
    configure_identity(origin.path());
    fs::write(origin.path().join("a.txt"), "one\n").unwrap();
    commit_all(&origin_repo, "init");
    git(origin.path(), &["branch", "-m", "base"]);

    let work = TempDir::new("app-branch-ff-work");
    git(
        Path::new("."),
        &[
            "clone",
            "-q",
            origin.path().to_str().unwrap(),
            work.path().to_str().unwrap(),
        ],
    );
    configure_identity(work.path());
    git(work.path(), &["checkout", "-q", "-b", "feat"]);
    git(
        work.path(),
        &["branch", "--set-upstream-to=origin/base", "feat"],
    );
    git(work.path(), &["checkout", "-q", "base"]);

    fs::write(origin.path().join("a.txt"), "one\ntwo\n").unwrap();
    commit_all(&origin_repo, "origin advances");
    git(work.path(), &["fetch", "-q", "origin"]);
    let upstream_tip = git(work.path(), &["rev-parse", "origin/base"]);

    let mut app = App::open(work.path()).unwrap();
    app.feed_key(char_key('3'));
    select_branch(&mut app, "feat");
    app.feed_key(char_key('u'));

    assert_eq!(git(work.path(), &["rev-parse", "feat"]), upstream_tip);
    assert_eq!(
        git(work.path(), &["symbolic-ref", "--short", "HEAD"]),
        "base",
        "u did not move HEAD off base"
    );
}

fn head_and_status(dir: &Path) -> (String, String) {
    (
        git(dir, &["rev-parse", "HEAD"]),
        git(dir, &["status", "--porcelain"]),
    )
}

fn parent_count(dir: &Path) -> usize {
    git(dir, &["log", "-1", "--format=%P"])
        .split_whitespace()
        .count()
}

#[test]
fn m_opens_the_merge_menu_and_merges_nothing() {
    let (dir, mut app) = two_branch_app("app-branch-merge-menu");
    let before = head_and_status(dir.path());
    select_branch(&mut app, "feat");
    app.feed_key(char_key('M'));

    let menu = app.menu_popup().expect("M opens a menu");
    assert_eq!(menu.title, "Merge");
    assert_eq!(menu.rows.len(), 4);
    assert!(menu.hint.contains("Fast-forward"), "{}", menu.hint);
    assert_eq!(head_and_status(dir.path()), before, "nothing merged yet");
    assert_eq!(
        fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "one\n"
    );
}

#[test]
fn esc_on_the_merge_menu_leaves_the_repo_untouched() {
    let (dir, mut app) = two_branch_app("app-branch-merge-esc");
    let before = head_and_status(dir.path());
    select_branch(&mut app, "feat");
    app.feed_key(char_key('M'));
    app.feed_key(KeyEvent::from(KeyCode::Esc));

    assert!(app.menu_popup().is_none());
    assert_eq!(head_and_status(dir.path()), before);
}

#[test]
fn the_fast_forward_choice_merges_without_a_merge_commit() {
    let (dir, mut app) = two_branch_app("app-branch-merge-ff");
    select_branch(&mut app, "feat");
    app.feed_key(char_key('M'));
    app.feed_key(KeyEvent::from(KeyCode::Enter));

    assert_eq!(
        fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "one\nfeat-change\n",
        "feat's change landed on base"
    );
    assert_eq!(parent_count(dir.path()), 1, "fast-forward, no merge commit");
    assert_eq!(
        git(dir.path(), &["rev-parse", "HEAD"]),
        git(dir.path(), &["rev-parse", "feat"])
    );
    assert!(app.note_popup().is_none(), "a clean merge needs no note");
}

#[test]
fn the_no_ff_choice_makes_a_merge_commit() {
    let (dir, mut app) = two_branch_app("app-branch-merge-noff");
    select_branch(&mut app, "feat");
    app.feed_key(char_key('M'));
    app.feed_key(char_key('n'));

    assert_eq!(parent_count(dir.path()), 2);
}

#[test]
fn the_squash_choice_stages_the_changes_without_committing() {
    let (dir, mut app) = two_branch_app("app-branch-merge-squash");
    let head = git(dir.path(), &["rev-parse", "HEAD"]);
    select_branch(&mut app, "feat");
    app.feed_key(char_key('M'));
    app.feed_key(char_key('s'));

    assert_eq!(git(dir.path(), &["rev-parse", "HEAD"]), head, "HEAD stays");
    assert_eq!(git(dir.path(), &["status", "--porcelain"]), "M  a.txt");
}

#[test]
fn the_squash_and_commit_choice_makes_one_ordinary_commit() {
    let (dir, mut app) = two_branch_app("app-branch-merge-squash-commit");
    select_branch(&mut app, "feat");
    app.feed_key(char_key('M'));
    app.feed_key(char_key('c'));

    assert_eq!(parent_count(dir.path()), 1, "not a merge commit");
    assert_eq!(git(dir.path(), &["status", "--porcelain"]), "");
    assert_eq!(
        fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "one\nfeat-change\n"
    );
    assert_ne!(
        git(dir.path(), &["rev-parse", "HEAD"]),
        git(dir.path(), &["rev-parse", "feat"])
    );
}

#[test]
fn m_on_a_conflicting_merge_shows_a_dismissible_note() {
    let dir = TempDir::new("app-branch-merge-conflict");
    let repo = Repository::init(dir.path()).unwrap();
    configure_identity(dir.path());
    fs::write(dir.path().join("a.txt"), "one\n").unwrap();
    commit_all(&repo, "init");
    git(dir.path(), &["checkout", "-q", "-b", "feat"]);
    fs::write(dir.path().join("a.txt"), "one\nfeat-change\n").unwrap();
    commit_all(&repo, "feat edits a.txt");
    git(dir.path(), &["checkout", "-q", "-"]);
    fs::write(dir.path().join("a.txt"), "one\nbase-change\n").unwrap();
    commit_all(&repo, "base edits a.txt too");

    let mut app = App::open(dir.path()).unwrap();
    app.feed_key(char_key('3'));
    select_branch(&mut app, "feat");
    app.feed_key(char_key('M'));
    app.feed_key(KeyEvent::from(KeyCode::Enter));

    let note = app.note_popup().expect("a conflict shows a note");
    assert!(note.contains("a.txt"), "names the conflicted file: {note}");
    assert!(note.contains("press m"), "{note}");
}
