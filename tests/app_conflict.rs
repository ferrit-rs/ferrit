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
//! `App`-level guard for `docs/PLAN_11_REBASE.md` R0: `<space>` and `a` must
//! not stage a conflicted file that still holds conflict markers.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use ferrit::app::{App, Pane};
use git2::{IndexAddOption, Repository, Signature};
use ratatui::crossterm::event::KeyCode;
use ratatui::crossterm::event::KeyEvent;

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

/// `main` and `side` both rewrite `f`, then `side` is merged into `main`:
/// `f` is `UU` and holds conflict markers. `n` is a clean, tracked file.
fn conflict_repo(tag: &str) -> TempDir {
    let dir = TempDir::new(tag);
    let repo = Repository::init(dir.path()).unwrap();
    configure_identity(dir.path());
    git(dir.path(), &["checkout", "-q", "-b", "main"]);
    fs::write(dir.path().join("f"), "base\n").unwrap();
    fs::write(dir.path().join("n"), "n\n").unwrap();
    commit_all(&repo, "base");
    git(dir.path(), &["checkout", "-q", "-b", "side"]);
    fs::write(dir.path().join("f"), "side\n").unwrap();
    commit_all(&repo, "side");
    git(dir.path(), &["checkout", "-q", "main"]);
    fs::write(dir.path().join("f"), "main\n").unwrap();
    commit_all(&repo, "main");
    // A conflicting merge exits non-zero by design.
    let _ = Command::new("git")
        .arg("-C")
        .arg(dir.path())
        .args(["merge", "side"])
        .output()
        .unwrap();
    dir
}

fn char_key(c: char) -> KeyEvent {
    KeyEvent::from(KeyCode::Char(c))
}

/// Files focused, the conflicted `f` selected.
fn conflict_app(tag: &str) -> (TempDir, App) {
    let dir = conflict_repo(tag);
    let mut app = App::open(dir.path()).unwrap();
    app.feed_key(char_key('2'));
    let row = (0..app.row_count(Pane::Files))
        .find(|&i| app.file_lines()[i].to_string().contains('f'))
        .expect("f is listed");
    app.select(Pane::Files, row);
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
fn space_on_a_conflicted_file_with_markers_stages_nothing() {
    let (dir, mut app) = conflict_app("app-conflict-space");
    app.feed_key(char_key(' '));

    assert!(
        git(dir.path(), &["status", "--porcelain"]).contains("UU f"),
        "f must stay unmerged"
    );
    assert!(
        status_text(&app).contains("conflict markers"),
        "{}",
        status_text(&app)
    );
}

#[test]
fn space_after_resolving_the_markers_stages_the_file() {
    let (dir, mut app) = conflict_app("app-conflict-resolved");
    fs::write(dir.path().join("f"), "resolved\n").unwrap();
    app.feed_key(char_key(' '));

    assert!(
        git(dir.path(), &["status", "--porcelain"]).contains("M  f"),
        "resolved file is staged"
    );
}

#[test]
fn space_stages_a_resolved_file_that_has_a_setext_underline() {
    let (dir, mut app) = conflict_app("app-conflict-setext");
    fs::write(dir.path().join("f"), "Title\n=======\n").unwrap();
    app.feed_key(char_key(' '));

    assert!(git(dir.path(), &["status", "--porcelain"]).contains("M  f"));
}

#[test]
fn a_stages_everything_except_conflicted_files_with_markers() {
    let (dir, mut app) = conflict_app("app-conflict-all");
    fs::write(dir.path().join("n"), "n\nchanged\n").unwrap();
    app.refresh();
    app.feed_key(char_key('a'));

    let status = git(dir.path(), &["status", "--porcelain"]);
    assert!(status.contains("UU f"), "{status}");
    assert!(
        status.contains("M  n"),
        "the clean file was still staged: {status}"
    );
    assert!(
        status_text(&app).contains("conflict markers") && status_text(&app).contains('f'),
        "{}",
        status_text(&app)
    );
}

#[test]
fn a_stages_a_conflicted_file_once_its_markers_are_gone() {
    let (dir, mut app) = conflict_app("app-conflict-all-resolved");
    fs::write(dir.path().join("f"), "resolved\n").unwrap();
    app.feed_key(char_key('a'));

    assert!(git(dir.path(), &["status", "--porcelain"]).contains("M  f"));
}

#[test]
fn space_on_a_normal_file_is_unchanged() {
    let dir = conflict_repo("app-conflict-normal");
    fs::write(dir.path().join("n"), "n\nchanged\n").unwrap();
    let mut app = App::open(dir.path()).unwrap();
    app.feed_key(char_key('2'));
    let row = (0..app.row_count(Pane::Files))
        .find(|&i| app.file_lines()[i].to_string().contains(" n"))
        .expect("n is listed");
    app.select(Pane::Files, row);
    app.feed_key(char_key(' '));

    assert!(git(dir.path(), &["status", "--porcelain"]).contains("M  n"));
}
