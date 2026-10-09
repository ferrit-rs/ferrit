//! The ssh host.

use std::sync::mpsc;

use crate::support::{
    Project, Shown, bare_refs, clear_name, draft, open_form, press, ready_app, shown, type_text,
    wait_for_created, wait_for_push,
};
use ratatui::crossterm::event::KeyCode;

#[test]
fn the_last_question_names_the_users_github_alias_from_their_ssh_config() {
    let project = Project::new("cr-host-default");
    let mut app = project.app();
    app.set_ssh_config_path(project.ssh_config(&["github.com-personal", "github.com-work"]));
    app.open_create_remote();
    press(&mut app, KeyCode::Enter);
    let Shown::Confirm { lines, .. } = shown(&app) else {
        panic!("the question")
    };
    assert!(
        lines
            .iter()
            .any(|l| l.contains("add remote `origin` using your SSH key for github.com-personal")),
        "the first alias: {lines:?}"
    );
}

#[test]
fn origin_is_rewritten_over_the_alias_and_the_push_reaches_it() {
    let project = Project::new("cr-host-push");
    project.commit();
    let bare = project.bare_origin_for_alias("my-alias", "acme/tool");
    let (mut app, rx) = ready_app(&project);
    app.set_ssh_config_path(project.ssh_config(&["my-alias"]));
    open_form(&mut app, &rx);
    clear_name(&mut app);
    type_text(&mut app, "acme/tool");
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Enter);
    wait_for_created(&mut app, &rx);
    wait_for_push(&mut app, &rx);

    // The raw value: `git remote get-url` would show the `insteadOf` rewrite.
    assert_eq!(
        project.git(&["config", "--get", "remote.origin.url"]),
        "git@my-alias:acme/tool.git"
    );
    let branch = project.branch();
    assert_eq!(
        bare_refs(&bare),
        branch,
        "the push went through the alias URL"
    );
    assert_eq!(
        project.git(&["rev-parse", "--abbrev-ref", "@{u}"]),
        format!("origin/{branch}")
    );
}

#[test]
fn with_no_alias_in_the_ssh_config_origin_keeps_the_url_gh_wrote() {
    let project = Project::new("cr-host-none");
    let bare = project.bare_origin();
    let mut app = project.app();
    let (tx, rx) = mpsc::channel();
    app.set_event_sender(tx.clone());
    app.start_create_remote(draft("acme/tool"), tx);
    wait_for_created(&mut app, &rx);
    wait_for_push(&mut app, &rx);
    assert_eq!(
        project.remote_url().as_deref(),
        Some(bare.to_str().unwrap())
    );
}

#[test]
fn the_rewrite_of_origin_is_in_the_command_log() {
    let project = Project::new("cr-host-log");
    project.bare_origin_for_alias("my-alias", "acme/tool");
    let mut app = project.app();
    app.set_ssh_config_path(project.ssh_config(&["my-alias"]));
    let (tx, rx) = mpsc::channel();
    app.set_event_sender(tx.clone());
    app.start_create_remote(draft("acme/tool"), tx);
    wait_for_created(&mut app, &rx);
    wait_for_push(&mut app, &rx);
    let logged = ferrit::git::command_log::recent(usize::MAX, true)
        .iter()
        .any(|r| {
            r.argv
                .ends_with("remote set-url -- origin git@my-alias:acme/tool.git")
                && r.exit == Some(0)
        });
    assert!(logged, "git remote set-url is logged");
}
