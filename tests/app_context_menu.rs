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
//! The `x` menu and right-click (`docs/PLAN_12_POLISH.md` P4): six extra
//! actions that do not earn a key, each one git command.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use ferrit::app::App;
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

fn status_lines(app: &App) -> Vec<String> {
    app.status_lines().iter().map(ToString::to_string).collect()
}

fn rows(app: &App) -> Vec<String> {
    app.menu_popup().map(|m| m.rows).unwrap_or_default()
}

fn type_text(app: &mut App, text: &str) {
    for c in text.chars() {
        app.feed_key(char_key(c));
    }
}

fn enter(app: &mut App) {
    app.feed_key(KeyEvent::from(KeyCode::Enter));
}

fn parents(dir: &Path, rev: &str) -> usize {
    git(dir, &["rev-list", "--parents", "-n1", rev])
        .split(' ')
        .count()
        - 1
}

fn branch_names(dir: &Path) -> Vec<String> {
    git(dir, &["branch", "--format=%(refname:short)"])
        .lines()
        .map(str::to_owned)
        .collect()
}

// ---------------------------------------------------------------- branches

#[test]
fn the_branches_menu_offers_rename_and_no_ff_for_the_selected_branch() {
    let dir = history("x-branches");
    git(dir.path(), &["branch", "topic"]);
    let mut app = App::open(dir.path()).unwrap();
    app.feed_key(char_key('3'));
    app.feed_key(char_key('x'));

    assert_eq!(
        rows(&app),
        ["Rename branch  (r)", "Merge with --no-ff  (n)"]
    );
    assert_eq!(app.menu_popup().unwrap().title, "main");
    app.feed_key(KeyEvent::from(KeyCode::Esc));
    assert!(app.menu_popup().is_none());
}

#[test]
fn renaming_a_branch_prefills_the_name_and_renames_it() {
    let dir = history("x-rename");
    git(dir.path(), &["branch", "topic"]);
    let mut app = App::open(dir.path()).unwrap();
    app.feed_key(char_key('3'));
    app.feed_key(char_key('j')); // topic
    app.feed_key(char_key('x'));
    app.feed_key(char_key('r'));

    let popup = app.name_popup().expect("a name popup");
    assert_eq!(popup.title, "Rename branch");
    assert_eq!(
        popup.lines.join(""),
        "topic",
        "pre-filled with the current name"
    );
    type_text(&mut app, "-2");
    enter(&mut app);

    assert!(app.name_popup().is_none(), "closed on success");
    assert_eq!(branch_names(dir.path()), ["main", "topic-2"]);
}

#[test]
fn a_taken_name_keeps_the_rename_popup_and_the_text() {
    let dir = history("x-rename-taken");
    git(dir.path(), &["branch", "topic"]);
    let mut app = App::open(dir.path()).unwrap();
    app.feed_key(char_key('3'));
    app.feed_key(char_key('j'));
    app.feed_key(char_key('x'));
    app.feed_key(char_key('r'));
    for _ in 0..5 {
        app.feed_key(KeyEvent::from(KeyCode::Backspace));
    }
    type_text(&mut app, "main"); // already exists
    enter(&mut app);

    let popup = app.name_popup().expect("the popup stays for a retry");
    assert_eq!(popup.lines.join(""), "main");
    assert_eq!(
        branch_names(dir.path()),
        ["main", "topic"],
        "nothing was renamed"
    );
}

#[test]
fn merge_no_ff_makes_a_merge_commit_where_a_plain_merge_would_fast_forward() {
    let dir = history("x-noff");
    git(dir.path(), &["checkout", "-q", "-b", "topic"]);
    fs::write(dir.path().join("g"), "topic\n").unwrap();
    git(dir.path(), &["add", "g"]);
    git(dir.path(), &["commit", "-qm", "topic"]);
    git(dir.path(), &["checkout", "-q", "main"]);
    let mut app = App::open(dir.path()).unwrap();
    app.feed_key(char_key('3'));
    app.feed_key(char_key('j')); // topic
    app.feed_key(char_key('x'));
    app.feed_key(char_key('n'));

    assert_eq!(parents(dir.path(), "HEAD"), 2, "a merge commit");
    assert!(git(dir.path(), &["log", "-1", "--format=%s"]).starts_with("Merge branch 'topic'"));
}

// ----------------------------------------------------------------- commits

#[test]
fn a_branch_can_start_at_any_commit() {
    let dir = history("x-branch-at");
    let older = git(dir.path(), &["rev-parse", "HEAD~2"]);
    let short = git(dir.path(), &["rev-parse", "--short", "HEAD~2"]);
    let mut app = App::open(dir.path()).unwrap();
    app.feed_key(char_key('4'));
    app.feed_key(char_key('j'));
    app.feed_key(char_key('j')); // `one`
    app.feed_key(char_key('x'));
    assert_eq!(rows(&app), ["New branch from this commit  (b)"]);
    app.feed_key(char_key('b'));

    let popup = app.name_popup().expect("a name popup");
    assert!(popup.title.contains(&short), "{}", popup.title);
    type_text(&mut app, "old-work");
    enter(&mut app);

    assert_eq!(
        git(dir.path(), &["symbolic-ref", "--short", "HEAD"]),
        "old-work"
    );
    assert_eq!(git(dir.path(), &["rev-parse", "HEAD"]), older);
}

// ------------------------------------------------------------------- stash

fn app_with_staged_and_unstaged(tag: &str) -> (TempDir, App) {
    let dir = history(tag);
    fs::write(dir.path().join("f"), "staged\n").unwrap();
    git(dir.path(), &["add", "f"]);
    fs::write(dir.path().join("f"), "staged\nunstaged\n").unwrap();
    let app = App::open(dir.path()).unwrap();
    (dir, app)
}

#[test]
fn stash_keeping_the_index_leaves_the_staged_part_in_place() {
    let (dir, mut app) = app_with_staged_and_unstaged("x-keep-index");
    app.feed_key(char_key('5'));
    app.feed_key(char_key('x'));
    assert_eq!(
        rows(&app),
        ["Stash, keeping the index  (i)"],
        "no entry to rename yet"
    );
    app.feed_key(char_key('i'));
    type_text(&mut app, "kept");
    enter(&mut app);

    assert!(git(dir.path(), &["stash", "list"]).contains("kept"));
    assert_eq!(git(dir.path(), &["status", "--porcelain"]), "M  f");
    assert_eq!(
        fs::read_to_string(dir.path().join("f")).unwrap(),
        "staged\n"
    );
}

#[test]
fn stash_keeping_the_index_on_a_clean_tree_is_an_error_not_a_popup() {
    let dir = history("x-keep-clean");
    let mut app = App::open(dir.path()).unwrap();
    app.feed_key(char_key('5'));
    app.feed_key(char_key('x'));
    app.feed_key(char_key('i'));
    assert!(app.name_popup().is_none());
    assert!(
        status_lines(&app)
            .join("\n")
            .contains("no local changes to save")
    );
}

#[test]
fn renaming_a_stash_edits_its_message_and_moves_it_to_the_top() {
    let (dir, mut app) = app_with_staged_and_unstaged("x-stash-rename");
    app.feed_key(char_key('2'));
    app.feed_key(char_key('s'));
    type_text(&mut app, "first");
    enter(&mut app);
    fs::write(dir.path().join("f"), "another\n").unwrap();
    app.refresh();
    app.feed_key(char_key('s'));
    type_text(&mut app, "second");
    enter(&mut app);
    app.feed_key(char_key('5'));
    app.feed_key(char_key('j')); // `first`, the older one
    app.feed_key(char_key('x'));
    assert_eq!(
        rows(&app),
        ["Stash, keeping the index  (i)", "Rename stash  (r)"]
    );
    app.feed_key(char_key('r'));

    let popup = app.name_popup().expect("a name popup");
    assert!(popup.lines.join("").contains("first"), "{:?}", popup.lines);
    for _ in 0..5 {
        app.feed_key(KeyEvent::from(KeyCode::Backspace));
    }
    type_text(&mut app, "renamed");
    enter(&mut app);

    let list = git(dir.path(), &["stash", "list"]);
    assert_eq!(list.lines().count(), 2, "{list}");
    assert!(list.lines().next().unwrap().contains("renamed"), "{list}");
    assert!(!list.contains("first"), "{list}");
}

#[test]
fn an_empty_stash_message_is_refused_when_renaming() {
    let (dir, mut app) = app_with_staged_and_unstaged("x-stash-empty");
    app.feed_key(char_key('2'));
    app.feed_key(char_key('s'));
    type_text(&mut app, "x");
    enter(&mut app);
    app.feed_key(char_key('5'));
    app.feed_key(char_key('x'));
    app.feed_key(char_key('r'));
    for _ in 0..40 {
        app.feed_key(KeyEvent::from(KeyCode::Backspace));
    }
    enter(&mut app);

    assert!(app.name_popup().is_some(), "the popup stays");
    assert!(status_lines(&app).join("\n").contains("needs a message"));
    assert!(
        git(dir.path(), &["stash", "list"]).contains(": x"),
        "unchanged"
    );
}

// ---------------------------------------------------------------- conflicts

fn conflict_app(tag: &str) -> (TempDir, App) {
    let dir = history(tag);
    git(dir.path(), &["checkout", "-q", "-b", "side", "HEAD~2"]);
    fs::write(dir.path().join("f"), "side\n").unwrap();
    git(dir.path(), &["commit", "-qam", "side"]);
    git(dir.path(), &["checkout", "-q", "main"]);
    let _ = Command::new("git")
        .arg("-C")
        .arg(dir.path())
        .args(["merge", "side"])
        .output()
        .unwrap();
    let mut app = App::open(dir.path()).unwrap();
    app.feed_key(char_key('2'));
    app.feed_key(char_key('j')); // the file row
    (dir, app)
}

#[test]
fn a_conflicted_file_can_take_ours_or_theirs_and_then_be_staged() {
    for (key, expected) in [('o', "three\n"), ('t', "side\n")] {
        let (dir, mut app) = conflict_app(&format!("x-conflict-{key}"));
        app.feed_key(char_key('x'));
        assert_eq!(rows(&app), ["Take ours  (o)", "Take theirs  (t)"]);
        assert!(app.menu_popup().unwrap().title.contains("(conflict)"));
        app.feed_key(char_key(key));

        assert_eq!(fs::read_to_string(dir.path().join("f")).unwrap(), expected);
        app.feed_key(char_key(' ')); // the markers are gone, so staging is allowed
        assert!(!git(dir.path(), &["status", "--porcelain"]).contains("UU"));
    }
}

#[test]
fn a_file_that_is_not_conflicted_has_no_menu() {
    let dir = history("x-no-conflict");
    fs::write(dir.path().join("f"), "edited\n").unwrap();
    let mut app = App::open(dir.path()).unwrap();
    app.feed_key(char_key('2'));
    app.feed_key(char_key('j'));
    app.feed_key(char_key('x'));

    assert!(app.menu_popup().is_none());
    assert!(status_lines(&app).join("\n").contains("no extra actions"));
}
