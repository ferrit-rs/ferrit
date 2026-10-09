//! The x menu.

use std::fs;
use std::sync::mpsc;

use crate::support::{CREATE_ROW, Project, Shown, draft, ready_app, shown};
use ratatui::crossterm::event::{KeyCode, KeyEvent};

#[test]
fn x_on_status_with_no_remote_offers_to_create_the_repository() {
    let project = Project::new("cr-menu-status");
    let mut app = project.app();
    app.feed_key(KeyEvent::from(KeyCode::Char('1')));
    app.feed_key(KeyEvent::from(KeyCode::Char('x')));
    assert_eq!(shown(&app), Shown::Menu(vec![CREATE_ROW.to_owned()]));

    // The row's letter, or Enter, starts the flow: the gh check, then the form.
    app.feed_key(KeyEvent::from(KeyCode::Char('g')));
    assert!(matches!(shown(&app), Shown::Form { .. }));
    assert_eq!(project.calls(), ["--version", "auth status"]);
}

#[test]
fn x_on_branches_adds_the_row_after_the_branch_actions() {
    let project = Project::new("cr-menu-branches");
    fs::write(project.work.join("a.txt"), "a").unwrap();
    project.git(&["add", "-A"]);
    project.git(&[
        "-c",
        "user.name=T",
        "-c",
        "user.email=t@e.x",
        "commit",
        "-q",
        "-m",
        "one",
    ]);
    let mut app = project.app();
    app.feed_key(KeyEvent::from(KeyCode::Char('3')));
    app.feed_key(KeyEvent::from(KeyCode::Char('x')));
    let Shown::Menu(rows) = shown(&app) else {
        panic!("a menu")
    };
    assert_eq!(rows.first().map(String::as_str), Some("Rename branch  (r)"));
    assert_eq!(rows.last().map(String::as_str), Some(CREATE_ROW));
}

#[test]
fn with_any_remote_the_row_is_not_there() {
    let project = Project::new("cr-menu-remote");
    project.git(&["remote", "add", "upstream", "https://example.com/x.git"]);
    let mut app = project.app();
    app.feed_key(KeyEvent::from(KeyCode::Char('1')));
    app.feed_key(KeyEvent::from(KeyCode::Char('x')));
    assert_eq!(shown(&app), Shown::Nothing, "nothing to offer on Status");

    app.feed_key(KeyEvent::from(KeyCode::Char('3')));
    app.feed_key(KeyEvent::from(KeyCode::Char('x')));
    if let Shown::Menu(rows) = shown(&app) {
        assert!(rows.iter().all(|r| r != CREATE_ROW), "{rows:?}");
    }
}

#[test]
fn the_row_waits_while_a_network_operation_runs() {
    let project = Project::new("cr-menu-busy");
    fs::write(project.dir.join("sleep"), "").unwrap();
    let (mut app, rx) = ready_app(&project);
    let (tx, _own) = mpsc::channel();
    app.start_create_remote(draft("tool"), tx);
    app.feed_key(KeyEvent::from(KeyCode::Char('1')));
    app.feed_key(KeyEvent::from(KeyCode::Char('x')));
    app.feed_key(KeyEvent::from(KeyCode::Char('g')));
    assert_eq!(
        shown(&app),
        Shown::Note("another network operation is running".to_owned())
    );
    drop(rx);
}
