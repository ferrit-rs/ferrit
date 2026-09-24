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
//! `App`-level wiring for the stash actions (`docs/PLAN_10_STASH.md`):
//! `s` on Files opens a message popup, `<space>` / `g` / `d` on Stash apply,
//! pop and drop, and the right pane previews the selected entry.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use ferrit::app::{App, DiffView, Pane};
use git2::{IndexAddOption, Repository, Signature};
use ratatui::crossterm::event::{KeyCode, KeyEvent};

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

fn configure_identity(dir: &Path) {
    for (key, value) in [("user.name", "Test"), ("user.email", "test@example.com")] {
        let out = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["config", key, value])
            .output()
            .unwrap();
        assert!(out.status.success());
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

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

fn char_key(c: char) -> KeyEvent {
    KeyEvent::from(KeyCode::Char(c))
}

fn type_text(app: &mut App, text: &str) {
    for c in text.chars() {
        app.feed_key(char_key(c));
    }
}

/// One commit with `a.txt`, plus a tracked edit and an untracked file.
/// Files focused when returned.
fn dirty_app(tag: &str) -> (TempDir, App) {
    let dir = TempDir::new(tag);
    let repo = Repository::init(dir.path()).unwrap();
    configure_identity(dir.path());
    fs::write(dir.path().join("a.txt"), "one\n").unwrap();
    commit_all(&repo, "init");
    fs::write(dir.path().join("a.txt"), "one\nlocal\n").unwrap();
    fs::write(dir.path().join("new.txt"), "untracked\n").unwrap();
    let mut app = App::open(dir.path()).unwrap();
    app.feed_key(char_key('2'));
    (dir, app)
}

/// `dirty_app` after one `s` + Enter, Stash focused with the entry selected.
fn stashed_app(tag: &str) -> (TempDir, App) {
    let (dir, mut app) = dirty_app(tag);
    app.feed_key(char_key('s'));
    type_text(&mut app, "parked");
    app.feed_key(KeyEvent::from(KeyCode::Enter));
    app.feed_key(char_key('5'));
    (dir, app)
}

fn status_text(app: &App) -> String {
    app.status_lines()
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn s_opens_types_and_enter_stashes_everything() {
    let (dir, mut app) = dirty_app("app-stash-push");
    app.feed_key(char_key('s'));
    assert!(app.stash_popup().is_some(), "s opened the popup");
    type_text(&mut app, "parked");
    app.feed_key(KeyEvent::from(KeyCode::Enter));

    assert!(app.stash_popup().is_none(), "popup closed on success");
    assert_eq!(git(dir.path(), &["status", "--porcelain"]), "");
    assert_eq!(app.row_count(Pane::Files), 0);
    assert!(git(dir.path(), &["stash", "list"]).contains("parked"));
    assert_eq!(app.row_count(Pane::Stash), 1);
}

#[test]
fn s_on_a_clean_tree_opens_no_popup_and_says_so() {
    let (dir, mut app) = dirty_app("app-stash-clean");
    git(dir.path(), &["stash", "push", "-u"]);
    app.refresh();

    app.feed_key(char_key('s'));
    assert!(app.stash_popup().is_none());
    assert!(
        status_text(&app).contains("no local changes"),
        "{}",
        status_text(&app)
    );
}

#[test]
fn esc_cancels_the_stash_popup_and_stashes_nothing() {
    let (dir, mut app) = dirty_app("app-stash-cancel");
    app.feed_key(char_key('s'));
    type_text(&mut app, "x");
    app.feed_key(KeyEvent::from(KeyCode::Esc));

    assert!(app.stash_popup().is_none());
    assert_eq!(git(dir.path(), &["stash", "list"]), "");
    assert!(!git(dir.path(), &["status", "--porcelain"]).is_empty());
}

#[test]
fn s_is_inert_outside_the_files_pane() {
    let (_dir, mut app) = dirty_app("app-stash-s-elsewhere");
    app.feed_key(char_key('3'));
    app.feed_key(char_key('s'));
    assert!(app.stash_popup().is_none());
}

#[test]
fn space_applies_and_keeps_the_entry() {
    let (dir, mut app) = stashed_app("app-stash-apply");
    app.feed_key(char_key(' '));

    assert!(dir.path().join("new.txt").exists());
    assert_eq!(app.row_count(Pane::Stash), 1, "apply keeps the entry");
    assert_eq!(
        fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "one\nlocal\n"
    );
}

#[test]
fn g_pops_and_removes_the_entry() {
    let (dir, mut app) = stashed_app("app-stash-pop");
    app.feed_key(char_key('g'));

    assert!(dir.path().join("new.txt").exists());
    assert_eq!(git(dir.path(), &["stash", "list"]), "");
    assert_eq!(app.row_count(Pane::Stash), 0);
}

#[test]
fn d_asks_then_y_drops_and_n_keeps() {
    let (dir, mut app) = stashed_app("app-stash-drop");
    app.feed_key(char_key('d'));
    let prompt = app.confirm_message().expect("confirm is up").to_owned();
    assert!(
        prompt.contains("stash@{0}") && prompt.contains("parked"),
        "{prompt}"
    );

    app.feed_key(char_key('n'));
    assert!(app.confirm_message().is_none());
    assert!(git(dir.path(), &["stash", "list"]).contains("parked"));

    app.feed_key(char_key('d'));
    app.feed_key(char_key('y'));
    assert_eq!(git(dir.path(), &["stash", "list"]), "");
    assert_eq!(app.row_count(Pane::Stash), 0);
}

#[test]
fn stash_keys_are_no_ops_on_an_empty_stash_pane() {
    let (_dir, mut app) = dirty_app("app-stash-empty");
    app.feed_key(char_key('5'));
    for c in [' ', 'g', 'd'] {
        app.feed_key(char_key(c));
    }
    assert!(app.confirm_message().is_none());
    assert!(app.note_popup().is_none());
}

#[test]
fn space_on_files_still_stages_and_d_still_asks_to_discard() {
    let (dir, mut app) = dirty_app("app-stash-prior");
    app.feed_key(char_key('d'));
    assert!(
        app.confirm_message().is_some_and(|m| m.contains("discard")),
        "d on Files still opens the discard confirm"
    );
    app.feed_key(char_key('n'));

    app.feed_key(char_key(' '));
    assert!(
        !git(dir.path(), &["diff", "--cached", "--name-only"]).is_empty(),
        "<space> on Files still stages the selected file"
    );
}

#[test]
fn the_right_pane_previews_the_selected_stash() {
    let (_dir, app) = stashed_app("app-stash-preview");
    match app.diff_view() {
        DiffView::Stash(entry, diff) => {
            assert!(entry.message.contains("parked"));
            assert!(diff.text.contains("+local"), "{}", diff.text);
            assert!(diff.text.contains("new.txt"), "{}", diff.text);
        },
        other => panic!("expected a stash diff, got {other:?}"),
    }
}

#[test]
fn a_conflicting_pop_keeps_the_stash_and_shows_a_note() {
    let (dir, mut app) = stashed_app("app-stash-conflict");
    fs::write(dir.path().join("a.txt"), "one\ncommitted\n").unwrap();
    commit_all(&Repository::open(dir.path()).unwrap(), "diverge");
    app.refresh();

    app.feed_key(char_key('g'));
    assert!(app.note_popup().is_some_and(|n| n.contains("conflicts")));
    assert!(git(dir.path(), &["stash", "list"]).contains("parked"));
}

#[test]
fn stash_keys_do_nothing_while_the_popup_is_up() {
    let (_dir, mut app) = dirty_app("app-stash-popup-up");
    app.feed_key(char_key('s'));
    app.feed_key(char_key('5'));
    assert!(
        app.stash_popup().is_some(),
        "the popup owns input, '5' was typed into it"
    );
}
