//! The dashboard's state, its background worker and its keys
//! (`docs/PLAN_13_DASHBOARD.md`, D2). Drawing is `screens/dashboard.rs`.
//!
//! The statistics are computed off the UI thread in two steps: a quick one
//! (walk and aggregate, no `git log --numstat`) that fills the screen, then the
//! full one with the lines added and removed and the hot files. A result carries
//! the generation of the request it answers, so a window changed or a screen
//! closed meanwhile drops it, as `DiffDone` does.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::JoinHandle;

use crate::git::port::GitPort;
use crate::git::stats::{RepoStats, StatsOptions, Window};
use crate::tui::error::AppError;
use crate::tui::workers::{WorkerKind, run_worker};

/// The windows `t` cycles through, shortest first.
pub(crate) const WINDOWS: [Window; 5] = [
    Window::Days7,
    Window::Days30,
    Window::Days90,
    Window::Year,
    Window::All,
];
/// `WINDOWS[2]`, 90 days: what the dashboard opens on.
pub(crate) const DEFAULT_WINDOW: usize = 2;
/// Rows `PageUp` / `PageDown` scroll.
pub(crate) const PAGE: usize = 10;
/// Rows one wheel notch scrolls.
pub(crate) const WHEEL_ROWS: usize = 3;

/// One answer of the worker.
#[derive(Debug)]
pub struct StatsCompletion {
    pub(crate) generation: u64,
    pub(crate) window: Window,
    /// The full pass (with the numstat), not the quick one.
    pub(crate) full: bool,
    pub(crate) result: Result<Box<RepoStats>, AppError>,
}

#[derive(Debug)]
pub(crate) struct Cached {
    pub(crate) stats: RepoStats,
    /// Only the quick pass is in: the lines and hot files are still coming.
    pub(crate) churn_pending: bool,
}

/// Everything the dashboard keeps between two frames.
#[derive(Debug)]
pub struct Dashboard {
    pub(crate) window: usize,
    pub(crate) cache: [Option<Cached>; WINDOWS.len()],
    /// Incremented by every request; a result of another generation is stale.
    pub(crate) generation: u64,
    pub(crate) cancel: Arc<AtomicBool>,
    pub(crate) worker: Option<JoinHandle<()>>,
    /// Which window the running request is for.
    pub(crate) in_flight: Option<usize>,
    /// What the cache was computed for: `refs_fingerprint` at request time.
    pub(crate) fingerprint: u64,
    pub(crate) scroll: usize,
    pub(crate) show_counts: bool,
    pub(crate) error: Option<String>,
}

impl Default for Dashboard {
    fn default() -> Self {
        Self {
            window: DEFAULT_WINDOW,
            cache: std::array::from_fn(|_| None),
            generation: 0,
            cancel: Arc::new(AtomicBool::new(false)),
            worker: None,
            in_flight: None,
            fingerprint: 0,
            scroll: 0,
            show_counts: false,
            error: None,
        }
    }
}

impl Dashboard {
    pub fn window(&self) -> Window {
        WINDOWS.get(self.window).copied().unwrap_or(Window::Days90)
    }

    pub(crate) fn cached(&self) -> Option<&Cached> {
        self.cache.get(self.window).and_then(Option::as_ref)
    }

    /// Store or drop the statistics of one window.
    pub(crate) fn store(&mut self, slot: usize, value: Option<Cached>) {
        if let Some(entry) = self.cache.get_mut(slot) {
            *entry = value;
        }
    }

    /// The statistics for the current window, if any have arrived.
    pub fn stats(&self) -> Option<&RepoStats> {
        self.cached().map(|c| &c.stats)
    }

    /// The quick pass is in and the numstat is still being read.
    pub fn churn_pending(&self) -> bool {
        self.cached().is_some_and(|c| c.churn_pending)
    }

    /// Nothing at all has arrived for the current window yet.
    pub fn computing(&self) -> bool {
        self.cached().is_none() && self.in_flight.is_some()
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn scroll(&self) -> usize {
        self.scroll
    }

    /// Counts instead of percentages as the primary figure (`n`).
    pub fn show_counts(&self) -> bool {
        self.show_counts
    }

    pub(crate) fn is_busy(&self) -> bool {
        self.in_flight.is_some()
    }

    /// Stop reading: set the flag the walk polls and let the thread finish on
    /// its own, nothing waits for it.
    pub(crate) fn cancel_running(&mut self) {
        self.cancel.store(true, Ordering::Release);
        self.worker = None;
        self.in_flight = None;
    }

    /// Quit: same, but wait, so the process does not exit under the thread.
    pub(crate) fn stop_and_join(&mut self) {
        self.cancel.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        self.in_flight = None;
    }
}

/// One pass of the statistics behind the panic boundary.
pub(crate) fn read_stats(
    repo: &dyn GitPort,
    window: Window,
    full: bool,
    cancel: &AtomicBool,
) -> Result<Box<RepoStats>, AppError> {
    let options = StatsOptions {
        churn: full,
        ..StatsOptions::default()
    };
    run_worker(WorkerKind::Stats, || {
        repo.stats_with(window, &options, cancel)
    })
    .map_err(AppError::from)
    .and_then(|result| result.map_err(AppError::from))
    .map(Box::new)
}
