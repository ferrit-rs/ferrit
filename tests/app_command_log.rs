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
//! The command log on screen (`docs/PLAN_12_POLISH.md` P0c): the panel shows
//! the newest commands ferrit ran, failures are marked, and `@` opens a
//! scrollable viewer.
//!
//! The log is process-wide and every test here writes to it, so they run one
//! at a time behind `serial()`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Mutex, MutexGuard, PoisonError};

use ferrit::app::App;
use ferrit::app::screens as ui;
use ferrit::domain::git::Repo;
use git2::{IndexAddOption, Repository, Signature};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
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

fn serial() -> MutexGuard<'static, ()> {
    static LOCK: Mutex<()> = Mutex::new(());
    LOCK.lock().unwrap_or_else(PoisonError::into_inner)
}

fn frame(app: &mut App, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|f| ui::draw(f, app)).unwrap();
    terminal.backend().to_string()
}

/// `a.txt` committed, then edited so the Files pane has a row to stage.
fn dirty_app(tag: &str) -> (TempDir, App) {
    let dir = TempDir::new(tag);
    let repo = Repository::init(dir.path()).unwrap();
    configure_identity(dir.path());
    git(dir.path(), &["checkout", "-q", "-b", "main"]);
    fs::write(dir.path().join("a.txt"), "one\n").unwrap();
    commit_all(&repo, "init");
    fs::write(dir.path().join("a.txt"), "one\ntwo\n").unwrap();
    let mut app = App::open(dir.path()).unwrap();
    app.feed_key(char_key('2'));
    (dir, app)
}

#[test]
fn the_panel_shows_the_command_ferrit_just_ran() {
    let _serial = serial();
    let (_dir, mut app) = dirty_app("log-panel");
    app.feed_key(char_key(' ')); // stage a.txt

    let out = frame(&mut app, 120, 40);
    assert!(out.contains("$ git add -- a.txt"), "{out}");
    assert!(
        !out.contains("git status --porcelain"),
        "the hard-coded sample is gone for a real repo:\n{out}"
    );
}

#[test]
fn a_failed_command_is_marked_with_its_exit_code() {
    let _serial = serial();
    let (_dir, mut app) = dirty_app("log-failed");
    app.feed_key(char_key('3')); // Branches, `main` checked out
    app.feed_key(char_key('d')); // git refuses to delete the current branch

    let out = frame(&mut app, 120, 40);
    assert!(out.contains("git branch -d main"), "{out}");
    assert!(out.contains("(exit 1)"), "{out}");
}

#[test]
fn the_author_label_shows_even_with_no_command_to_its_left() {
    let _serial = serial();
    let (_dir, mut app) = dirty_app("log-author");
    let out = frame(&mut app, 120, 40);
    assert!(out.contains("Test"), "{out}");
}

#[test]
fn at_opens_the_viewer_and_esc_closes_it() {
    let _serial = serial();
    let (_dir, mut app) = dirty_app("log-viewer");
    app.feed_key(char_key(' '));
    app.feed_key(char_key('@'));

    let out = frame(&mut app, 120, 40);
    assert!(out.contains("command log"), "{out}");
    assert!(out.contains("$ git add -- a.txt"), "{out}");
    assert!(out.contains("Esc close"), "{out}");

    app.feed_key(KeyEvent::from(KeyCode::Esc));
    assert!(!frame(&mut app, 120, 40).contains("Esc close"));
    app.feed_key(char_key('@'));
    app.feed_key(char_key('@'));
    assert!(
        !frame(&mut app, 120, 40).contains("Esc close"),
        "@ again closes it"
    );
}

#[test]
fn the_viewer_lists_reads_the_panel_hides() {
    let _serial = serial();
    let (_dir, mut app) = dirty_app("log-reads");
    // Selecting a file loads its diff, a `git diff` read.
    app.feed_key(char_key('j'));
    app.feed_key(char_key('k'));
    let panel = frame(&mut app, 120, 40);
    assert!(
        !panel.contains("$ git diff"),
        "reads stay out of the panel:\n{panel}"
    );

    app.feed_key(char_key('@'));
    let viewer = frame(&mut app, 120, 40);
    assert!(viewer.contains("$ git diff"), "{viewer}");
}

#[test]
fn the_viewer_scrolls_from_the_newest_to_the_oldest() {
    let _serial = serial();
    let (dir, mut app) = dirty_app("log-scroll");
    let repo = Repo::open(dir.path()).unwrap();
    for i in 0..60 {
        let _ = repo.checkout(&format!("log-scroll-{i:02}"));
    }
    app.feed_key(char_key('@'));

    // The panel behind the viewer always shows the two newest commands
    // (58 and 59), so these checks use entries only the viewer can show.
    let newest = frame(&mut app, 100, 24);
    assert!(newest.contains("log-scroll-50"), "{newest}");
    assert!(!newest.contains("log-scroll-30"), "{newest}");

    for _ in 0..25 {
        app.feed_key(char_key('k'));
    }
    let scrolled = frame(&mut app, 100, 24);
    assert!(scrolled.contains("log-scroll-30"), "{scrolled}");
    assert!(!scrolled.contains("log-scroll-50"), "{scrolled}");

    app.feed_key(char_key('G'));
    assert!(frame(&mut app, 100, 24).contains("log-scroll-50"));
    app.feed_key(char_key('g'));
    let oldest = frame(&mut app, 100, 24);
    assert!(!oldest.contains("log-scroll-50"), "{oldest}");
}

#[test]
fn other_keys_are_swallowed_while_the_viewer_is_up() {
    let _serial = serial();
    let (dir, mut app) = dirty_app("log-swallow");
    app.feed_key(char_key('@'));
    app.feed_key(char_key(' ')); // would stage a.txt on Files

    assert!(
        git(dir.path(), &["diff", "--cached", "--name-only"]).is_empty(),
        "the viewer owns input"
    );
}
