#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "integration test scaffolding: a failed setup is the assertion"
)]
//! The popups of creating the GitHub repository rendered into a `TestBackend`
//! (`docs/PLAN_15_CREATE_REMOTE.md`, R3). `gh` is a fake script that says it is
//! ready; nothing here reaches GitHub.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;

use ferrit::app::{App, screens as ui};
use ferrit::domain::git::host::GhProgram;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::style::Color;

struct Project {
    dir: PathBuf,
}

impl Project {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("ferrit-{tag}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(dir.join("work")).unwrap();
        let status = Command::new("git")
            .arg("-C")
            .arg(dir.join("work"))
            .args(["init", "-q", "."])
            .status()
            .unwrap();
        assert!(status.success());
        let script = dir.join("gh");
        fs::write(&script, "#!/bin/sh\nexit 0\n").unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        Self { dir }
    }

    /// An app with the check answered: the form is up.
    fn app_with_form(&self) -> App {
        let mut app = App::open(&self.dir.join("work")).unwrap();
        app.set_gh_program(GhProgram::new(self.dir.join("gh")));
        app.open_create_remote();
        app
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn render(app: &mut App, width: u16, height: u16) -> Buffer {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal.draw(|f| ui::draw(f, app)).unwrap();
    terminal.backend().buffer().clone()
}

fn text(buf: &Buffer) -> String {
    (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn press(app: &mut App, code: KeyCode) {
    app.feed_key(KeyEvent::from(code));
}

#[test]
fn the_form_shows_its_four_fields_private_by_default() {
    let project = Project::new("crs-form");
    let mut app = project.app_with_form();
    let shown = text(&render(&mut app, 100, 30));

    assert!(shown.contains("Create on GitHub"), "{shown}");
    for label in ["Name", "Visibility", "Description"] {
        assert!(shown.contains(label), "{label} in {shown}");
    }
    assert!(shown.contains("work"), "the folder's name is the default");
    assert!(shown.contains("(\u{2022}) private"), "{shown}");
    assert!(shown.contains("( ) public"), "{shown}");
    assert!(shown.contains("after creating"), "{shown}");
    assert!(shown.contains("[x]"));
    assert!(shown.contains("Next: Tab"), "{shown}");
}

#[test]
fn choosing_public_moves_the_dot_and_a_bad_name_shows_its_reason() {
    let project = Project::new("crs-public");
    let mut app = project.app_with_form();
    press(&mut app, KeyCode::Tab);
    press(&mut app, KeyCode::Char(' '));
    let shown = text(&render(&mut app, 100, 30));
    assert!(
        shown.contains("( ) private") && shown.contains("(\u{2022}) public"),
        "{shown}"
    );

    press(&mut app, KeyCode::BackTab);
    for _ in 0..10 {
        press(&mut app, KeyCode::Backspace);
    }
    for c in "a b".chars() {
        app.feed_key(KeyEvent::from(KeyCode::Char(c)));
    }
    press(&mut app, KeyCode::Enter);
    let shown = text(&render(&mut app, 100, 30));
    assert!(shown.contains("' ' is not allowed in a name"), "{shown}");
}

#[test]
fn the_last_question_for_a_private_repository_says_enter_creates() {
    let project = Project::new("crs-private");
    let mut app = project.app_with_form();
    press(&mut app, KeyCode::Enter);
    let shown = text(&render(&mut app, 100, 30));
    assert!(shown.contains("Create work"), "{shown}");
    assert!(shown.contains("PRIVATE repository"), "{shown}");
    assert!(shown.contains("add remote `origin`"), "{shown}");
    assert!(shown.contains("Enter/y: create"), "{shown}");
}

#[test]
fn the_last_question_for_a_public_repository_is_red_and_says_enter_does_not_create() {
    let project = Project::new("crs-warn");
    let mut app = project.app_with_form();
    press(&mut app, KeyCode::Tab);
    press(&mut app, KeyCode::Char(' '));
    press(&mut app, KeyCode::Enter);
    let buf = render(&mut app, 100, 30);
    let shown = text(&buf);
    assert!(shown.contains("PUBLIC repository"), "{shown}");
    assert!(shown.contains("Everyone can read its history."), "{shown}");
    assert!(shown.contains("y: create (Enter does not)"), "{shown}");

    // The warning colour of the discard prompt, on the line that says PUBLIC.
    let row = text(&buf)
        .lines()
        .position(|line| line.contains("PUBLIC repository"))
        .unwrap();
    let line = text(&buf).lines().nth(row).unwrap().to_owned();
    let column = line[..line.find("PUBLIC").unwrap()].chars().count();
    let cell = &buf[(u16::try_from(column).unwrap(), u16::try_from(row).unwrap())];
    assert_eq!(cell.style().fg, Some(Color::Red));
}

#[test]
fn the_check_popup_and_tiny_terminals_do_not_panic() {
    let project = Project::new("crs-tiny");
    let mut app = project.app_with_form();
    for (w, h) in [(1, 1), (10, 3), (30, 5), (50, 8)] {
        let _ = render(&mut app, w, h);
    }
    press(&mut app, KeyCode::Enter);
    for (w, h) in [(1, 1), (10, 3), (30, 5)] {
        let _ = render(&mut app, w, h);
    }
}

#[test]
fn a_repository_with_no_commit_shows_the_first_commit_row_ticked_and_the_question_says_it() {
    let project = Project::new("crs-first");
    let mut app = project.app_with_form();
    let shown = text(&render(&mut app, 100, 30));
    assert!(
        shown.contains("Initial commit with an empty README.md"),
        "{shown}"
    );
    assert!(shown.contains("[x]"));

    press(&mut app, KeyCode::Enter);
    let shown = text(&render(&mut app, 100, 30));
    assert!(
        shown.contains("first: commit an empty README.md"),
        "{shown}"
    );
    assert!(shown.contains("add remote `origin`"), "{shown}");
}

#[test]
fn unticked_the_row_says_so_and_the_question_drops_the_line() {
    let project = Project::new("crs-first-off");
    let mut app = project.app_with_form();
    for _ in 0..3 {
        press(&mut app, KeyCode::Tab);
    }
    press(&mut app, KeyCode::Char(' '));
    let shown = text(&render(&mut app, 100, 30));
    let row = shown
        .lines()
        .find(|l| l.contains("Initial commit"))
        .unwrap();
    assert!(row.contains("[ ]"), "{row}");
    press(&mut app, KeyCode::Enter);
    assert!(!text(&render(&mut app, 100, 30)).contains("first: commit"));
}

#[test]
fn with_a_commit_there_is_no_such_row() {
    let project = Project::new("crs-first-none");
    let work = project.dir.join("work");
    for args in [
        &["config", "user.name", "T"][..],
        &["config", "user.email", "t@e.x"],
        &["commit", "-q", "--allow-empty", "-m", "one"],
    ] {
        let status = Command::new("git")
            .arg("-C")
            .arg(&work)
            .args(args)
            .status()
            .unwrap();
        assert!(status.success());
    }
    let mut app = project.app_with_form();
    let shown = text(&render(&mut app, 100, 30));
    assert!(!shown.contains("Initial commit"), "{shown}");
    assert!(shown.contains("after creating"));
}
