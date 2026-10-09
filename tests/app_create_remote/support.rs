//! Hand-built data and render helpers shared by the dashboard screen tests.

use std::fmt::Write as _;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc;
use std::time::Duration;

use ferrit::app::App;
use ferrit::app::events::AppEvent;
use ferrit::app::state::create_remote_form::{CreateRemoteView, Field};
use ferrit::app::state::views::PopupView;
use ferrit::git::host::CreateDraft;
use ferrit::git::host::{GhProgram, Visibility};
use ratatui::crossterm::event::{KeyCode, KeyEvent};

/// A repository and a fake `gh` that logs its calls, adds `origin` like the
/// real one and answers with the web URL, or fails while `name-taken` exists.
pub(crate) struct Project {
    pub(crate) dir: PathBuf,
    pub(crate) work: PathBuf,
}

impl Project {
    pub(crate) fn new(tag: &str) -> Self {
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

    pub(crate) fn git(&self, args: &[&str]) -> String {
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

    pub(crate) fn app(&self) -> App {
        let mut app = App::open(&self.work).unwrap();
        app.set_gh_program(GhProgram::new(self.dir.join("gh")));
        // Never the user's own ~/.ssh/config: it would pick the host.
        app.set_ssh_config_path(self.dir.join("no-ssh-config"));
        app
    }

    pub(crate) fn calls(&self) -> Vec<String> {
        fs::read_to_string(self.dir.join("calls.log"))
            .unwrap_or_default()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    pub(crate) fn remotes(&self) -> String {
        self.git(&["remote"])
    }

    /// Where `origin` points, if it exists.
    pub(crate) fn remote_url(&self) -> Option<String> {
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
pub(crate) fn wait_for_created(app: &mut App, rx: &mpsc::Receiver<AppEvent>) {
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

pub(crate) fn draft(target: &str) -> CreateDraft {
    CreateDraft::new(target.to_owned())
}

impl Project {
    pub(crate) fn commit_count(&self) -> String {
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

pub(crate) fn bare_tree(bare: &std::path::Path, branch: &str) -> String {
    let out = Command::new("git")
        .arg("--git-dir")
        .arg(bare)
        .args(["ls-tree", "-r", "--name-only", branch])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

pub(crate) fn g(app: &mut App) {
    app.feed_key(KeyEvent::from(KeyCode::Char('G')));
}

pub(crate) fn press(app: &mut App, code: KeyCode) {
    app.feed_key(KeyEvent::from(code));
}

pub(crate) fn type_text(app: &mut App, text: &str) {
    for c in text.chars() {
        app.feed_key(KeyEvent::from(KeyCode::Char(c)));
    }
}

/// What the create popup shows, flattened to text for an assertion.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Shown {
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

pub(crate) fn shown(app: &App) -> Shown {
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

pub(crate) fn clear_name(app: &mut App) {
    for _ in 0..80 {
        press(app, KeyCode::Backspace);
    }
}

/// An app on a project with a ready `gh` and a way to receive events.
pub(crate) fn ready_app(project: &Project) -> (App, mpsc::Receiver<AppEvent>) {
    let mut app = project.app();
    let (tx, rx) = mpsc::channel();
    app.set_event_sender(tx);
    (app, rx)
}

/// Deliver the `gh` check's answer and open the form.
pub(crate) fn open_form(app: &mut App, rx: &mpsc::Receiver<AppEvent>) {
    app.open_create_remote();
    assert_eq!(shown(app), Shown::Checking);
    match rx.recv_timeout(Duration::from_secs(20)) {
        Ok(event @ AppEvent::GhChecked { .. }) => app.deliver_event(event),
        other => panic!("expected GhChecked, got {other:?}"),
    }
}

impl Project {
    pub(crate) fn commit(&self) {
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
    pub(crate) fn bare_origin(&self) -> PathBuf {
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

    pub(crate) fn branch(&self) -> String {
        self.git(&["rev-parse", "--abbrev-ref", "HEAD"])
    }
}

pub(crate) fn bare_refs(bare: &std::path::Path) -> String {
    let out = Command::new("git")
        .arg("--git-dir")
        .arg(bare)
        .args(["for-each-ref", "--format=%(refname:short)"])
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

/// Deliver events, refreshes included, until the push's `RemoteDone` arrives.
pub(crate) fn wait_for_push(app: &mut App, rx: &mpsc::Receiver<AppEvent>) {
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

pub(crate) fn status_text(app: &App) -> String {
    app.status_lines()
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

impl Project {
    /// An ssh config next to the project, with these `Host` aliases for GitHub.
    pub(crate) fn ssh_config(&self, aliases: &[&str]) -> PathBuf {
        let path = self.dir.join("ssh_config");
        let mut text = String::new();
        for alias in aliases {
            let _ = write!(text, "Host {alias}\n  HostName github.com\n  User git\n");
        }
        fs::write(&path, text).unwrap();
        path
    }
}

impl Project {
    /// A bare repository that `git@<host>:<path>.git` is rewritten to (git's own
    /// `insteadOf`), so a push to the alias URL lands there with no network.
    pub(crate) fn bare_origin_for_alias(&self, host: &str, path: &str) -> PathBuf {
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

pub(crate) const CREATE_ROW: &str = "Create a repository on GitHub  (g)";
