//! Creating the GitHub repository from ferrit (`docs/PLAN_15_CREATE_REMOTE.md`,
//! R2, R3): the draft the user typed, the popups that fill it (the `gh`
//! check, the form, the last question), the background `gh repo create`, and
//! what happens when it answers.
//!
//! The creation takes the same slot as fetch, pull and push (`remote_busy`),
//! so one network operation runs at a time, the Status pane shows it, and quit
//! cancels it the way it cancels them.

use std::path::PathBuf;
use std::sync::Arc;

use crate::app::App;
use crate::app::error::AppError;
use crate::app::events::AppEvent;
use crate::app::workers::{WorkerKind, run_worker};
use crate::git::error::GitError;
use crate::git::host::{
    self, CreateDraft, CreateRequest, GhProgram, GhStatus, parse_target, sanitize_name,
    ssh_remote_url,
};
use crate::git::remote::RemoteOp;
use crate::git::ssh_config::read_github_aliases;
use crate::interface::state::create_remote_form::{
    Consequences, CreateRemoteView, Form, FormKey, Step,
};
use crate::interface::state::popup::Popup;
use color_eyre::Result;
use ratatui::crossterm::event::KeyEvent;
use std::sync::mpsc;
use std::thread;

/// The creation's own state on `App`.
#[derive(Debug, Default)]
pub struct CreateRemote {
    /// The last draft, until a creation succeeds.
    pub draft: Option<CreateDraft>,
    /// Why the last creation was refused, for the form to show.
    pub error: Option<String>,
    /// The web URL of the repository just created.
    pub web_url: Option<String>,
    gh: GhProgram,
    /// Bumped each time the check starts or is abandoned.
    generation: u64,
    /// The push in flight is the one that follows a creation.
    pushing_after: bool,
    /// The ssh config the host aliases are read from; `None` is
    /// `~/.ssh/config`. A test points it elsewhere.
    ssh_config: Option<PathBuf>,
    /// The SSH host chosen when the creation started (the user's own alias, or
    /// none), used once `gh` has made the repository.
    ssh_host: String,
}

impl CreateRemote {
    /// Keep the `gh` program a test or the replay injected when the app is
    /// rebuilt on a new repository (`App::attach_repository`).
    pub(crate) fn carry_program_from(&mut self, previous: &Self) {
        self.gh = previous.gh.clone();
        self.ssh_config.clone_from(&previous.ssh_config);
    }

    /// The GitHub aliases of the ssh config, in the order it lists them.
    fn ssh_aliases(&self) -> Vec<String> {
        let path = self.ssh_config.clone().or_else(|| {
            std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".ssh/config"))
        });
        path.map_or_else(Vec::new, |path| read_github_aliases(&path))
    }
}

impl App {
    /// The creation's state, for the popups and for tests.
    pub fn create_remote(&self) -> &CreateRemote {
        &self.create_remote
    }

    /// Read the ssh host aliases from this file instead of `~/.ssh/config`.
    /// Integration-test seam: a test must not depend on the user's own config.
    #[doc(hidden)]
    pub fn set_ssh_config_path(&mut self, path: PathBuf) {
        self.create_remote.ssh_config = Some(path);
    }

    /// Run this program for `gh` (a fake script in a test). Integration-test
    /// seam: the crate forbids `unsafe`, so the program cannot come from an
    /// environment variable set in-process.
    #[doc(hidden)]
    pub fn set_gh_program(&mut self, gh: GhProgram) {
        self.create_remote.gh = gh;
    }

    /// The `x` menu's "Create a repository on GitHub": ask `gh` whether it is
    /// ready (off the UI thread, it reaches the network), then show the form.
    /// Refused with a note when the repository already has a remote or another
    /// network operation runs.
    #[doc(hidden)]
    pub fn open_create_remote(&mut self) {
        if self.modal.is_some() || self.repo.is_none() {
            return;
        }
        if !self.snapshot.remotes.is_empty() {
            self.modal.open_popup(Popup::Note(
                "this repository already has a remote".to_owned(),
            ));
            return;
        }
        if self.workers.remote_busy.is_some() {
            self.modal.open_popup(Popup::Note(
                "another network operation is running".to_owned(),
            ));
            return;
        }
        self.create_remote.generation += 1;
        let generation = self.create_remote.generation;
        self.modal
            .open_popup(Popup::CreateRemote(Step::Checking { generation }));
        let gh = self.create_remote.gh.clone();
        let Some(sender) = self.workers.sender.clone() else {
            // No event loop (`App::mock`, a test without `run()`): ask now.
            let status = host::gh_status(&gh);
            self.on_gh_checked(generation, status);
            return;
        };
        thread::spawn(move || {
            let status = run_worker(WorkerKind::Remote, || host::gh_status(&gh))
                .unwrap_or(GhStatus::Missing);
            let _ = sender.send(AppEvent::GhChecked { generation, status });
        });
    }

    /// `AppEvent::GhChecked` arrived. Only the check the popup is waiting for
    /// counts; `gh` ready opens the form, anything else says what to do.
    pub(crate) fn on_gh_checked(&mut self, generation: u64, status: GhStatus) {
        let waiting = matches!(
            self.modal.popup(),
            Some(Popup::CreateRemote(Step::Checking { generation: g })) if *g == generation
        );
        if !waiting {
            return;
        }
        self.modal.open_popup(match status {
            GhStatus::Ready => {
                let draft = self
                    .create_remote
                    .draft
                    .clone()
                    .unwrap_or_else(|| CreateDraft::new(sanitize_name(&self.repo_name)));
                Popup::CreateRemote(Step::Form(Form::from_draft(&draft, None)))
            },
            GhStatus::Missing => Popup::Note(
                "gh is required: https://cli.github.com. Install it, then try again.".to_owned(),
            ),
            GhStatus::SignedOut => Popup::Note(
                "gh is not signed in: run `gh auth login` in a shell, then try again.".to_owned(),
            ),
        });
    }

    /// The form reopened on the name after a refusal, every field as typed.
    fn reopen_form(&mut self, error: String) {
        let Some(draft) = &self.create_remote.draft else {
            return;
        };
        if self.modal.popup().is_none() {
            self.modal
                .open_popup(Popup::CreateRemote(Step::Form(Form::from_draft(
                    draft,
                    Some(error),
                ))));
        }
    }

    /// The repository has no commit yet, asked of git itself: the app's own list
    /// of commits can be a refresh behind, right after a first commit.
    fn repo_has_no_commit(&self) -> bool {
        self.repo.as_ref().is_some_and(|repo| !repo.has_commits())
    }

    /// What the renderer draws, or `None` when this popup is not up.
    pub fn create_remote_view(&self) -> Option<CreateRemoteView<'_>> {
        let Some(Popup::CreateRemote(step)) = self.modal.popup() else {
            return None;
        };
        let aliases = self.create_remote.ssh_aliases();
        Some(step.view(&Consequences {
            first_commit: self.repo_has_no_commit(),
            ssh_host: aliases.first().map(String::as_str),
            branch: &self.snapshot.header.branch,
        }))
    }

    /// Every key while the popup is up.
    pub(crate) fn create_remote_key(&mut self, key: KeyEvent) {
        let Some(Popup::CreateRemote(step)) = self.modal.popup_mut() else {
            return;
        };
        match step.key(key) {
            FormKey::None => {},
            FormKey::Close => self.close_create_remote(),
            FormKey::Continue => {
                let Some(Popup::CreateRemote(Step::Form(form))) = self.modal.take_popup() else {
                    return;
                };
                match form.validate() {
                    Ok(_) => self
                        .modal
                        .open_popup(Popup::CreateRemote(Step::Confirm(form))),
                    Err(reason) => {
                        self.modal.open_popup(Popup::CreateRemote(Step::Form(
                            form.with_error(reason.to_string()),
                        )));
                    },
                }
            },
            FormKey::Back => {
                if let Some(Popup::CreateRemote(Step::Confirm(form))) = self.modal.take_popup() {
                    self.modal.open_popup(Popup::CreateRemote(Step::Form(form)));
                }
            },
            FormKey::Create => {
                let Some(Popup::CreateRemote(Step::Confirm(form))) = self.modal.take_popup() else {
                    return;
                };
                let Some(sender) = self.workers.sender.clone() else {
                    return;
                };
                self.start_create_remote(form.draft(), sender);
            },
        }
    }

    /// `Esc`: nothing was created. The typed fields are kept for next time; a
    /// check still running is abandoned.
    fn close_create_remote(&mut self) {
        match self.modal.take_popup() {
            Some(Popup::CreateRemote(Step::Form(form) | Step::Confirm(form))) => {
                self.create_remote.draft = Some(form.draft());
            },
            Some(Popup::CreateRemote(Step::Checking { .. })) => self.create_remote.generation += 1,
            Some(other) => self.modal.open_popup(other),
            None => {},
        }
    }

    /// Start `gh repo create` for `draft` in the background. One network
    /// operation at a time: ignored while another runs. A target that fails
    /// validation is reported at once and nothing starts. Takes `sender` like
    /// `start_remote_op`, so a test can drive the answer itself.
    #[doc(hidden)]
    pub fn start_create_remote(&mut self, draft: CreateDraft, sender: mpsc::Sender<AppEvent>) {
        if self.workers.remote_busy.is_some() {
            return;
        }
        let (owner, name) = match parse_target(&draft.target) {
            Ok(target) => target,
            Err(error) => {
                self.create_remote.error = Some(error.to_string());
                self.create_remote.draft = Some(draft);
                return;
            },
        };
        let Some(repo) = self.repo_handle() else {
            return;
        };
        let request = CreateRequest {
            owner,
            name,
            visibility: draft.visibility,
            description: draft.description.clone(),
        };
        let gh = self.create_remote.gh.clone();
        // The user's own alias, when they have one: the key it names is the one
        // that unlocks the push. None keeps the URL `gh` writes.
        self.create_remote.ssh_host = self
            .create_remote
            .ssh_aliases()
            .into_iter()
            .next()
            .unwrap_or_default();
        let author = self.authorship.author_arg();
        self.create_remote.draft = Some(draft);
        self.create_remote.error = None;
        self.workers.begin_remote(RemoteOp::Create);
        self.status_note = None;
        let cancel = Arc::clone(&self.workers.remote_cancel);
        self.workers.remote_worker = Some(thread::spawn(move || {
            let result = run_worker(WorkerKind::Remote, || {
                // The first commit comes first, and is local: if it fails
                // nothing has been created anywhere. A repository that already
                // has a commit is left as it is.
                repo.initial_commit(author).map_err(|error| {
                    GitError::HostFailed(format!(
                        "the first commit failed, nothing was created: {error}"
                    ))
                })?;
                repo.create_repo(&gh, &request, &cancel)
            })
            .map_err(AppError::from)
            .and_then(|result| result.map_err(AppError::from))
            .map(|created| created.web_url);
            let _ = sender.send(AppEvent::RemoteCreated(result));
        }));
    }

    /// `AppEvent::RemoteCreated` arrived. The slot is freed and the panes
    /// refresh (`gh` added `origin`). A success keeps the web URL and drops the
    /// draft, then pushes; a refusal keeps the draft and says
    /// why, nothing was configured.
    pub fn on_remote_created(&mut self, result: Result<String, AppError>) {
        self.workers.end_remote();
        self.request_refresh();
        match result {
            Ok(url) => {
                self.create_remote.draft = None;
                let ssh_host = std::mem::take(&mut self.create_remote.ssh_host);
                self.create_remote.error = None;
                self.last_error = None;
                self.status_note = Some(format!("Created {url}"));
                self.create_remote.web_url = Some(url.clone());
                self.after_creation(&url, &ssh_host);
            },
            Err(error) => {
                let message = error.to_string();
                self.create_remote.error = Some(message.clone());
                self.status_note = None;
                self.reopen_form(message);
                self.report_error(AppError::Background(Arc::new(error)));
            },
        }
    }

    /// What follows a creation: `origin` rewritten over the user's SSH alias (so
    /// the push reaches the key that alias names, and ferrit's passphrase popup
    /// answers for it, as for `P`), then the push of the current branch through
    /// the same path as `P`, or a note saying why not (a detached `HEAD`). The repository and
    /// `origin` stay whatever happens.
    fn after_creation(&mut self, url: &str, ssh_host: &str) {
        let mut created = format!("Created {url}.");
        if !ssh_host.is_empty() {
            let Some(remote) = ssh_remote_url(ssh_host, url) else {
                self.status_note = Some(format!(
                    "{created} Its owner and name could not be read from that URL: origin keeps \
                     the URL gh wrote."
                ));
                return;
            };
            let set = self
                .repo
                .as_ref()
                .map(|repo| repo.set_remote_url("origin", &remote));
            if let Some(Err(error)) = set {
                self.status_note = Some(format!(
                    "{created} {error}. origin keeps the URL gh wrote; fix it with git remote \
                     set-url, then P pushes."
                ));
                return;
            }
            created = format!("{created} origin is {remote}.");
        }
        if self.snapshot.header.detached {
            self.status_note = Some(format!("{created} HEAD is detached: nothing to push."));
        } else {
            // The push's own progress takes the status line from here.
            let branch = self.snapshot.header.branch.clone();
            self.push_with_upstream("origin".to_owned(), branch);
            // Only a push that really started is the one to explain if it fails.
            self.create_remote.pushing_after = self.workers.remote_busy.is_some();
        }
    }

    /// A push finished well: it was no longer "the one after a creation".
    pub(crate) fn create_remote_push_done(&mut self) {
        self.create_remote.pushing_after = false;
    }

    /// The failure of the push that followed a creation says the repository
    /// exists and how to retry; any other failure is left as it is.
    pub(crate) fn explain_push_after_creation(&mut self, failure: AppError) -> AppError {
        if !std::mem::take(&mut self.create_remote.pushing_after) {
            return failure;
        }
        match &self.create_remote.web_url {
            Some(url) => AppError::PushAfterCreation {
                source: Arc::new(failure),
                url: url.clone(),
            },
            None => failure,
        }
    }
}
