#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::pathbuf_init_then_push,
    reason = "integration test scaffolding: a failed setup is the assertion"
)]
//! The key bar while the Commits or Branches pane is drilled in (Enter on a commit
//! lists its files, Enter on a branch lists its log): what the list offers there is
//! read only, so the bar must not show `Stage`, `Commit` or `Reword`, which do
//! nothing on those rows. Found by comparing `test/flows/feature-workflow.flow`
//! with lazygit, which switches to the sub-view's own keys.

use std::fs;
use std::path::{Path, PathBuf};

use ferrit::app::{App, screens};
use git2::{Repository, Signature};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent};

struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn app_with_two_commits(tag: &str) -> (TempDir, App) {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let mut path = std::env::temp_dir();
    path.push(format!("ferrit-{tag}-{}-{nanos}", std::process::id()));
    fs::create_dir_all(&path).unwrap();
    let dir = TempDir(path);
    let repo = Repository::init(&dir.0).unwrap();
    let sig = Signature::now("Test", "test@example.com").unwrap();
    let mut parent: Option<git2::Commit<'_>> = None;
    for n in 0..2 {
        fs::write(dir.0.join("f.txt"), format!("{n}\n")).unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("f.txt")).unwrap();
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let parents: Vec<&git2::Commit<'_>> = parent.iter().collect();
        let oid = repo
            .commit(Some("HEAD"), &sig, &sig, &format!("c{n}"), &tree, &parents)
            .unwrap();
        parent = Some(repo.find_commit(oid).unwrap());
    }
    let app = App::open(&dir.0).unwrap();
    (dir, app)
}

fn frame(app: &mut App) -> String {
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    terminal.draw(|f| screens::draw(f, app)).unwrap();
    terminal.backend().to_string()
}

fn key(app: &mut App, code: KeyCode) {
    app.feed_key(KeyEvent::from(code));
}

#[test]
fn keybar_offers_only_back_and_open_inside_a_commit() {
    let (_dir, mut app) = app_with_two_commits("drilled-keybar-commit");
    key(&mut app, KeyCode::Char('4')); // focus Commits
    assert!(frame(&mut app).contains("Reword:"), "the Commits bar first");

    key(&mut app, KeyCode::Enter);
    assert!(app.commits_drilled(), "Enter drilled into the commit");
    let out = frame(&mut app);
    assert!(out.contains("Back: esc"), "{out}");
    assert!(out.contains("Open: enter"), "{out}");
    for hint in ["Stage:", "Commit:", "Amend:", "Reword:", "Drop:"] {
        assert!(!out.contains(hint), "{hint} does nothing here\n{out}");
    }

    key(&mut app, KeyCode::Esc);
    assert!(
        frame(&mut app).contains("Reword:"),
        "backing out brings the Commits bar back"
    );
}

#[test]
fn keybar_offers_only_back_and_open_inside_a_branch() {
    let (_dir, mut app) = app_with_two_commits("drilled-keybar-branch");
    key(&mut app, KeyCode::Char('3')); // focus Branches
    assert!(
        frame(&mut app).contains("Checkout:"),
        "the Branches bar first"
    );

    key(&mut app, KeyCode::Enter);
    assert!(
        app.branches_drilled(),
        "Enter drilled into the branch's log"
    );
    let out = frame(&mut app);
    assert!(out.contains("Back: esc"), "{out}");
    assert!(!out.contains("Checkout:"), "{out}");
    assert!(!out.contains("Merge:"), "{out}");
}
