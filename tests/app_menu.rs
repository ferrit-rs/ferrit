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
//! The `m` menu for an operation git is stopped in (`docs/PLAN_11_REBASE.md`
//! R2): continue, skip and abort, with a confirm before the abort.

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

fn open_menu(app: &mut App) {
    app.feed_key(char_key('m'));
}

fn rows(app: &App) -> Vec<String> {
    app.menu_popup().map(|m| m.rows).unwrap_or_default()
}

fn resolve(dir: &Path, content: &str) {
    fs::write(dir.join("f"), format!("{content}\n")).unwrap();
    git(dir, &["add", "f"]);
}

#[test]
fn m_does_nothing_when_no_operation_is_in_progress() {
    let dir = history("menu-none");
    let mut app = App::open(dir.path()).unwrap();
    open_menu(&mut app);
    assert!(app.menu_popup().is_none());
}

#[test]
fn a_merge_menu_has_no_skip() {
    let dir = history("menu-merge");
    conflicted_merge(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    open_menu(&mut app);

    assert_eq!(rows(&app), ["Continue  (c)", "Abort  (a)"]);
    let out = frame(&mut app);
    assert!(out.contains("MERGING"), "{out}");
    assert!(out.contains("Enter / letter run"), "{out}");
    app.feed_key(char_key('s'));
    assert!(
        app.menu_popup().is_some(),
        "s is not a merge row, the menu stays"
    );
}

#[test]
fn a_rebase_menu_shows_skip_and_the_progress() {
    let dir = history("menu-rebase");
    conflicting_rebase(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    open_menu(&mut app);

    assert_eq!(
        rows(&app),
        ["Continue  (c)", "Skip this step  (s)", "Abort  (a)"]
    );
    assert_eq!(app.menu_popup().unwrap().title, "REBASING 2/3");
}

#[test]
fn esc_closes_the_menu_and_m_does_not_nest() {
    let dir = history("menu-esc");
    conflicted_merge(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    open_menu(&mut app);
    open_menu(&mut app); // `m` inside the menu is not a row
    assert!(app.menu_popup().is_some());

    app.feed_key(KeyEvent::from(KeyCode::Esc));
    assert!(app.menu_popup().is_none());
}

#[test]
fn j_and_k_move_the_highlight_and_stop_at_the_ends() {
    let dir = history("menu-move");
    conflicting_rebase(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    open_menu(&mut app);

    assert_eq!(app.menu_popup().unwrap().selected, 0);
    app.feed_key(char_key('k'));
    assert_eq!(app.menu_popup().unwrap().selected, 0);
    for _ in 0..5 {
        app.feed_key(char_key('j'));
    }
    assert_eq!(app.menu_popup().unwrap().selected, 2);
}

#[test]
fn continue_over_an_unresolved_file_reports_git_and_changes_nothing() {
    let dir = history("menu-refuse");
    conflicted_merge(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    open_menu(&mut app);
    app.feed_key(char_key('c'));

    assert!(app.menu_popup().is_none(), "the menu closed");
    let text = status_lines(&app).join("\n");
    assert!(text.contains("unmerged files"), "{text}");
    assert!(text.contains("MERGING"), "still merging: {text}");
}

#[test]
fn a_resolved_merge_continues_to_a_merge_commit() {
    let dir = history("menu-merge-done");
    conflicted_merge(dir.path());
    resolve(dir.path(), "merged");
    let mut app = App::open(dir.path()).unwrap();
    open_menu(&mut app);
    app.feed_key(char_key('c'));

    assert!(!status_lines(&app).join("\n").contains("MERGING"));
    assert_eq!(
        git(dir.path(), &["rev-list", "--parents", "-n1", "HEAD"])
            .split(' ')
            .count(),
        3
    );
}

#[test]
fn abort_asks_first_and_n_keeps_the_operation() {
    let dir = history("menu-abort-no");
    conflicting_rebase(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    open_menu(&mut app);
    app.feed_key(char_key('a'));

    let prompt = app.confirm_message().expect("a confirm is up").to_owned();
    assert!(prompt.contains("abort the rebase"), "{prompt}");
    app.feed_key(char_key('n'));
    assert!(status_lines(&app).contains(&"REBASING 2/3".to_owned()));
}

#[test]
fn confirming_the_abort_restores_the_branch() {
    let dir = history("menu-abort-yes");
    let before = git(dir.path(), &["rev-parse", "HEAD"]);
    conflicting_rebase(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    open_menu(&mut app);
    // Enter on the highlighted row: move to Abort first.
    app.feed_key(char_key('j'));
    app.feed_key(char_key('j'));
    app.feed_key(KeyEvent::from(KeyCode::Enter));
    app.feed_key(char_key('y'));

    assert!(!status_lines(&app).join("\n").contains("REBASING"));
    assert_eq!(git(dir.path(), &["rev-parse", "HEAD"]), before);
}

#[test]
fn a_continue_that_hits_the_next_conflict_says_so() {
    let dir = history("menu-next-conflict");
    conflicting_rebase(dir.path());
    resolve(dir.path(), "resolved differently");
    let mut app = App::open(dir.path()).unwrap();
    open_menu(&mut app);
    app.feed_key(char_key('c'));

    let note = app.note_popup().expect("a note explains the stop");
    assert!(note.contains("stopped on a conflict"), "{note}");
    assert!(status_lines(&app).contains(&"REBASING 3/3".to_owned()));
}

#[test]
fn skip_moves_a_rebase_forward() {
    let dir = history("menu-skip");
    conflicting_rebase(dir.path());
    let mut app = App::open(dir.path()).unwrap();
    open_menu(&mut app);
    app.feed_key(char_key('s'));

    // Skipping `two` sends `three` onto `base`, which conflicts as well.
    assert!(status_lines(&app).contains(&"REBASING 3/3".to_owned()));
    assert!(app.note_popup().is_some());
}

#[test]
fn an_edit_stop_is_reported_as_stopped_for_editing() {
    let dir = history("menu-edit");
    let short = |rev: &str| git(dir.path(), &["rev-parse", "--short", rev]);
    let todo = format!(
        "edit {}\nedit {}\npick {}\n",
        short("HEAD~2"),
        short("HEAD~1"),
        short("HEAD")
    );
    let file = dir.path().join(".git").join("ferrit-test-todo");
    fs::write(&file, todo).unwrap();
    let editor = format!("cp {}", file.display());
    assert!(try_git(
        dir.path(),
        &["rebase", "-i", "HEAD~3"],
        &[("GIT_SEQUENCE_EDITOR", editor.as_str())]
    ));
    let mut app = App::open(dir.path()).unwrap();
    open_menu(&mut app);
    app.feed_key(char_key('c'));

    let note = app.note_popup().expect("the next edit stop is reported");
    assert!(note.contains("stopped for you to edit"), "{note}");
}

#[test]
fn the_keybar_points_at_the_menu_only_while_an_operation_is_stopped() {
    let dir = history("menu-keybar");
    let mut app = App::open(dir.path()).unwrap();
    assert!(!frame(&mut app).contains("abort: m"));

    conflicted_merge(dir.path());
    app.refresh();
    assert!(frame(&mut app).contains("abort: m"));
}

#[test]
fn a_cherry_pick_abort_names_it() {
    let dir = history("menu-pick");
    git(dir.path(), &["checkout", "-q", "-b", "other", "HEAD~3"]);
    fs::write(dir.path().join("f"), "other\n").unwrap();
    git(dir.path(), &["commit", "-qam", "other"]);
    assert!(!try_git(dir.path(), &["cherry-pick", "main"], &[]));
    let mut app = App::open(dir.path()).unwrap();
    open_menu(&mut app);
    app.feed_key(char_key('a'));
    assert!(
        app.confirm_message()
            .is_some_and(|m| m.contains("abort the cherry-pick"))
    );
}
