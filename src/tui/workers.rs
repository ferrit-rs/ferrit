//! The background work in flight and the result of a refresh.

use crate::git;
use crate::git::diff::DiffOpts;
use crate::git::error::GitError;
use crate::git::identity::Authorship;
use crate::git::identity::Profile;
use crate::git::port::GitPort;
use crate::git::remote::RemoteOp;
use crate::git::remote::{self as remote_ops, RemoteRequest};
use crate::tui::App;
use crate::tui::components::diff::queries::DiffQueryState;
use crate::tui::components::files::tree::commit_drill_files;
use crate::tui::error::AppError;
use crate::tui::events::AppEvent;
use color_eyre::Result;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread::{self, JoinHandle};
use std::time::Instant;

/// One snapshot worker at a time. Bursty filesystem events collapse into one
/// follow-up snapshot instead of queuing stale concurrent reads.
#[derive(Default)]
pub(crate) struct RefreshQueryState {
    pub(crate) in_flight: bool,
    pub(crate) pending: bool,
}

#[derive(Default)]
pub(crate) struct ImageQueryState {
    pub(crate) path: Option<PathBuf>,
    pub(crate) generation: u64,
    pub(crate) in_flight: bool,
    pub(crate) pending: Option<(PathBuf, u64)>,
}

pub(crate) struct Workers {
    /// A handle onto `Events`' own channel, so a key can hand a background
    /// thread a way back onto it. `None` in `App::mock()` and right after
    /// `App::open`: only `run()` has an `Events` to ask for one. A
    /// `feed_key`-driven test with no `run()` leaves the remote keys inert
    /// unless it calls `start_remote_op` directly with its own channel.
    pub(crate) sender: Option<mpsc::Sender<AppEvent>>,
    pub(crate) refresh: RefreshQueryState,
    /// One selected-diff worker; only the latest requested key waits behind it.
    pub(crate) diff: DiffQueryState,
    pub(crate) image: ImageQueryState,
    /// `Some` while a background fetch/pull/push is running. The keys pressed
    /// again while `Some` are ignored outright, not queued, which sidesteps
    /// two git processes racing over the same `index.lock`.
    /// See `docs/PLAN_9_REMOTE.md`.
    pub(crate) remote_busy: Option<RemoteOp>,
    /// Start time for the inline branch-row spinner.
    pub(crate) remote_started: Option<Instant>,
    pub(crate) remote_cancel: Arc<AtomicBool>,
    pub(crate) remote_worker: Option<JoinHandle<()>>,
    /// A remote failure must outlive the snapshot completion that was requested
    /// immediately after the remote command.
    pub(crate) remote_refresh_error: Option<Arc<AppError>>,
    /// The snapshot error the last toast was about, so a repeat is not toasted again.
    pub(crate) refresh_failure: Option<String>,
}

impl Workers {
    pub(crate) fn new() -> Self {
        Self {
            sender: None,
            refresh: RefreshQueryState::default(),
            diff: DiffQueryState::default(),
            image: ImageQueryState::default(),
            remote_busy: None,
            remote_started: None,
            remote_cancel: Arc::new(AtomicBool::new(false)),
            remote_worker: None,
            remote_refresh_error: None,
            refresh_failure: None,
        }
    }
}

/// Which background worker a panic came from; its lowercase name is how the
/// message calls it.
#[derive(Debug, Clone, Copy, strum::Display)]
#[strum(serialize_all = "lowercase")]
pub(crate) enum WorkerKind {
    Refresh,
    Diff,
    Image,
    Remote,
    Stats,
}

#[derive(Debug, thiserror::Error)]
#[error("{worker} worker panicked: {detail}")]
pub struct WorkerError {
    pub(crate) worker: WorkerKind,
    pub(crate) detail: String,
}

/// Run worker logic behind a panic boundary so completion events can release
/// single-flight state even when a repository operation unexpectedly panics.
pub(crate) fn run_worker<T>(
    worker: WorkerKind,
    work: impl FnOnce() -> T,
) -> Result<T, WorkerError> {
    catch_unwind(AssertUnwindSafe(work)).map_err(|payload| {
        let detail = payload
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| payload.downcast_ref::<&'static str>().copied())
            .unwrap_or("non-string panic payload")
            .to_owned();
        WorkerError { worker, detail }
    })
}

impl Workers {
    /// Ask the network operation in flight to stop, and wait for it.
    pub(crate) fn stop_remote(&mut self) {
        if let Some(worker) = self.remote_worker.take() {
            self.remote_cancel.store(true, Ordering::Release);
            let _ = worker.join();
            self.remote_busy = None;
        }
    }
}

impl Workers {
    /// A network operation starts: note which, and when, and clear the cancel flag.
    pub(crate) fn begin_remote(&mut self, op: RemoteOp) {
        self.remote_busy = Some(op);
        self.remote_started = Some(Instant::now());
        self.remote_cancel.store(false, Ordering::Release);
    }

    /// The network operation is over: wait for its thread and clear the busy flag.
    pub(crate) fn end_remote(&mut self) {
        self.remote_busy = None;
        self.remote_started = None;
        if let Some(worker) = self.remote_worker.take() {
            let _ = worker.join();
        }
    }

    /// A short label for the Status pane while a fetch/pull/push is in
    /// flight, or `None`. Not a progress bar: ferrit has no way to know the
    /// percentages without parsing git's `--progress` stream.
    pub(crate) fn remote_busy_label(&self) -> Option<&'static str> {
        self.remote_busy.map(RemoteOp::busy_label)
    }

    /// LazyGit-style label, with a small animation, attached to the
    /// checked-out branch row while a network operation runs.
    pub(crate) fn remote_branch_status(&self) -> Option<String> {
        let label = self.remote_busy?.branch_label();
        let elapsed = self.remote_started?.elapsed().as_millis();
        let frame = [
            "\u{25cf}\u{2219}\u{2219}",
            "\u{2219}\u{25cf}\u{2219}",
            "\u{2219}\u{2219}\u{25cf}",
            "\u{2219}\u{25cf}\u{2219}",
        ]
        .get(usize::try_from(elapsed / 120).unwrap_or(usize::MAX) % 4)
        .copied()
        .unwrap_or("\u{25cf}\u{2219}\u{2219}");
        Some(format!("{label} {frame}"))
    }
}

/// Snapshot plus any active drill-down data loaded in the same worker.
/// A result whose error is shared between the Status line, the toast and the
/// refresh bookkeeping, none of which can own it alone.
pub(crate) type Shared<T> = Result<T, Arc<AppError>>;

#[doc(hidden)]
#[derive(Debug)]
pub struct RefreshCompletion {
    pub(crate) snapshot: Shared<git::Snapshot>,
    pub(crate) profile: Option<Profile>,
    pub(crate) branch_log: Option<(String, Shared<Vec<git::model::CommitEntry>>)>,
    pub(crate) commit_files: Option<(String, Shared<Vec<git::model::FileEntry>>)>,
}

impl RefreshCompletion {
    /// Read the snapshot, and the drilled branch log and commit files when the
    /// user is inside them, in one go (the worker's whole job).
    pub(crate) fn load(
        repo: &mut dyn GitPort,
        branch: Option<String>,
        commit: Option<String>,
        opts: DiffOpts,
    ) -> Self {
        let snapshot = repo.snapshot().map_err(|error| Arc::new(error.into()));
        let branch_log = branch.map(|name| {
            let result = repo
                .branch_log(&name)
                .map_err(|error| Arc::new(error.into()));
            (name, result)
        });
        let commit_files = commit.map(|hash| {
            let result = repo
                .commit_diff(&hash, opts)
                .map(|diff| commit_drill_files(&diff))
                .map_err(|error| Arc::new(error.into()));
            (hash, result)
        });
        Self {
            snapshot,
            profile: Some(Authorship::profile_of(repo)),
            branch_log,
            commit_files,
        }
    }

    /// The repository could not be opened: every part of the refresh fails
    /// with that error.
    pub(crate) fn failed(error: GitError, branch: Option<String>, commit: Option<String>) -> Self {
        let error = Arc::new(AppError::from(error));
        Self {
            snapshot: Err(Arc::clone(&error)),
            profile: None,
            branch_log: branch.map(|name| (name, Err(Arc::clone(&error)))),
            commit_files: commit.map(|hash| (hash, Err(error))),
        }
    }
}

impl App {
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
                remote_ops::run(repo.as_ref(), &request, &cancel)
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
            Err(error) => Err(self.create_remote.explain_push(error)),
            ok => {
                self.create_remote.pushing_after = false;
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
