#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "integration test scaffolding: a failed setup is the assertion"
)]
//! The background creation of the GitHub repository (`docs/PLAN_15_CREATE_REMOTE.md`,
//! R2): the slot, the busy label, the answer. No popups yet. `gh` is a fake
//! script, so nothing here reaches GitHub.

use std::fmt::Write as _;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc;
use std::time::Duration;

use ferrit::app::create_remote::{CreateDraft, CreateRemoteView, Field};
use ferrit::app::events::{AppEvent, RemoteOp};
use ferrit::app::{App, PopupView};
use ferrit::domain::git::host::{GhProgram, Visibility};
use ratatui::crossterm::event::{KeyCode, KeyEvent};

/// A repository and a fake `gh` that logs its calls, adds `origin` like the
/// real one and answers with the web URL, or fails while `name-taken` exists.
struct Project {
    dir: PathBuf,
    work: PathBuf,
}

impl Project {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = fs::canonicalize({
            let dir =
                std::env::temp_dir().join(format!("ferrit-{tag}-{}-{nanos}", std::process::id()));
            fs::create_dir_all(&dir).unwrap();
            dir
        })
        .unwrap();
        let work = dir.join("work");
        fs::create_dir_all(&work).unwrap();
        let project = Self { dir, work };
        project.git(&["init", "-q", "."]);
        // A local identity: the first commit must not depend on the machine's.
        project.git(&["config", "user.name", "Test Author"]);
        project.git(&["config", "user.email", "test@example.com"]);
        let script = project.dir.join("gh");
        fs::write(
            &script,
            "#!/bin/sh\n\
             here=$(dirname \"$0\")\n\
             echo \"$@\" >> \"$here/calls.log\"\n\
             case \"$1\" in\n\
               --version) echo 'gh version 2.50.0'; exit 0 ;;\n\
               auth) [ -f \"$here/signed-out\" ] && exit 1; exit 0 ;;\n\
               repo)\n\
                 [ -f \"$here/sleep\" ] && sleep 2\n\
                 if [ -f \"$here/name-taken\" ]; then echo 'Name already exists on this account' >&2; exit 1; fi\n\
                 target=\"$3\"\n\
                 while [ $# -gt 0 ]; do [ \"$1\" = --source ] && src=\"$2\"; shift; done\n\
                 url=$(cat \"$here/origin-url\" 2>/dev/null || echo \"https://github.com/$target.git\")\n\
                 git -C \"$src\" remote add origin \"$url\"\n\
                 case \"$target\" in */*) path=\"$target\" ;; *) path=\"fake-user/$target\" ;; esac\n\
                 echo \"https://github.com/$path\"; exit 0 ;;\n\
             esac\n\
             exit 0\n",
        )
        .unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        // Every creation pushes: a local bare repository is the default URL the
        // fake gh gives `origin`, so no test reaches a network.
        let _bare = project.bare_origin();
        project
    }

    fn git(&self, args: &[&str]) -> String {
        let out = Command::new("git")
            .arg("-C")
            .arg(&self.work)
            .args(args)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    }

    fn app(&self) -> App {
        let mut app = App::open(&self.work).unwrap();
        app.set_gh_program(GhProgram::new(self.dir.join("gh")));
        // Never the user's own ~/.ssh/config: it would pick the host.
        app.set_ssh_config_path(self.dir.join("no-ssh-config"));
        app
    }

    fn calls(&self) -> Vec<String> {
        fs::read_to_string(self.dir.join("calls.log"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    fn remotes(&self) -> String {
        self.git(&["remote"])
    }

    /// Where `origin` points, if it exists.
    fn remote_url(&self) -> Option<String> {
        let out = Command::new("git")
            .arg("-C")
            .arg(&self.work)
            .args(["remote", "get-url", "origin"])
            .output()
            .unwrap();
        out.status
            .success()
            .then(|| String::from_utf8_lossy(&out.stdout).trim().to_owned())
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

/// Deliver events, refreshes included, until the creation's answer arrives.
fn wait_for_created(app: &mut App, rx: &mpsc::Receiver<AppEvent>) {
    loop {
        match rx.recv_timeout(Duration::from_secs(20)) {
            Ok(event @ AppEvent::RemoteCreated(_)) => {
                app.deliver_event(event);
                return;
            },
            Ok(other) => app.deliver_event(other),
            Err(error) => panic!("no RemoteCreated: {error}"),
        }
    }
}

fn draft(target: &str) -> CreateDraft {
    CreateDraft::new(target.to_owned())
}

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

// ---------------------------------------------------------------- the popups

fn press(app: &mut App, code: KeyCode) {
    app.feed_key(KeyEvent::from(code));
}

fn type_text(app: &mut App, text: &str) {
    for c in text.chars() {
        app.feed_key(KeyEvent::from(KeyCode::Char(c)));
    }
}

/// What the create popup shows, flattened to text for an assertion.
#[derive(Debug, PartialEq, Eq)]
enum Shown {
    Nothing,
    Checking,
    Form {
        name: String,
        description: String,
        visibility: Visibility,
        focus: Field,
        error: Option<String>,
    },
    Confirm {
        title: String,
        visibility: Visibility,
        lines: Vec<String>,
    },
    Note(String),
    Menu(Vec<String>),
}

fn shown(app: &mut App) -> Shown {
    match app.popup_view() {
        None => Shown::Nothing,
        Some(PopupView::Note(message)) => Shown::Note(message.to_owned()),
        Some(PopupView::Menu(menu)) => Shown::Menu(menu.rows),
        Some(PopupView::CreateRemote(view)) => match view {
            CreateRemoteView::Checking => Shown::Checking,
            CreateRemoteView::Form(form) => Shown::Form {
                name: form.name.text(),
                description: form.description.text(),
                visibility: form.visibility,
                focus: form.focus,
                error: form.error.map(str::to_owned),
            },
            CreateRemoteView::Confirm(confirm) => Shown::Confirm {
                title: confirm.title,
                visibility: confirm.visibility,
                lines: confirm.lines,
            },
        },
        Some(_) => panic!("another popup"),
    }
}

fn clear_name(app: &mut App) {
    for _ in 0..80 {
        press(app, KeyCode::Backspace);
    }
}

/// An app on a project with a ready `gh` and a way to receive events.
fn ready_app(project: &Project) -> (App, mpsc::Receiver<AppEvent>) {
    let mut app = project.app();
    let (tx, rx) = mpsc::channel();
    app.set_event_sender(tx);
    (app, rx)
}

/// Deliver the `gh` check's answer and open the form.
fn open_form(app: &mut App, rx: &mpsc::Receiver<AppEvent>) {
    app.open_create_remote();
    assert_eq!(shown(app), Shown::Checking);
    match rx.recv_timeout(Duration::from_secs(20)) {
        Ok(event @ AppEvent::GhChecked { .. }) => app.deliver_event(event),
        other => panic!("expected GhChecked, got {other:?}"),
    }
}

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
    } = shown(&mut app)
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
    let Shown::Note(message) = shown(&mut app) else {
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
    let Shown::Note(message) = shown(&mut app) else {
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
    assert_eq!(shown(&mut app), Shown::Nothing);
    match rx.recv_timeout(Duration::from_secs(20)) {
        Ok(event @ AppEvent::GhChecked { .. }) => app.deliver_event(event),
        other => panic!("expected GhChecked, got {other:?}"),
    }
    assert_eq!(
        shown(&mut app),
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
        shown(&mut app),
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
        shown(&mut app),
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
    } = shown(&mut app)
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

    let Shown::Form { error, .. } = shown(&mut app) else {
        panic!("still the form")
    };
    assert!(error.unwrap().contains("not allowed in a name"));
    type_text(&mut app, "x");
    let Shown::Form { error, .. } = shown(&mut app) else {
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
    } = shown(&mut app)
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
    assert_eq!(shown(&mut app), Shown::Nothing);
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
    } = shown(&mut app)
    else {
        panic!("the question")
    };
    assert_eq!(visibility, Visibility::Public);
    assert_eq!(lines[0], "PUBLIC repository");
    assert_eq!(lines[1], "Everyone can read its history.");

    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Char(' '));
    assert!(
        matches!(shown(&mut app), Shown::Confirm { .. }),
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
        assert!(matches!(shown(&mut app), Shown::Confirm { .. }));
        press(&mut app, key);
        let Shown::Form { name, .. } = shown(&mut app) else {
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
    assert_eq!(shown(&mut app), Shown::Nothing);

    // The typed fields are remembered for the next time.
    app.open_create_remote();
    match rx.recv_timeout(Duration::from_secs(20)) {
        Ok(event) => app.deliver_event(event),
        other => panic!("expected GhChecked, got {other:?}"),
    }
    press(&mut app, KeyCode::Enter);
    assert!(matches!(shown(&mut app), Shown::Confirm { .. }));
    press(&mut app, KeyCode::Esc);
    press(&mut app, KeyCode::Esc);
    assert_eq!(shown(&mut app), Shown::Nothing);

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
    } = shown(&mut app)
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

// ------------------------------------------------------------- the x menu

const CREATE_ROW: &str = "Create a repository on GitHub  (g)";

#[test]
fn x_on_status_with_no_remote_offers_to_create_the_repository() {
    let project = Project::new("cr-menu-status");
    let mut app = project.app();
    app.feed_key(KeyEvent::from(KeyCode::Char('1')));
    app.feed_key(KeyEvent::from(KeyCode::Char('x')));
    assert_eq!(shown(&mut app), Shown::Menu(vec![CREATE_ROW.to_owned()]));

    // The row's letter, or Enter, starts the flow: the gh check, then the form.
    app.feed_key(KeyEvent::from(KeyCode::Char('g')));
    assert!(matches!(shown(&mut app), Shown::Form { .. }));
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
    let Shown::Menu(rows) = shown(&mut app) else {
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
    assert_eq!(
        shown(&mut app),
        Shown::Nothing,
        "nothing to offer on Status"
    );

    app.feed_key(KeyEvent::from(KeyCode::Char('3')));
    app.feed_key(KeyEvent::from(KeyCode::Char('x')));
    if let Shown::Menu(rows) = shown(&mut app) {
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
        shown(&mut app),
        Shown::Note("another network operation is running".to_owned())
    );
    drop(rx);
}

// ------------------------------------------------- the push after creating

impl Project {
    fn commit(&self) {
        fs::write(self.work.join("a.txt"), "a").unwrap();
        self.git(&["add", "-A"]);
        self.git(&[
            "-c",
            "user.name=T",
            "-c",
            "user.email=t@e.x",
            "commit",
            "-q",
            "-m",
            "one",
        ]);
    }

    /// A bare repository the fake `gh` will set as `origin`'s URL.
    fn bare_origin(&self) -> PathBuf {
        let bare = self.dir.join("remote.git");
        let status = Command::new("git")
            .args(["init", "-q", "--bare"])
            .arg(&bare)
            .status()
            .unwrap();
        assert!(status.success());
        fs::write(self.dir.join("origin-url"), bare.display().to_string()).unwrap();
        bare
    }

    fn branch(&self) -> String {
        self.git(&["rev-parse", "--abbrev-ref", "HEAD"])
    }
}

fn bare_refs(bare: &std::path::Path) -> String {
    let out = Command::new("git")
        .arg("--git-dir")
        .arg(bare)
        .args(["for-each-ref", "--format=%(refname:short)"])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

/// Deliver events, refreshes included, until the push's `RemoteDone` arrives.
fn wait_for_push(app: &mut App, rx: &mpsc::Receiver<AppEvent>) {
    loop {
        match rx.recv_timeout(Duration::from_secs(30)) {
            Ok(event @ AppEvent::RemoteDone { .. }) => {
                app.deliver_event(event);
                return;
            },
            Ok(other) => app.deliver_event(other),
            Err(error) => panic!("no RemoteDone: {error}"),
        }
    }
}

fn status_text(app: &App) -> String {
    app.status_lines()
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

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
    app.on_remote_done(RemoteOp::Push, Err("plain failure".to_owned()));
    let shown = status_text(&app);
    assert!(shown.contains("plain failure"), "{shown}");
    assert!(!shown.contains("P retries"), "{shown}");
}

// ------------------------------------------------------------ the G key

fn g(app: &mut App) {
    app.feed_key(KeyEvent::from(KeyCode::Char('G')));
}

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

// ------------------------------------------------- the first commit

impl Project {
    fn commit_count(&self) -> String {
        let out = Command::new("git")
            .arg("-C")
            .arg(&self.work)
            .args(["rev-list", "--count", "HEAD"])
            .output()
            .unwrap();
        if out.status.success() {
            String::from_utf8_lossy(&out.stdout).trim().to_owned()
        } else {
            "0".to_owned()
        }
    }
}

fn bare_tree(bare: &std::path::Path, branch: &str) -> String {
    let out = Command::new("git")
        .arg("--git-dir")
        .arg(bare)
        .args(["ls-tree", "-r", "--name-only", branch])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

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

// ------------------------------------------------------- the SSH host

impl Project {
    /// An ssh config next to the project, with these `Host` aliases for GitHub.
    fn ssh_config(&self, aliases: &[&str]) -> PathBuf {
        let path = self.dir.join("ssh_config");
        let mut text = String::new();
        for alias in aliases {
            let _ = write!(text, "Host {alias}\n  HostName github.com\n  User git\n");
        }
        fs::write(&path, text).unwrap();
        path
    }
}

#[test]
fn the_last_question_names_the_users_github_alias_from_their_ssh_config() {
    let project = Project::new("cr-host-default");
    let mut app = project.app();
    app.set_ssh_config_path(project.ssh_config(&["github.com-personal", "github.com-work"]));
    app.open_create_remote();
    press(&mut app, KeyCode::Enter);
    let Shown::Confirm { lines, .. } = shown(&mut app) else {
        panic!("the question")
    };
    assert!(
        lines
            .iter()
            .any(|l| l.contains("add remote `origin` over ssh host github.com-personal")),
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
    let logged = ferrit::domain::git::command_log::recent(usize::MAX, true)
        .iter()
        .any(|r| {
            r.argv
                .ends_with("remote set-url -- origin git@my-alias:acme/tool.git")
                && r.exit == Some(0)
        });
    assert!(logged, "git remote set-url is logged");
}

impl Project {
    /// A bare repository that `git@<host>:<path>.git` is rewritten to (git's own
    /// `insteadOf`), so a push to the alias URL lands there with no network.
    fn bare_origin_for_alias(&self, host: &str, path: &str) -> PathBuf {
        let bare = self.dir.join("alias-remote.git");
        let status = Command::new("git")
            .args(["init", "-q", "--bare"])
            .arg(&bare)
            .status()
            .unwrap();
        assert!(status.success());
        self.git(&[
            "config",
            &format!("url.{}.insteadOf", bare.display()),
            &format!("git@{host}:{path}.git"),
        ]);
        bare
    }
}
