//! The g key.

use std::fs;
use std::sync::mpsc;

use crate::support::{Project, Shown, clear_name, draft, g, ready_app, shown};
use ratatui::crossterm::event::{KeyCode, KeyEvent};

#[test]
fn g_opens_the_creation_from_any_pane_when_there_is_no_remote() {
    let project = Project::new("cr-g");
    for pane in ['1', '2', '3', '4', '5'] {
        let mut app = project.app();
        app.feed_key(KeyEvent::from(KeyCode::Char(pane)));
        g(&mut app);
        assert!(matches!(shown(&mut app), Shown::Form { .. }), "pane {pane}");
    }
    assert_eq!(project.remotes(), "");
}

#[test]
fn g_with_a_remote_says_so_and_checks_nothing() {
    let project = Project::new("cr-g-remote");
    project.git(&["remote", "add", "upstream", "https://example.com/x.git"]);
    let mut app = project.app();
    g(&mut app);
    assert_eq!(
        shown(&mut app),
        Shown::Note("this repository already has a remote".to_owned())
    );
    assert!(project.calls().is_empty(), "gh was not even asked");
}

#[test]
fn in_the_form_g_is_a_letter_and_while_busy_it_waits() {
    let project = Project::new("cr-g-typed");
    let mut app = project.app();
    g(&mut app);
    clear_name(&mut app);
    g(&mut app);
    let Shown::Form { name, .. } = shown(&mut app) else {
        panic!("the form")
    };
    assert_eq!(name, "G", "typed into the name, not a second creation");

    let project = Project::new("cr-g-busy");
    fs::write(project.dir.join("sleep"), "").unwrap();
    let (mut app, rx) = ready_app(&project);
    let (tx, _own) = mpsc::channel();
    app.start_create_remote(draft("tool"), tx);
    g(&mut app);
    assert_eq!(
        shown(&mut app),
        Shown::Note("another network operation is running".to_owned())
    );
    drop(rx);
}
