//! Creating.

use std::fs;
use std::sync::mpsc;

use crate::support::{Project, draft, wait_for_created, wait_for_push};
use ferrit::app::events::RemoteOp;
use ferrit::git::host::Visibility;

#[test]
fn a_creation_is_busy_then_reports_the_url_pushes_and_refreshes() {
    let project = Project::new("cr-ok");
    let mut app = project.app();
    let (tx, rx) = mpsc::channel();
    app.set_event_sender(tx.clone());

    app.start_create_remote(draft("acme/tool"), tx);
    assert_eq!(app.remote_busy_label(), Some("Creating repository\u{2026}"));
    wait_for_created(&mut app, &rx);
    // The push follows on its own: pushing is not a choice.
    assert_eq!(app.remote_busy_label(), Some("Pushing\u{2026}"));
    wait_for_push(&mut app, &rx);

    assert!(app.remote_busy_label().is_none());
    assert_eq!(
        app.create_remote().web_url.as_deref(),
        Some("https://github.com/acme/tool")
    );
    assert!(
        app.create_remote().draft.is_none(),
        "a success drops the draft"
    );
    assert_eq!(project.remotes(), "origin");
    let calls = project.calls();
    assert_eq!(calls.len(), 1);
    assert!(
        calls[0].starts_with("repo create acme/tool --private --source "),
        "{}",
        calls[0]
    );
    assert!(!calls[0].contains("--push"));
}

#[test]
fn a_public_draft_runs_gh_with_public() {
    let project = Project::new("cr-public");
    let mut app = project.app();
    let (tx, rx) = mpsc::channel();
    let mut public = draft("tool");
    public.visibility = Visibility::Public;
    app.start_create_remote(public, tx);
    wait_for_created(&mut app, &rx);
    assert!(project.calls()[0].starts_with("repo create tool --public "));
}

#[test]
fn a_refusal_keeps_the_draft_and_the_reason_and_configures_nothing() {
    let project = Project::new("cr-refused");
    fs::write(project.dir.join("name-taken"), "").unwrap();
    let mut app = project.app();
    let (tx, rx) = mpsc::channel();
    let mut typed = draft("tool");
    typed.description = "my words".to_owned();
    app.start_create_remote(typed.clone(), tx);
    wait_for_created(&mut app, &rx);

    assert!(app.remote_busy_label().is_none(), "the slot is free again");
    assert_eq!(app.create_remote().draft.as_ref(), Some(&typed));
    let reason = app.create_remote().error.as_deref().unwrap();
    assert!(
        reason.contains("Name already exists on this account"),
        "{reason}"
    );
    assert!(app.create_remote().web_url.is_none());
    assert_eq!(project.remotes(), "");
}

#[test]
fn a_bad_target_is_refused_at_once_and_gh_never_runs() {
    let project = Project::new("cr-bad");
    let mut app = project.app();
    let (tx, rx) = mpsc::channel();
    app.start_create_remote(draft("my repo"), tx);

    assert!(app.remote_busy_label().is_none());
    assert!(
        app.create_remote()
            .error
            .as_deref()
            .unwrap()
            .contains("not allowed in a name")
    );
    assert_eq!(
        app.create_remote()
            .draft
            .as_ref()
            .map(|d| d.target.as_str()),
        Some("my repo")
    );
    assert!(rx.try_recv().is_err());
    assert!(project.calls().is_empty());
}

#[test]
fn it_takes_the_slot_of_the_other_network_operations() {
    let project = Project::new("cr-slot");
    fs::write(project.dir.join("sleep"), "").unwrap();
    let mut app = project.app();
    let (tx, rx) = mpsc::channel();
    app.set_event_sender(tx.clone());
    app.start_create_remote(draft("tool"), tx.clone());
    assert_eq!(app.remote_busy_label(), Some("Creating repository\u{2026}"));

    // A fetch, a push or a second creation while it runs is ignored.
    app.start_remote_op(RemoteOp::Fetch, None, tx.clone());
    app.start_create_remote(draft("other"), tx);
    assert_eq!(app.remote_busy_label(), Some("Creating repository\u{2026}"));
    assert!(rx.try_recv().is_err());

    // The one creation answers once, for the first target only.
    wait_for_created(&mut app, &rx);
    wait_for_push(&mut app, &rx);
    assert!(rx.try_recv().is_err(), "no second answer");
    let calls = project.calls();
    assert_eq!(calls.len(), 1, "{calls:?}");
    assert!(calls[0].starts_with("repo create tool "), "{}", calls[0]);
}
