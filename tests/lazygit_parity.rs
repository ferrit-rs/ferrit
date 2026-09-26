#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::pathbuf_init_then_push,
    reason = "integration test scaffolding: a failed setup is the assertion"
)]
//! What the audit of `test/flows/feature-workflow.flow` found ferrit lacking next to
//! lazygit. Each test is one row's definition of done, on a real repository, read
//! from the rendered frame.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use ferrit::app::{App, Pane, screens};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent};

struct Repo {
    dir: PathBuf,
}

impl Repo {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let mut dir = std::env::temp_dir();
        dir.push(format!(
            "ferrit-parity-{tag}-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).unwrap();
        let repo = Self { dir };
        repo.git(&["init", "-q", "-b", "main"]);
        for (key, value) in [
            ("user.name", "Test"),
            ("user.email", "test@example.com"),
            ("commit.gpgsign", "false"),
            ("core.editor", "true"),
        ] {
            repo.git(&["config", key, value]);
        }
        repo
    }

    fn git(&self, args: &[&str]) -> String {
        let out = Command::new("git")
            .arg("-C")
            .arg(&self.dir)
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    }

    fn write(&self, path: &str, text: &str) {
        let full = self.dir.join(path);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(full, text).unwrap();
    }

    fn commit(&self, path: &str, text: &str, message: &str) {
        self.write(path, text);
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "-m", message]);
    }

    fn app(&self) -> App {
        App::open(&self.dir).unwrap()
    }
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn frame(app: &mut App) -> String {
    let mut terminal = Terminal::new(TestBackend::new(140, 40)).unwrap();
    terminal.draw(|f| screens::draw(f, app)).unwrap();
    terminal.backend().to_string()
}

fn key(app: &mut App, c: char) {
    app.feed_key(KeyEvent::from(KeyCode::Char(c)));
}

/// Done when: with nothing to commit and the Files pane focused, the right pane is
/// titled "Diff" and says "No changed files", as lazygit's does (step 6, 11).
#[test]
fn an_empty_working_tree_gives_a_diff_pane_that_says_so() {
    let repo = Repo::new("empty-diff");
    repo.commit("a.txt", "one\n", "init");
    let mut app = repo.app();
    key(&mut app, '2');
    let out = frame(&mut app);
    assert!(out.contains("Diff"), "{out}");
    assert!(out.contains("No changed files"), "{out}");
    assert!(!out.contains("Unstaged changes"), "{out}");

    // With a change, the two-sided view is back.
    repo.write("a.txt", "one\ntwo\n");
    app.refresh();
    let out = frame(&mut app);
    assert!(out.contains("Unstaged Changes"), "{out}");
    assert!(!out.contains("No changed files"), "{out}");
    let _ = (Pane::Files, Path::new(""));
}
