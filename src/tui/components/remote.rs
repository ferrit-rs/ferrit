//! Fetch, pull, push in the background, and the credential prompt they may raise.

use crate::git::askpass;
use crate::git::remote::{self, PushPlan, RemoteOp, RemoteRequest};
use crate::tui::App;
use crate::tui::components::diff::CommitPopupView;
use crate::tui::components::popups::{ConfirmAction, ConfirmPrompt, Popup};
use crate::tui::error::AppError;
use crate::tui::events::AppEvent;
use crate::tui::widgets::text_input::{TextInput, TextInputMode};
use crate::tui::workers::{WorkerKind, run_worker};
use color_eyre::Result;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use std::sync::{Arc, mpsc};
use std::thread;

impl App {
    /// `f` / `p`: fetch / pull. Global, not Branches-only — unlike phase
    /// 8's branch actions, these act on the repo and its current branch,
    /// not a selected row. A no-op with no `event_sender` set
    /// (`App::mock()`, or a test driving `on_key` without `run()`).
    pub(crate) fn trigger_remote_op(&mut self, op: RemoteOp) {
        let Some(sender) = self.workers.sender.clone() else {
            return;
        };
        self.start_remote(RemoteRequest::new(op), sender);
    }

    /// `P`: push. Unlike `f`/`p`, push needs to know *before* running
    /// whether the current branch has an upstream at all (the Status header
    /// already has it, read for the ahead/behind count): `git::remote::plan_push`
    /// decides between a plain push, a lease-guarded one that asks first, one
    /// that sets the upstream, and a LazyGit-style editable `<remote> <branch>`
    /// prompt.
    pub(crate) fn push_current_branch(&mut self) {
        if self.modal.popup().is_some() {
            return;
        }
        let Some(repo) = &self.repo else { return };
        let remotes = if self.snapshot.header.upstream.is_some() {
            Vec::new()
        } else {
            match repo.remotes() {
                Ok(remotes) => remotes,
                Err(error) => {
                    self.report_error(error);
                    return;
                },
            }
        };
        let plan = remote::plan_push(&self.snapshot.header, &remotes, repo.push_default_current());
        match plan {
            PushPlan::Plain => self.trigger_remote_op(RemoteOp::Push),
            PushPlan::ConfirmForce(message) => self.modal.ask(ConfirmPrompt {
                message,
                action: ConfirmAction::ForcePush,
            }),
            PushPlan::SetCurrent => {
                if let Some(sender) = self.workers.sender.clone() {
                    self.start_remote(RemoteRequest::push_current(), sender);
                }
            },
            PushPlan::AskUpstream(text) => self
                .modal
                .open_popup(Popup::Upstream(TextInput::from_text(&text))),
        }
    }

    pub(crate) fn submit_upstream(&mut self, value: &str) {
        let Some((remote, branch)) = remote::parse_upstream(value) else {
            self.report_error(AppError::BadUpstream);
            return;
        };
        self.modal.close_popup();
        self.push_with_upstream(remote.to_owned(), branch.to_owned());
    }

    /// `git push -u <remote> <local>:<remote branch>`.
    pub(crate) fn push_with_upstream(&mut self, remote: String, branch: String) {
        if let Some(sender) = self.workers.sender.clone() {
            self.start_remote(RemoteRequest::push_to(remote, branch), sender);
        }
    }

    /// Spawn `request` on its own thread, `sender` its way back onto the same
    /// channel `Events::next()` reads (`docs/PLAN_9_REMOTE.md`: network calls
    /// are the first slow ones in `git::`, and running one on this thread
    /// would freeze the whole UI). One at a time: a second call while one is
    /// already running is ignored outright, not queued: two git processes
    /// racing over the same `index.lock` is a real failure mode. A no-op with
    /// no repo to reopen (`App::mock()`, a bare repo).
    ///
    /// Takes `sender` as a parameter rather than reading `self.workers.sender`
    /// directly so a test can call this with its own channel, no `run()`
    /// required.
    pub(crate) fn start_remote(&mut self, request: RemoteRequest, sender: mpsc::Sender<AppEvent>) {
        if self.workers.remote_busy.is_some() {
            return;
        }
        let Some(repo) = self.repo_handle() else {
            return;
        };
        let op = request.op;
        self.workers.begin_remote(op);
        self.status_note = None;
        let cancel = Arc::clone(&self.workers.remote_cancel);
        self.workers.remote_worker = Some(thread::spawn(move || {
            let message = run_worker(WorkerKind::Remote, || {
                remote::run(repo.as_ref(), &request, &cancel)
            })
            .map_err(AppError::from)
            .and_then(|result| result.map_err(AppError::from));
            let _ = sender.send(AppEvent::RemoteDone { op, message });
        }));
    }

    /// Integration-test seam: start `op` with `push_upstream` on a channel of
    /// the test's own.
    #[doc(hidden)]
    pub fn start_remote_op(
        &mut self,
        op: RemoteOp,
        push_upstream: Option<String>,
        sender: mpsc::Sender<AppEvent>,
    ) {
        let request = RemoteRequest {
            push_upstream,
            ..RemoteRequest::new(op)
        };
        self.start_remote(request, sender);
    }

    /// `AppEvent::RemoteDone` arrived: clear the busy flag, show a success
    /// line or the failure, then refresh: ahead/behind, branches, commits
    /// and files may all have moved. `pub`: `App::run`'s own match arm calls
    /// this, and so does a test that drove `start_remote_op` with its own
    /// channel.
    pub fn on_remote_done(&mut self, op: RemoteOp, message: Result<String, AppError>) {
        self.workers.end_remote();
        // Request refresh before setting the remote result. Async snapshot
        // completion preserves this operation's failure as the final Status
        // line; eventless callers refresh synchronously, then set it here.
        self.request_refresh();
        let message = match message {
            Err(error) => Err(self.explain_push_after_creation(error)),
            ok => {
                self.create_remote_push_done();
                ok
            },
        };
        match message {
            Ok(line) => {
                self.workers.remote_refresh_error = None;
                self.last_error = None;
                // git's own words after a push (`To github.com:…`, `branch 'main'
                // set up to track …`) are noise in the Status pane: the line
                // above already shows the branch in step, and the command log
                // has the command. A fetch or a pull keeps its line.
                self.status_note = (op != RemoteOp::Push).then_some(line);
            },
            Err(error) => {
                let error = Arc::new(error);
                self.workers.remote_refresh_error =
                    self.workers.sender.as_ref().map(|_| Arc::clone(&error));
                self.status_note = None;
                self.report_error(AppError::Background(error));
            },
        }
    }

    /// The Status pane's label while a fetch/pull/push is in flight.
    pub fn remote_busy_label(&self) -> Option<&'static str> {
        self.workers.remote_busy_label()
    }
}

impl App {
    /// A git child asked `prompt`. Open the popup, unless another popup is
    /// up: overwriting a half-typed commit message would lose it, so that
    /// question is cancelled and the operation fails instead.
    pub(crate) fn on_askpass(&mut self, prompt: String, reply: mpsc::Sender<Option<String>>) {
        if self.modal.popup().is_some() {
            let _ = reply.send(None);
            return;
        }
        self.modal.open_popup(Popup::Askpass(AskpassPrompt {
            secret: askpass::is_secret(&prompt),
            prompt,
            typed: TextInput::default(),
            shown: TextInput::default(),
            reply,
        }));
    }

    /// Enter answers, Esc cancels (git then reports the failed login);
    /// anything else edits the answer.
    pub(crate) fn askpass_key(&mut self, key: KeyEvent) {
        let Some(Popup::Askpass(ask)) = self.modal.popup_mut() else {
            return;
        };
        match key.code {
            KeyCode::Enter => ask.reply(Some(ask.typed.text())),
            KeyCode::Esc => ask.reply(None),
            _ => {
                ask.typed.handle_key_event(key, TextInputMode::SingleLine);
                let text = ask.typed.text();
                ask.shown = TextInput::from_text(&if ask.secret {
                    "\u{2022}".repeat(text.chars().count())
                } else {
                    text
                });
                return;
            },
        }
        self.modal.close_popup();
    }

    pub fn askpass_popup(&self) -> Option<CommitPopupView<'_>> {
        let Some(Popup::Askpass(ask)) = self.modal.popup() else {
            return None;
        };
        Some(CommitPopupView {
            title: ask.prompt.trim_end().trim_end_matches(':'),
            input: &ask.shown,
            description: None,
            summary_focused: false,
            overlay_state: None,
            lines: ask.shown.lines(),
            cursor: ask.shown.cursor(),
            toggles: None,
            author: None,
            hints: "Send: Enter | Cancel: Esc",
        })
    }
}

/// One pending question and where its answer goes. `typed` holds the real
/// text; `shown` is what the popup draws, dots when the answer is secret.
pub(crate) struct AskpassPrompt {
    pub(crate) prompt: String,
    pub(crate) typed: TextInput,
    pub(crate) shown: TextInput,
    pub(crate) secret: bool,
    pub(crate) reply: mpsc::Sender<Option<String>>,
}

impl AskpassPrompt {
    pub(crate) fn reply(&self, answer: Option<String>) {
        let _ = self.reply.send(answer);
    }
}
