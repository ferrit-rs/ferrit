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
//! `App`-level wiring for the commit popup (`docs/PLAN_7_COMMIT.md`): `c`
//! opens it, typing fills the draft, `Ctrl-S` commits and refreshes, `Esc`
//! cancels but keeps the draft for the next `c`. `A` / `w` pre-fill from
//! `HEAD`'s message.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use ferrit::app::{App, DiffView, Pane};
use git2::{IndexAddOption, Repository, Signature};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

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

fn char_key(c: char) -> KeyEvent {
    KeyEvent::from(KeyCode::Char(c))
}

fn ctrl_key(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
}

fn type_text(app: &mut App, text: &str) {
    for c in text.chars() {
        app.feed_key(char_key(c));
    }
}

fn head_summary(app: &mut App) -> String {
    app.select(Pane::Commits, 0);
    match app.diff_view() {
        DiffView::Commit(entry, _) => entry.summary.clone(),
        other => panic!("expected the newest commit's diff, got {other:?}"),
    }
}

#[test]
fn c_opens_types_and_ctrl_s_commits() {
    let dir = TempDir::new("app-commit-basic");
    let repo = Repository::init(dir.path()).unwrap();
    configure_identity(dir.path());
    fs::write(dir.path().join("a.txt"), "one\n").unwrap();
    commit_all(&repo, "init");
    fs::write(dir.path().join("a.txt"), "one\ntwo\n").unwrap();
    Command::new("git")
        .arg("-C")
        .arg(dir.path())
        .args(["add", "a.txt"])
        .output()
        .unwrap();

    let mut app = App::open(dir.path()).unwrap();
    let before = app.row_count(Pane::Commits);

    app.feed_key(char_key('c'));
    assert!(app.commit_popup().is_some(), "c opened the popup");
    type_text(&mut app, "feat: a new line");
    app.feed_key(ctrl_key('s'));

    assert!(app.commit_popup().is_none(), "popup closed on success");
    assert!(app.note_popup().is_none());
    assert_eq!(app.row_count(Pane::Commits), before + 1, "the new commit showed up");
    assert_eq!(head_summary(&mut app), "feat: a new line");
}

#[test]
fn esc_cancels_but_keeps_the_draft_for_next_time() {
    let dir = TempDir::new("app-commit-draft");
    let repo = Repository::init(dir.path()).unwrap();
    configure_identity(dir.path());
    fs::write(dir.path().join("a.txt"), "one\n").unwrap();
    commit_all(&repo, "init");
    fs::write(dir.path().join("a.txt"), "one\ntwo\n").unwrap();
    Command::new("git")
        .arg("-C")
        .arg(dir.path())
        .args(["add", "a.txt"])
        .output()
        .unwrap();

    let mut app = App::open(dir.path()).unwrap();

    app.feed_key(char_key('c'));
    type_text(&mut app, "wip: half a thought");
    app.feed_key(KeyEvent::from(KeyCode::Esc));
    assert!(app.commit_popup().is_none(), "Esc closed the popup");

    app.feed_key(char_key('c'));
    let view = app.commit_popup().expect("c reopened the popup");
    assert_eq!(view.lines.join("\n"), "wip: half a thought", "draft came back");
}

#[test]
fn empty_message_is_rejected_with_a_note_and_no_commit() {
    let dir = TempDir::new("app-commit-empty");
    let repo = Repository::init(dir.path()).unwrap();
    configure_identity(dir.path());
    fs::write(dir.path().join("a.txt"), "one\n").unwrap();
    commit_all(&repo, "init");
    fs::write(dir.path().join("a.txt"), "one\ntwo\n").unwrap();
    Command::new("git")
        .arg("-C")
        .arg(dir.path())
        .args(["add", "a.txt"])
        .output()
        .unwrap();

    let mut app = App::open(dir.path()).unwrap();
    let before = app.row_count(Pane::Commits);

    app.feed_key(char_key('c'));
    app.feed_key(ctrl_key('s'));

    assert!(app.note_popup().is_some(), "empty message is rejected");
    assert_eq!(app.row_count(Pane::Commits), before, "no commit was made");
}

#[test]
fn c_is_a_noop_with_nothing_staged() {
    let dir = TempDir::new("app-commit-nostage");
    let repo = Repository::init(dir.path()).unwrap();
    configure_identity(dir.path());
    fs::write(dir.path().join("a.txt"), "one\n").unwrap();
    commit_all(&repo, "init");

    let mut app = App::open(dir.path()).unwrap();
    app.feed_key(char_key('c'));
    assert!(app.commit_popup().is_none(), "nothing staged: c does nothing");
}

#[test]
fn amend_prefills_the_current_message_and_keeps_the_parent() {
    let dir = TempDir::new("app-commit-amend");
    let repo = Repository::init(dir.path()).unwrap();
    configure_identity(dir.path());
    fs::write(dir.path().join("a.txt"), "one\n").unwrap();
    commit_all(&repo, "base");
    fs::write(dir.path().join("b.txt"), "two\n").unwrap();
    commit_all(&repo, "feat: wip message");

    let mut app = App::open(dir.path()).unwrap();
    app.feed_key(KeyEvent::from(KeyCode::Char('A')));
    let view = app.commit_popup().expect("A opened the popup");
    assert_eq!(view.title, "Amend HEAD");
    assert_eq!(view.lines.join("\n"), "feat: wip message");

    // Replace the prefilled text with a fixed one: backspace it all out,
    // then type the real message.
    for _ in 0.."feat: wip message".chars().count() {
        app.feed_key(KeyEvent::from(KeyCode::Backspace));
    }
    type_text(&mut app, "feat: the real message");
    app.feed_key(ctrl_key('s'));

    assert_eq!(head_summary(&mut app), "feat: the real message");
    assert_eq!(app.row_count(Pane::Commits), 2, "amend did not add a commit");
}
