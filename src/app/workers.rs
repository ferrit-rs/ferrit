//! The background work the app has in flight: the channel back to the event
//! loop, the single-flight state of the refresh, diff and image workers, and
//! the one network operation (fetch, pull, push, create) allowed at a time.
//! The code that starts each of them stays in its own module (`remote`,
//! `diff_query`, `image_query`, `App::request_refresh`); this is the state.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Instant;

use super::AppError;
use super::diff_query::DiffQueryState;
use super::events::{AppEvent, RemoteOp};

/// One snapshot worker at a time. Bursty filesystem events collapse into one
/// follow-up snapshot instead of queuing stale concurrent reads.
#[derive(Default)]
pub(super) struct RefreshQueryState {
    pub(super) in_flight: bool,
    pub(super) pending: bool,
}

#[derive(Default)]
pub(super) struct ImageQueryState {
    pub(super) path: Option<PathBuf>,
    pub(super) generation: u64,
    pub(super) in_flight: bool,
    pub(super) pending: Option<(PathBuf, u64)>,
}

pub(super) struct Workers {
    /// A handle onto `Events`' own channel, so a key can hand a background
    /// thread a way back onto it. `None` in `App::mock()` and right after
    /// `App::open`: only `run()` has an `Events` to ask for one. A
    /// `feed_key`-driven test with no `run()` leaves the remote keys inert
    /// unless it calls `start_remote_op` directly with its own channel.
    pub(super) sender: Option<mpsc::Sender<AppEvent>>,
    pub(super) refresh: RefreshQueryState,
    /// One selected-diff worker; only the latest requested key waits behind it.
    pub(super) diff: DiffQueryState,
    pub(super) image: ImageQueryState,
    /// `Some` while a background fetch/pull/push is running. The keys pressed
    /// again while `Some` are ignored outright, not queued, which sidesteps
    /// two git processes racing over the same `index.lock`.
    /// See `docs/PLAN_9_REMOTE.md`.
    pub(super) remote_busy: Option<RemoteOp>,
    /// Start time for the inline branch-row spinner.
    pub(super) remote_started: Option<Instant>,
    pub(super) remote_cancel: Arc<AtomicBool>,
    pub(super) remote_worker: Option<JoinHandle<()>>,
    /// A remote failure must outlive the snapshot completion that was requested
    /// immediately after the remote command.
    pub(super) remote_refresh_error: Option<Arc<AppError>>,
    /// The snapshot error the last toast was about, so a repeat is not toasted again.
    pub(super) refresh_failure: Option<String>,
}

impl Workers {
    pub(super) fn new() -> Self {
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
