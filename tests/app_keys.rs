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
//! Key routing through the keymap (`docs/PLAN_12_POLISH.md` P2b): what the
//! old `on_key` did, plus the one deliberate change, that modifiers must
//! match. The rest of the behaviour is pinned by every other `tests/app_*.rs`
//! file, which pass unmodified.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use ferrit::app::{App, Pane};
use git2::{IndexAddOption, Repository, Signature};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
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

fn two_modified_files(tag: &str) -> (TempDir, App) {
    let dir = TempDir::new(tag);
    let repo = Repository::init(dir.path()).unwrap();
    configure_identity(dir.path());
    let tall: String = (0..200).map(|n| format!("line {n}\n")).collect();
    fs::write(dir.path().join("a.txt"), &tall).unwrap();
    fs::write(dir.path().join("b.txt"), "b\n").unwrap();
    commit_all(&repo, "init");
    fs::write(
        dir.path().join("a.txt"),
        tall.replace("line 5\n", "line 5 X\n")
            .replace("line 150\n", "line 150 X\n"),
    )
    .unwrap();
    fs::write(dir.path().join("b.txt"), "b changed\n").unwrap();
    let mut app = App::open(dir.path()).unwrap();
    app.feed_key(char_key('2'));
    app.select(Pane::Files, 0);
    (dir, app)
}

fn modified(code: KeyCode, modifiers: KeyModifiers) -> KeyEvent {
    KeyEvent::new(code, modifiers)
}

#[test]
fn a_plain_d_asks_to_discard_but_ctrl_d_no_longer_does() {
    let (_dir, mut app) = two_modified_files("keys-d");
    app.feed_key(modified(KeyCode::Char('d'), KeyModifiers::CONTROL));
    assert!(
        app.confirm_message().is_none(),
        "Ctrl-d used to fall through to `d` and open the discard prompt"
    );
    app.feed_key(char_key('d'));
    assert!(app.confirm_message().is_some_and(|m| m.contains("discard")));
}

#[test]
fn alt_and_ctrl_letters_do_not_trigger_plain_letter_bindings() {
    let (_dir, mut app) = two_modified_files("keys-alt");
    app.feed_key(modified(KeyCode::Char('j'), KeyModifiers::ALT));
    assert_eq!(
        app.selected(Pane::Files),
        0,
        "Alt-j did not move the selection"
    );
    app.feed_key(char_key('j'));
    assert_eq!(app.selected(Pane::Files), 1, "plain j did");
    app.feed_key(modified(KeyCode::Char('k'), KeyModifiers::CONTROL));
    assert_eq!(app.selected(Pane::Files), 1, "Ctrl-k did not move it back");
}

#[test]
fn shift_on_a_capital_letter_still_scrolls_the_right_pane() {
    let (_dir, mut app) = two_modified_files("keys-shift");
    app.set_right_viewport(10);
    // Real terminals report `J` as the character with the shift modifier.
    app.feed_key(modified(KeyCode::Char('J'), KeyModifiers::SHIFT));
    assert_eq!(app.right_scroll(), 1);
    app.feed_key(modified(KeyCode::Char('K'), KeyModifiers::SHIFT));
    assert_eq!(app.right_scroll(), 0);
}

#[test]
fn ctrl_d_and_ctrl_u_are_half_pages_over_a_diff() {
    let (_dir, mut app) = two_modified_files("keys-half");
    app.set_right_viewport(8);
    app.feed_key(modified(KeyCode::Char('d'), KeyModifiers::CONTROL));
    assert_eq!(app.right_scroll(), 4);
    app.feed_key(modified(KeyCode::Char('u'), KeyModifiers::CONTROL));
    assert_eq!(app.right_scroll(), 0);
}

#[test]
fn scroll_keys_do_nothing_when_the_right_pane_is_not_a_diff() {
    let (_dir, mut app) = two_modified_files("keys-noscroll");
    app.feed_key(char_key('1')); // Status: no diff on the right
    app.set_right_viewport(10);
    for key in ['J', 'K', '>', '<', ']', '['] {
        app.feed_key(char_key(key));
    }
    assert_eq!(app.right_scroll(), 0);
    assert_eq!(app.selected(Pane::Files), 0, "and they moved no selection");
}

#[test]
fn the_diff_cursor_context_wins_over_the_pane_and_global_ones() {
    let (_dir, mut app) = two_modified_files("keys-cursor");
    app.set_right_area(Rect {
        x: 40,
        y: 0,
        width: 80,
        height: 12,
    });
    app.feed_key(KeyEvent::from(KeyCode::Enter)); // into the diff
    app.feed_key(char_key('j'));
    assert_eq!(
        app.selected(Pane::Files),
        0,
        "j moved the cursor inside the diff, not the file selection"
    );
    app.feed_key(KeyEvent::from(KeyCode::Esc)); // back to the file list
    app.feed_key(char_key('j'));
    assert_eq!(
        app.selected(Pane::Files),
        1,
        "outside the diff j is the selection"
    );
}

#[test]
fn a_global_binding_still_answers_inside_a_pane_context() {
    let (_dir, mut app) = two_modified_files("keys-fallthrough");
    app.feed_key(char_key('4')); // Commits has its own bindings
    app.feed_key(char_key('?')); // ... but `?` is global
    let out = {
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;
        let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
        terminal
            .draw(|f| ferrit::app::screens::draw(f, &mut app))
            .unwrap();
        terminal.backend().to_string()
    };
    assert!(out.contains("toggle this help"), "{out}");
}
