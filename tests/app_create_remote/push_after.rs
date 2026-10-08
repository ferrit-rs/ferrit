//! The push after creating.

use std::fs;

use crate::support::{
    Project, bare_refs, open_form, press, ready_app, status_text, wait_for_created, wait_for_push,
};
use ferrit::app::events::RemoteOp;
use ferrit::domain::git::error::GitError;
use ratatui::crossterm::event::KeyCode;

#[test]
fn a_ticked_form_pushes_the_branch_and_sets_its_upstream() {
    let project = Project::new("cr-push");
    project.commit();
    let bare = project.bare_origin();
    let (mut app, rx) = ready_app(&project);
    open_form(&mut app, &rx);
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Enter);
    wait_for_created(&mut app, &rx);
    assert_eq!(
        app.remote_busy_label(),
        Some("Pushing\u{2026}"),
        "the push follows without a key"
    );
    wait_for_push(&mut app, &rx);

    let branch = project.branch();
    assert_eq!(bare_refs(&bare), branch);
    assert_eq!(
        project.git(&["rev-parse", "--abbrev-ref", "@{u}"]),
        format!("origin/{branch}")
    );
    assert_eq!(project.git(&["remote"]), "origin");
}

#[test]
fn with_a_detached_head_the_push_is_skipped_with_a_note() {
    let project = Project::new("cr-skip-detached");
    project.commit();
    project.git(&["checkout", "-q", "--detach"]);
    let (mut app, rx) = ready_app(&project);
    open_form(&mut app, &rx);
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Enter);
    wait_for_created(&mut app, &rx);
    assert!(app.remote_busy_label().is_none());
    assert!(
        status_text(&app).contains("HEAD is detached"),
        "{}",
        status_text(&app)
    );
}

#[test]
fn a_failed_push_says_the_repository_exists_and_that_p_retries() {
    let project = Project::new("cr-push-fails");
    project.commit();
    fs::write(
        project.dir.join("origin-url"),
        "/nonexistent/ferrit-test/x.git",
    )
    .unwrap();
    let (mut app, rx) = ready_app(&project);
    open_form(&mut app, &rx);
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Enter);
    wait_for_created(&mut app, &rx);
    wait_for_push(&mut app, &rx);

    let shown = status_text(&app);
    assert!(shown.contains("P retries the push"), "{shown}");
    assert!(
        shown.contains("https://github.com/fake-user/work"),
        "{shown}"
    );
    assert_eq!(project.remotes(), "origin", "the remote stays");
}

#[test]
fn an_ordinary_push_failure_later_is_not_dressed_as_a_creation() {
    let project = Project::new("cr-plain-push");
    let (mut app, _rx) = ready_app(&project);
    app.on_remote_done(
        RemoteOp::Push,
        Err(GitError::PushFailed("plain failure".to_owned()).into()),
    );
    let shown = status_text(&app);
    assert!(shown.contains("plain failure"), "{shown}");
    assert!(!shown.contains("P retries"), "{shown}");
}
