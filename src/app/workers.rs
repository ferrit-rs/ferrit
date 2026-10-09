//! The background work the app has in flight: the channel back to the event
//! loop, the single-flight state of the refresh, diff and image workers, and
//! the one network operation (fetch, pull, push, create) allowed at a time.
//! The code that starts each of them stays in its own module (`remote`,
//! `diff_query`, `image_query`, `App::request_refresh`); this is the state.

use crate::app::events::AppEvent;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::thread::JoinHandle;
use std::time::Instant;

use crate::app::error::AppError;
use crate::git::remote::RemoteOp;
use crate::interface::state::diff_query::DiffQueryState;

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
