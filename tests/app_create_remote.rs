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

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc;
use std::time::Duration;

use ferrit::app::App;
use ferrit::app::create_remote::CreateDraft;
use ferrit::app::events::{AppEvent, RemoteOp};
use ferrit::domain::git::host::{GhProgram, Visibility};

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
        let script = project.dir.join("gh");
        fs::write(
            &script,
            "#!/bin/sh\n\
             here=$(dirname \"$0\")\n\
             echo \"$@\" >> \"$here/calls.log\"\n\
             case \"$1\" in\n\
               repo)\n\
                 [ -f \"$here/sleep\" ] && sleep 2\n\
                 if [ -f \"$here/name-taken\" ]; then echo 'Name already exists on this account' >&2; exit 1; fi\n\
                 target=\"$3\"\n\
                 while [ $# -gt 0 ]; do [ \"$1\" = --source ] && src=\"$2\"; shift; done\n\
                 git -C \"$src\" remote add origin \"https://github.com/$target.git\"\n\
                 echo \"https://github.com/$target\"; exit 0 ;;\n\
             esac\n\
             exit 0\n",
        )
        .unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
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
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn wait_for_created(app: &mut App, rx: &mpsc::Receiver<AppEvent>) {
    match rx.recv_timeout(Duration::from_secs(20)) {
        Ok(event @ AppEvent::RemoteCreated(_)) => app.deliver_event(event),
        other => panic!("expected RemoteCreated, got {other:?}"),
    }
}

fn draft(target: &str) -> CreateDraft {
    CreateDraft::new(target.to_owned())
}

#[test]
fn a_creation_is_busy_then_reports_the_url_and_refreshes() {
    let project = Project::new("cr-ok");
    let mut app = project.app();
    let (tx, rx) = mpsc::channel();

    app.start_create_remote(draft("acme/tool"), tx);
    assert_eq!(app.remote_busy_label(), Some("Creating repository\u{2026}"));
    wait_for_created(&mut app, &rx);

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
    assert!(
        app.status_lines().iter().any(|l| l
            .to_string()
            .contains("Created https://github.com/acme/tool")),
        "{:?}",
        app.status_lines()
    );
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
    app.start_create_remote(draft("tool"), tx.clone());
    assert_eq!(app.remote_busy_label(), Some("Creating repository\u{2026}"));

    // A fetch, a push or a second creation while it runs is ignored.
    app.start_remote_op(RemoteOp::Fetch, None, tx.clone());
    app.start_create_remote(draft("other"), tx);
    assert_eq!(app.remote_busy_label(), Some("Creating repository\u{2026}"));
    assert!(rx.try_recv().is_err());

    // The one creation answers once, for the first target only.
    wait_for_created(&mut app, &rx);
    assert!(rx.try_recv().is_err(), "no second answer");
    let calls = project.calls();
    assert_eq!(calls.len(), 1, "{calls:?}");
    assert!(calls[0].starts_with("repo create tool "), "{}", calls[0]);
}
