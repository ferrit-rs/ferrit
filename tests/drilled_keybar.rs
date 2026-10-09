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

use ferrit::tui::App;
use ferrit::tui::screens;
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

fn click(app: &mut App, column: u16, row: u16) {
    use ratatui::crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
    app.feed_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column,
        row,
        modifiers: KeyModifiers::NONE,
    });
}

/// Colour of the top-left border cell of the pane whose title is on `line`.
fn border_fg(terminal: &Terminal<TestBackend>, title: &str) -> ratatui::style::Color {
    let buf = terminal.backend().buffer();
    let width = buf.area.width;
    for y in 0..buf.area.height {
        let row: String = (0..width).map(|x| buf[(x, y)].symbol()).collect();
        if let Some(at) = row.find(title) {
            let x = u16::try_from(row[..at].chars().count()).unwrap();
            return buf[(x.saturating_sub(2), y)].fg;
        }
    }
    panic!("no pane titled {title}");
}

#[test]
fn clicking_the_right_pane_leaves_only_it_focused_with_its_own_key_bar() {
    let (_dir, mut app) = app_with_two_commits("right-click-focus");
    key(&mut app, KeyCode::Char('4')); // focus Commits
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    terminal.draw(|f| screens::draw(f, &mut app)).unwrap();
    let focused_fg = border_fg(&terminal, "Commits");
    assert_ne!(focused_fg, border_fg(&terminal, "Files"));

    click(&mut app, 100, 10); // inside the right pane
    assert!(app.right_focused());
    terminal.draw(|f| screens::draw(f, &mut app)).unwrap();
    let out = terminal.backend().to_string();
    assert!(out.contains("Switch view: tab"), "{out}");
    assert!(out.contains("Back: esc"), "{out}");
    assert!(
        !out.contains("Reword:"),
        "the Commits actions are gone\n{out}"
    );
    assert_eq!(
        border_fg(&terminal, "Commits"),
        border_fg(&terminal, "Files"),
        "Commits is drawn unfocused"
    );

    key(&mut app, KeyCode::Esc);
    assert!(
        frame(&mut app).contains("Reword:"),
        "Esc gives Commits back"
    );
}

#[test]
fn a_drilled_commit_opens_expanded_with_folded_directories_and_no_root_row() {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let mut path = std::env::temp_dir();
    path.push(format!("ferrit-drill-tree-{}-{nanos}", std::process::id()));
    fs::create_dir_all(path.join("test/flows")).unwrap();
    let dir = TempDir(path);
    let repo = Repository::init(&dir.0).unwrap();
    let sig = Signature::now("Test", "test@example.com").unwrap();
    fs::write(dir.0.join("test/flows/a.flow"), "x\n").unwrap();
    let mut index = repo.index().unwrap();
    index.add_path(Path::new("test/flows/a.flow")).unwrap();
    index.write().unwrap();
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
    repo.commit(Some("HEAD"), &sig, &sig, "c0", &tree, &[])
        .unwrap();
    let mut app = App::open(&dir.0).unwrap();

    key(&mut app, KeyCode::Char('4'));
    key(&mut app, KeyCode::Enter);
    assert!(app.commits_drilled());
    let out = frame(&mut app);
    assert!(out.contains("▼ test/flows"), "open folded directory\n{out}");
    assert!(
        out.contains("A a.flow"),
        "the file shows without Enter\n{out}"
    );
    assert!(out.contains("1 of 2"), "directory plus file\n{out}");
    assert!(!out.contains("▶"), "nothing starts collapsed\n{out}");

    key(&mut app, KeyCode::Enter); // toggles the selected directory row
    let out = frame(&mut app);
    assert!(out.contains("▶ test/flows"), "Enter collapses it\n{out}");
    assert!(!out.contains("A a.flow"), "{out}");
}
