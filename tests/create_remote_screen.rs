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

use ferrit::git::host::GhProgram;
use ferrit::tui::App;
use ferrit::tui::screens as ui;
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

    /// The same, reading the ssh host aliases from `config`.
    fn app_with_form_over(&self, config: &std::path::Path) -> App {
        let mut app = App::open(&self.dir.join("work")).unwrap();
        app.set_gh_program(GhProgram::new(self.dir.join("gh")));
        app.set_ssh_config_path(config.to_path_buf());
        app.open_create_remote();
        app
    }

    /// An app with the check answered: the form is up.
    fn app_with_form(&self) -> App {
        let mut app = App::open(&self.dir.join("work")).unwrap();
        app.set_gh_program(GhProgram::new(self.dir.join("gh")));
        app.set_ssh_config_path(self.dir.join("no-ssh-config"));
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
fn the_form_has_three_choices_and_nothing_else() {
    let project = Project::new("crs-form");
    let mut app = project.app_with_form();
    let shown = text(&render(&mut app, 100, 40));

    assert!(shown.contains("Create on GitHub"), "{shown}");
    for label in ["Name", "Visibility", "Description"] {
        assert!(shown.contains(label), "{label} in {shown}");
    }
    assert!(shown.contains("work"), "the folder's name is the default");
    assert!(shown.contains("(\u{2022}) private"), "{shown}");
    assert!(shown.contains("( ) public"), "{shown}");
    // Not choices: the first commit, the push and the SSH host happen on their own.
    for gone in ["Initial commit", "SSH host", "after creating", "[x]", "[ ]"] {
        assert!(!shown.contains(gone), "{gone} in {shown}");
    }
    assert!(shown.contains("Next: Tab"), "{shown}");
}

#[test]
fn the_text_fields_are_framed_boxes_with_a_counter_like_the_commit_popup() {
    let project = Project::new("crs-boxes");
    let mut app = project.app_with_form();
    let shown = text(&render(&mut app, 100, 40));
    assert!(shown.contains("\u{256d} Name "), "{shown}");
    assert!(shown.contains("\u{256d} Description "), "{shown}");
    assert!(
        shown.contains("4/100"),
        "the folder's name is 4 characters: {shown}"
    );
    assert!(shown.contains("0/350"), "{shown}");
}

#[test]
fn a_long_description_wraps_over_the_box_and_all_of_it_stays_in_view() {
    let project = Project::new("crs-wrap");
    let mut app = project.app_with_form();
    press(&mut app, KeyCode::Tab);
    press(&mut app, KeyCode::Tab);
    let words: Vec<String> = (1..=45).map(|i| format!("word{i:02}")).collect();
    let description = words.join(" ");
    assert!(description.chars().count() > 250, "longer than one row");
    for c in description.chars() {
        app.feed_key(KeyEvent::from(KeyCode::Char(c)));
    }
    let shown = text(&render(&mut app, 100, 40));
    assert!(shown.contains("word01"), "the start: {shown}");
    assert!(shown.contains("word45"), "the end, not cut off: {shown}");
    assert!(
        shown.contains(&format!("{}/350", description.chars().count())),
        "the counter: {shown}"
    );
    // It really wraps: the text sits on several rows of the box.
    let rows_with_words = shown.lines().filter(|l| l.contains("word")).count();
    assert!(rows_with_words >= 4, "{rows_with_words} rows: {shown}");
}

#[test]
fn a_long_description_stays_in_view_on_a_smaller_terminal_too() {
    let project = Project::new("crs-wrap-small");
    let mut app = project.app_with_form();
    press(&mut app, KeyCode::Tab);
    press(&mut app, KeyCode::Tab);
    for c in "alpha beta gamma delta epsilon zeta eta theta iota kappa lambda mu nu xi omicron pi rho sigma tau upsilon last".chars() {
        app.feed_key(KeyEvent::from(KeyCode::Char(c)));
    }
    let shown = text(&render(&mut app, 80, 24));
    assert!(shown.contains("alpha") && shown.contains("last"), "{shown}");
}

#[test]
fn a_long_name_stays_in_view_in_its_box() {
    let project = Project::new("crs-wrap-name");
    let mut app = project.app_with_form();
    for _ in 0..30 {
        press(&mut app, KeyCode::Backspace);
    }
    let name = format!("{}-end", "n".repeat(90));
    for c in name.chars() {
        app.feed_key(KeyEvent::from(KeyCode::Char(c)));
    }
    let shown = text(&render(&mut app, 100, 40));
    assert!(shown.contains("-end"), "the end of the name: {shown}");
    assert!(
        shown.contains(&format!("{}/100", name.chars().count())),
        "{shown}"
    );
}

#[test]
fn the_last_question_says_the_first_commit_is_made_by_ferrit_only_when_there_is_none() {
    let project = Project::new("crs-first");
    let mut app = project.app_with_form();
    press(&mut app, KeyCode::Enter);
    let shown = text(&render(&mut app, 100, 30));
    assert!(
        shown.contains("first: commit an empty README.md, made by Ferrit"),
        "{shown}"
    );
    assert!(shown.contains("add remote `origin`"), "{shown}");

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
    press(&mut app, KeyCode::Enter);
    let shown = text(&render(&mut app, 100, 30));
    assert!(!shown.contains("first: commit"), "{shown}");
}

#[test]
fn the_last_question_names_the_users_ssh_alias_and_is_wide_enough_for_it() {
    let project = Project::new("crs-host");
    let config = project.dir.join("ssh_config");
    fs::write(&config, "Host github.com-personal\n  HostName github.com\n").unwrap();
    let mut app = project.app_with_form_over(&config);
    press(&mut app, KeyCode::Enter);
    let shown = text(&render(&mut app, 100, 30));
    assert!(
        shown.contains("using your SSH key for github.com-personal"),
        "{shown}"
    );

    let mut app = project.app_with_form();
    press(&mut app, KeyCode::Enter);
    assert!(!text(&render(&mut app, 100, 30)).contains("using your SSH key"));
}
