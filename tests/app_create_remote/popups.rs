//! The popups.

use std::fs;
use std::time::Duration;

use crate::support::{
    Project, Shown, clear_name, open_form, press, ready_app, shown, type_text, wait_for_created,
};
use ferrit::git::host::{GhProgram, Visibility};
use ferrit::tui::components::create_remote::Field;
use ferrit::tui::events::AppEvent;
use ratatui::crossterm::event::{KeyCode, KeyEvent};

#[test]
fn the_check_runs_off_the_ui_thread_then_the_form_opens_on_the_folder_name() {
    let project = Project::new("cr-form");
    let (mut app, rx) = ready_app(&project);
    open_form(&mut app, &rx);

    let Shown::Form {
        name,
        description,
        visibility,
        focus,
        error,
    } = shown(&app)
    else {
        panic!("a form")
    };
    assert_eq!(name, "work", "the folder's name");
    assert_eq!(description, "");
    assert_eq!(visibility, Visibility::Private, "private by default");
    assert_eq!(focus, Field::Name);
    assert_eq!(error, None);
    assert_eq!(project.calls(), ["--version", "auth status"]);
}

#[test]
fn a_missing_or_signed_out_gh_says_what_to_do_and_shows_no_form() {
    let project = Project::new("cr-nogh");
    let mut app = project.app();
    app.set_gh_program(GhProgram::new("/nonexistent/ferrit-test/gh"));
    app.open_create_remote();
    let Shown::Note(message) = shown(&app) else {
        panic!("a note")
    };
    assert!(
        message.contains("gh is required: https://cli.github.com"),
        "{message}"
    );

    let project = Project::new("cr-signedout");
    fs::write(project.dir.join("signed-out"), "").unwrap();
    let mut app = project.app();
    app.open_create_remote();
    let Shown::Note(message) = shown(&app) else {
        panic!("a note")
    };
    assert!(message.contains("gh auth login"), "{message}");
    assert_eq!(project.remotes(), "");
}

#[test]
fn closing_while_gh_is_checked_ignores_the_late_answer() {
    let project = Project::new("cr-late");
    let (mut app, rx) = ready_app(&project);
    app.open_create_remote();
    press(&mut app, KeyCode::Esc);
    assert_eq!(shown(&app), Shown::Nothing);
    match rx.recv_timeout(Duration::from_secs(20)) {
        Ok(event @ AppEvent::GhChecked { .. }) => app.deliver_event(event),
        other => panic!("expected GhChecked, got {other:?}"),
    }
    assert_eq!(
        shown(&app),
        Shown::Nothing,
        "no form pops up behind the user's back"
    );
}

#[test]
fn a_repository_that_has_a_remote_gets_a_note_and_no_check() {
    let project = Project::new("cr-has-remote");
    project.git(&["remote", "add", "upstream", "https://example.com/x.git"]);
    let (mut app, rx) = ready_app(&project);
    app.open_create_remote();
    assert_eq!(
        shown(&app),
        Shown::Note("this repository already has a remote".to_owned())
    );
    assert!(rx.try_recv().is_err());
    assert!(project.calls().is_empty());
}

#[test]
fn the_form_edits_its_three_fields_with_tab_arrows_and_space() {
    let project = Project::new("cr-keys");
    let (mut app, rx) = ready_app(&project);
    open_form(&mut app, &rx);

    clear_name(&mut app);
    type_text(&mut app, "acme/tool");
    press(&mut app, KeyCode::Tab);
    press(&mut app, KeyCode::Char(' '));
    press(&mut app, KeyCode::Tab);
    type_text(&mut app, "A small tool");
    assert_eq!(
        shown(&app),
        Shown::Form {
            name: "acme/tool".to_owned(),
            description: "A small tool".to_owned(),
            visibility: Visibility::Public,
            focus: Field::Description,
            error: None,
        }
    );
    // Tab wraps round to the name; Shift-Tab goes back; the arrows flip the visibility.
    press(&mut app, KeyCode::Tab);
    press(&mut app, KeyCode::BackTab);
    press(&mut app, KeyCode::BackTab);
    press(&mut app, KeyCode::Right);
    let Shown::Form {
        visibility, focus, ..
    } = shown(&app)
    else {
        panic!("a form")
    };
    assert_eq!(
        (visibility, focus),
        (Visibility::Private, Field::Visibility)
    );
}

#[test]
fn a_bad_name_stays_on_the_form_with_the_reason_until_it_is_edited() {
    let project = Project::new("cr-invalid");
    let (mut app, rx) = ready_app(&project);
    open_form(&mut app, &rx);
    clear_name(&mut app);
    type_text(&mut app, "my repo");
    press(&mut app, KeyCode::Enter);

    let Shown::Form { error, .. } = shown(&app) else {
        panic!("still the form")
    };
    assert!(error.unwrap().contains("not allowed in a name"));
    type_text(&mut app, "x");
    let Shown::Form { error, .. } = shown(&app) else {
        panic!("a form")
    };
    assert_eq!(error, None, "typing clears the reason");
}

#[test]
fn the_last_question_names_what_will_happen_and_a_private_one_takes_enter() {
    let project = Project::new("cr-confirm");
    let (mut app, rx) = ready_app(&project);
    open_form(&mut app, &rx);
    clear_name(&mut app);
    type_text(&mut app, "tool");
    press(&mut app, KeyCode::Enter);

    let Shown::Confirm {
        title,
        visibility,
        lines,
    } = shown(&app)
    else {
        panic!("the question")
    };
    assert_eq!(title, "Create tool");
    assert_eq!(visibility, Visibility::Private);
    assert_eq!(lines[0], "PRIVATE repository");
    assert_eq!(lines[1], "first: commit an empty README.md, made by Ferrit");
    assert!(
        lines[2].starts_with("then: add remote `origin`, push "),
        "{lines:?}"
    );
    assert_eq!(project.calls().len(), 2, "only the two status reads so far");

    press(&mut app, KeyCode::Enter);
    assert_eq!(shown(&app), Shown::Nothing);
    assert_eq!(app.remote_busy_label(), Some("Creating repository\u{2026}"));
    wait_for_created(&mut app, &rx);
    assert_eq!(project.remotes(), "origin");
}

#[test]
fn a_public_repository_is_confirmed_by_y_alone_and_enter_does_nothing() {
    let project = Project::new("cr-public-confirm");
    let (mut app, rx) = ready_app(&project);
    open_form(&mut app, &rx);
    press(&mut app, KeyCode::Tab);
    press(&mut app, KeyCode::Char(' '));
    press(&mut app, KeyCode::Enter);

    let Shown::Confirm {
        visibility, lines, ..
    } = shown(&app)
    else {
        panic!("the question")
    };
    assert_eq!(visibility, Visibility::Public);
    assert_eq!(lines[0], "PUBLIC repository");
    assert_eq!(lines[1], "Everyone can read its history.");

    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Char(' '));
    assert!(
        matches!(shown(&app), Shown::Confirm { .. }),
        "Enter and Space are not a yes"
    );
    assert!(app.remote_busy_label().is_none());
    assert!(project.calls().iter().all(|c| !c.starts_with("repo ")));

    app.feed_key(KeyEvent::from(KeyCode::Char('y')));
    assert_eq!(app.remote_busy_label(), Some("Creating repository\u{2026}"));
    wait_for_created(&mut app, &rx);
    assert!(
        project
            .calls()
            .iter()
            .any(|c| c.starts_with("repo create work --public "))
    );
}

#[test]
fn n_and_esc_at_the_question_go_back_to_the_form_with_its_fields() {
    let project = Project::new("cr-back");
    let (mut app, rx) = ready_app(&project);
    open_form(&mut app, &rx);
    clear_name(&mut app);
    type_text(&mut app, "tool");
    for key in [KeyCode::Char('n'), KeyCode::Esc] {
        press(&mut app, KeyCode::Enter);
        assert!(matches!(shown(&app), Shown::Confirm { .. }));
        press(&mut app, key);
        let Shown::Form { name, .. } = shown(&app) else {
            panic!("the form")
        };
        assert_eq!(name, "tool");
    }
}

#[test]
fn cancelling_at_any_step_leaves_no_remote_and_gh_untouched_beyond_its_checks() {
    let project = Project::new("cr-cancel");
    let (mut app, rx) = ready_app(&project);
    open_form(&mut app, &rx);
    press(&mut app, KeyCode::Esc);
    assert_eq!(shown(&app), Shown::Nothing);

    // The typed fields are remembered for the next time.
    app.open_create_remote();
    match rx.recv_timeout(Duration::from_secs(20)) {
        Ok(event) => app.deliver_event(event),
        other => panic!("expected GhChecked, got {other:?}"),
    }
    press(&mut app, KeyCode::Enter);
    assert!(matches!(shown(&app), Shown::Confirm { .. }));
    press(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::Esc);
    assert_eq!(shown(&app), Shown::Nothing);

    assert_eq!(project.remotes(), "");
    assert!(
        project
            .calls()
            .iter()
            .all(|c| c == "--version" || c == "auth status"),
        "{:?}",
        project.calls()
    );
}

#[test]
fn a_refusal_reopens_the_form_on_the_name_with_every_field_kept() {
    let project = Project::new("cr-reopen");
    fs::write(project.dir.join("name-taken"), "").unwrap();
    let (mut app, rx) = ready_app(&project);
    open_form(&mut app, &rx);
    clear_name(&mut app);
    type_text(&mut app, "tool");
    press(&mut app, KeyCode::Tab);
    press(&mut app, KeyCode::Tab);
    type_text(&mut app, "my words");
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Enter);
    wait_for_created(&mut app, &rx);

    let Shown::Form {
        name,
        description,
        focus,
        error,
        ..
    } = shown(&app)
    else {
        panic!("the form is back")
    };
    assert_eq!((name.as_str(), description.as_str()), ("tool", "my words"));
    assert_eq!(focus, Field::Name);
    assert!(
        error
            .unwrap()
            .contains("Name already exists on this account")
    );
    assert_eq!(project.remotes(), "");
}
