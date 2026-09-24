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
//! The Status pane names the operation git is stopped in
//! (`docs/PLAN_11_REBASE.md` R1): `MERGING`, `REBASING 2/3`, and so on, right
//! under the first line, refreshed when the state changes.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use ferrit::app::App;
use ferrit::app::screens as ui;
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

fn try_git(dir: &Path, args: &[&str], envs: &[(&str, &str)]) -> bool {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(dir).args(args).env("GIT_EDITOR", "true");
    for (key, value) in envs {
        cmd.env(key, value);
    }
    cmd.output().unwrap().status.success()
}

fn history(tag: &str) -> TempDir {
    let dir = TempDir::new(tag);
    let repo = Repository::init(dir.path()).unwrap();
    configure_identity(dir.path());
    git(dir.path(), &["checkout", "-q", "-b", "main"]);
    for content in ["base", "one", "two", "three"] {
        fs::write(dir.path().join("f"), format!("{content}\n")).unwrap();
        commit_all(&repo, content);
    }
    dir
}

fn conflicted_merge(dir: &Path) {
    git(dir, &["checkout", "-q", "-b", "side", "HEAD~2"]);
    fs::write(dir.join("f"), "side\n").unwrap();
    git(dir, &["commit", "-qam", "side"]);
    git(dir, &["checkout", "-q", "main"]);
    assert!(!try_git(dir, &["merge", "side"], &[]));
}

fn conflicting_rebase(dir: &Path) {
    let short = |rev: &str| git(dir, &["rev-parse", "--short", rev]);
    let todo = format!(
        "drop {}\npick {}\npick {}\n",
        short("HEAD~2"),
        short("HEAD~1"),
        short("HEAD")
    );
    let file = dir.join(".git").join("ferrit-test-todo");
    fs::write(&file, todo).unwrap();
    let editor = format!("cp {}", file.display());
    assert!(!try_git(
        dir,
        &["rebase", "-i", "HEAD~3"],
        &[("GIT_SEQUENCE_EDITOR", editor.as_str())]
    ));
}

fn status_lines(app: &App) -> Vec<String> {
    app.status_lines().iter().map(ToString::to_string).collect()
}

fn frame(app: &mut App) -> String {
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    terminal.draw(|f| ui::draw(f, app)).unwrap();
    terminal.backend().to_string()
}

#[test]
fn a_clean_repository_shows_no_badge() {
    let dir = history("opapp-clean");
    let app = App::open(dir.path()).unwrap();
    let text = status_lines(&app).join("\n");
    assert!(
        !text.contains("MERGING") && !text.contains("REBASING"),
        "{text}"
    );
}

#[test]
fn a_conflicted_merge_shows_merging_right_under_the_first_line() {
    let dir = history("opapp-merge");
    conflicted_merge(dir.path());
    let app = App::open(dir.path()).unwrap();

    let lines = status_lines(&app);
    assert_eq!(lines[1], "MERGING", "{lines:?}");
    assert!(frame(&mut { app }).contains("MERGING"));
}

#[test]
fn a_rebase_shows_its_progress() {
    let dir = history("opapp-rebase");
    conflicting_rebase(dir.path());
    let app = App::open(dir.path()).unwrap();
    assert_eq!(status_lines(&app)[1], "REBASING 2/3");
}

#[test]
fn the_badge_follows_the_state_across_a_refresh() {
    let dir = history("opapp-refresh");
    conflicted_merge(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    assert!(status_lines(&app).contains(&"MERGING".to_owned()));

    git(dir.path(), &["merge", "--abort"]);
    app.refresh();
    assert!(!status_lines(&app).contains(&"MERGING".to_owned()));
}

#[test]
fn the_badge_sits_below_an_error_line() {
    let dir = history("opapp-error");
    conflicted_merge(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    // Git refuses to delete the checked-out branch: an error line.
    app.feed_key(char_key('3'));
    app.feed_key(char_key('d'));
    let lines = status_lines(&app);
    assert!(lines[0].starts_with("error:"), "{lines:?}");
    assert_eq!(lines[1], "MERGING", "{lines:?}");
}

#[test]
fn the_mock_app_has_no_badge() {
    let app = App::mock();
    assert!(!status_lines(&app).iter().any(|l| l.contains("MERGING")));
}
