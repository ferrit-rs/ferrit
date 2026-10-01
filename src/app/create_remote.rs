//! Creating the GitHub repository from ferrit (`docs/PLAN_15_CREATE_REMOTE.md`,
//! R2): the draft the user typed, the background `gh repo create`, and what
//! happens when it answers. The popups that fill the draft are R3.
//!
//! The creation takes the same slot as fetch, pull and push (`remote_busy`),
//! so one network operation runs at a time, the Status pane shows it, and quit
//! cancels it the way it cancels them.

use std::sync::atomic::Ordering;
use std::time::Instant;

use super::{App, AppError, AppEvent, Result, WorkerKind, events, mpsc, run_worker, thread};
use crate::domain::git::host::{CreateRequest, GhProgram, Visibility, parse_target};

/// What the form holds. Kept on the app while `gh` runs, so a refusal can
/// reopen the form with every field as typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreateDraft {
    /// `name` or `owner/name`.
    pub target: String,
    pub visibility: Visibility,
    pub description: String,
    /// Push the current branch once the repository exists.
    pub push_after: bool,
}

impl CreateDraft {
    /// The form's starting point: private, pushing, named after the folder.
    #[must_use]
    pub fn new(target: String) -> Self {
        Self {
            target,
            visibility: Visibility::Private,
            description: String::new(),
            push_after: true,
        }
    }
}

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
}

impl App {
    /// The creation's state, for the popups and for tests.
    pub fn create_remote(&self) -> &CreateRemote {
        &self.create_remote
    }

    /// Run this program for `gh` (a fake script in a test). Integration-test
    /// seam: the crate forbids `unsafe`, so the program cannot come from an
    /// environment variable set in-process.
    #[doc(hidden)]
    pub fn set_gh_program(&mut self, gh: GhProgram) {
        self.create_remote.gh = gh;
    }

    /// Start `gh repo create` for `draft` in the background. One network
    /// operation at a time: ignored while another runs. A target that fails
    /// validation is reported at once and nothing starts. Takes `sender` like
    /// `start_remote_op`, so a test can drive the answer itself.
    #[doc(hidden)]
    pub fn start_create_remote(&mut self, draft: CreateDraft, sender: mpsc::Sender<AppEvent>) {
        if self.remote_busy.is_some() {
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
        self.create_remote.draft = Some(draft);
        self.create_remote.error = None;
        self.remote_busy = Some(events::RemoteOp::Create);
        self.remote_busy_started = Some(Instant::now());
        self.status_note = None;
        self.remote_cancel.store(false, Ordering::Release);
        let cancel = std::sync::Arc::clone(&self.remote_cancel);
        self.remote_worker = Some(thread::spawn(move || {
            let result = run_worker(WorkerKind::RemoteOperation, || {
                repo.create_repo(&gh, &request, &cancel)
            })
            .map_err(|error| error.to_string())
            .and_then(|result| result.map_err(|error| error.to_string()))
            .map(|created| created.web_url);
            let _ = sender.send(AppEvent::RemoteCreated(result));
        }));
    }

    /// `AppEvent::RemoteCreated` arrived. The slot is freed and the panes
    /// refresh (`gh` added `origin`). A success keeps the web URL and drops the
    /// draft; a refusal keeps the draft and says why, nothing was configured.
    pub fn on_remote_created(&mut self, result: Result<String, String>) {
        self.remote_busy = None;
        self.remote_busy_started = None;
        if let Some(worker) = self.remote_worker.take() {
            let _ = worker.join();
        }
        self.request_refresh();
        match result {
            Ok(url) => {
                self.create_remote.draft = None;
                self.create_remote.error = None;
                self.last_error = None;
                self.status_note = Some(format!("Created {url}"));
                self.create_remote.web_url = Some(url);
            },
            Err(message) => {
                self.create_remote.error = Some(message.clone());
                self.status_note = None;
                self.report_error(AppError::Background(message));
            },
        }
    }
}
