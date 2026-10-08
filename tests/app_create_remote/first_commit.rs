//! The first commit.

use std::fs;
use std::os::unix::fs::PermissionsExt;

use crate::support::{
    Project, Shown, bare_tree, open_form, press, ready_app, shown, wait_for_created, wait_for_push,
};
use ratatui::crossterm::event::KeyCode;

#[test]
fn the_first_commit_the_remote_and_the_push_happen_in_one_go() {
    let project = Project::new("cr-first");
    let bare = project.bare_origin();
    let (mut app, rx) = ready_app(&project);
    open_form(&mut app, &rx);
    press(&mut app, KeyCode::Enter);
    let Shown::Confirm { lines, .. } = shown(&mut app) else {
        panic!("the question")
    };
    assert_eq!(lines[1], "first: commit an empty README.md, made by Ferrit");
    press(&mut app, KeyCode::Enter);
    wait_for_created(&mut app, &rx);
    wait_for_push(&mut app, &rx);

    let branch = project.branch();
    assert_eq!(project.commit_count(), "1");
    assert_eq!(project.git(&["log", "-1", "--format=%s"]), "Initial commit");
    assert_eq!(project.git(&["log", "-1", "--format=%an"]), "Test Author");
    assert_eq!(
        bare_tree(&bare, &branch),
        "README.md",
        "the remote has the file"
    );
    assert_eq!(
        project.git(&["cat-file", "-s", "HEAD:README.md"]),
        "0",
        "empty"
    );
    assert_eq!(
        project.git(&["rev-parse", "--abbrev-ref", "@{u}"]),
        format!("origin/{branch}")
    );
    let calls = project.calls();
    assert!(
        calls.iter().any(|c| c.starts_with("repo create ")),
        "{calls:?}"
    );
}

#[test]
fn with_a_commit_already_there_no_first_commit_is_announced_or_made() {
    let project = Project::new("cr-first-na");
    project.commit();
    let (mut app, rx) = ready_app(&project);
    open_form(&mut app, &rx);
    press(&mut app, KeyCode::Enter);
    let Shown::Confirm { lines, .. } = shown(&mut app) else {
        panic!("the question")
    };
    assert!(lines.iter().all(|l| !l.starts_with("first:")), "{lines:?}");
    press(&mut app, KeyCode::Enter);
    wait_for_created(&mut app, &rx);
    wait_for_push(&mut app, &rx);
    assert_eq!(project.commit_count(), "1");
    assert!(!project.work.join("README.md").exists(), "no file is added");
}

#[test]
fn a_failing_first_commit_stops_everything_before_anything_is_created() {
    let project = Project::new("cr-first-fails");
    let hook = project.work.join(".git/hooks/pre-commit");
    fs::write(&hook, "#!/bin/sh\necho 'refused by the hook' >&2\nexit 1\n").unwrap();
    fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    let (mut app, rx) = ready_app(&project);
    open_form(&mut app, &rx);
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Enter);
    wait_for_created(&mut app, &rx);

    let Shown::Form { error, .. } = shown(&mut app) else {
        panic!("the form is back")
    };
    let error = error.unwrap();
    assert!(
        error.contains("the first commit failed, nothing was created"),
        "{error}"
    );
    assert!(error.contains("refused by the hook"), "{error}");
    assert_eq!(project.remotes(), "");
    assert_eq!(project.commit_count(), "0");
    assert!(
        project.calls().iter().all(|c| !c.starts_with("repo ")),
        "gh never created anything"
    );
}

#[test]
fn a_refused_creation_keeps_the_commit_and_a_retry_does_not_make_a_second() {
    let project = Project::new("cr-first-retry");
    fs::write(project.dir.join("name-taken"), "").unwrap();
    let (mut app, rx) = ready_app(&project);
    open_form(&mut app, &rx);
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Enter);
    wait_for_created(&mut app, &rx);

    assert_eq!(project.commit_count(), "1", "the local commit stays");
    assert_eq!(project.remotes(), "");
    let Shown::Form { error, .. } = shown(&mut app) else {
        panic!("the form is back")
    };
    assert!(error.unwrap().contains("Name already exists"));

    fs::remove_file(project.dir.join("name-taken")).unwrap();
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Enter);
    wait_for_created(&mut app, &rx);
    wait_for_push(&mut app, &rx);
    assert_eq!(project.remotes(), "origin");
    assert_eq!(project.commit_count(), "1", "still one commit");
}
