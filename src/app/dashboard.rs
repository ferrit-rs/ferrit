//! The dashboard's state, its background worker and its keys
//! (`docs/PLAN_13_DASHBOARD.md`, D2). Drawing is `screens/dashboard.rs`.
//!
//! The statistics are computed off the UI thread in two steps: a quick one
//! (walk and aggregate, no `git log --numstat`) that fills the screen, then the
//! full one with the lines added and removed and the hot files. A result carries
//! the generation of the request it answers, so a window changed or a screen
//! closed meanwhile drops it, as `DiffDone` does.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};

use super::{App, AppEvent, FullScreen, KeyCode, KeyEvent, WorkerKind, git, run_worker};
use crate::domain::git::stats::{RepoStats, StatsOptions, Window};

/// The windows `t` cycles through, shortest first.
const WINDOWS: [Window; 5] = [
    Window::Days7,
    Window::Days30,
    Window::Days90,
    Window::Year,
    Window::All,
];
/// `WINDOWS[2]`, 90 days: what the dashboard opens on.
const DEFAULT_WINDOW: usize = 2;
/// Rows `PageUp` / `PageDown` scroll.
const PAGE: usize = 10;

/// One answer of the worker.
#[derive(Debug)]
pub struct StatsCompletion {
    generation: u64,
    window: Window,
    /// The full pass (with the numstat), not the quick one.
    full: bool,
    result: Result<Box<RepoStats>, String>,
}

#[derive(Debug)]
struct Cached {
    stats: RepoStats,
    /// Only the quick pass is in: the lines and hot files are still coming.
    churn_pending: bool,
}

/// Everything the dashboard keeps between two frames.
#[derive(Debug)]
pub struct Dashboard {
    window: usize,
    cache: [Option<Cached>; WINDOWS.len()],
    /// Incremented by every request; a result of another generation is stale.
    generation: u64,
    cancel: Arc<AtomicBool>,
    worker: Option<JoinHandle<()>>,
    /// Which window the running request is for.
    in_flight: Option<usize>,
    /// What the cache was computed for: `refs_fingerprint` at request time.
    fingerprint: u64,
    scroll: usize,
    show_counts: bool,
    error: Option<String>,
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

    fn cached(&self) -> Option<&Cached> {
        self.cache.get(self.window).and_then(Option::as_ref)
    }

    /// Store or drop the statistics of one window.
    fn store(&mut self, slot: usize, value: Option<Cached>) {
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

    pub(super) fn is_busy(&self) -> bool {
        self.in_flight.is_some()
    }

    /// Stop reading: set the flag the walk polls and let the thread finish on
    /// its own, nothing waits for it.
    fn cancel_running(&mut self) {
        self.cancel.store(true, Ordering::Release);
        self.worker = None;
        self.in_flight = None;
    }

    /// Quit: same, but wait, so the process does not exit under the thread.
    pub(super) fn stop_and_join(&mut self) {
        self.cancel.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        self.in_flight = None;
    }
}

impl App {
    /// A fingerprint of what the statistics depend on that the snapshot already
    /// tells: the checked-out branch, the newest commit, every branch's tip time
    /// and counts, the stash count. When it moves, the cache is dropped.
    fn refs_fingerprint(&self) -> u64 {
        let mut hasher = DefaultHasher::new();
        self.header.branch.hash(&mut hasher);
        self.commits.first().map(|c| &c.full_hash).hash(&mut hasher);
        for branch in &self.branches {
            (&branch.name, branch.tip_time, branch.ahead, branch.behind).hash(&mut hasher);
        }
        self.stashes.len().hash(&mut hasher);
        hasher.finish()
    }

    /// `D`: show the dashboard over the panes, computing what is not cached.
    pub fn open_dashboard(&mut self) {
        self.dashboard.error = None;
        self.dashboard.scroll = 0;
        self.full_screen = FullScreen::Dashboard;
        self.ensure_stats(false);
    }

    /// The renderer's word on how far the page scrolls: keep the offset in it.
    pub(crate) fn clamp_dashboard_scroll(&mut self, max: usize) {
        self.dashboard.scroll = self.dashboard.scroll.min(max);
    }

    /// Back to the panes. A running computation is told to stop; its result, if
    /// it still arrives, is kept only when it is good.
    pub fn close_dashboard(&mut self) {
        self.full_screen = FullScreen::None;
        self.dashboard.cancel_running();
    }

    /// Make sure the current window has statistics, or is on its way to have
    /// them. `force` recomputes even when the cache is fresh (`r`).
    fn ensure_stats(&mut self, force: bool) {
        let fingerprint = self.refs_fingerprint();
        if fingerprint != self.dashboard.fingerprint {
            self.dashboard.cache = std::array::from_fn(|_| None);
            self.dashboard.fingerprint = fingerprint;
        }
        let slot = self.dashboard.window;
        let cached = self.dashboard.cached();
        if !force && cached.is_some_and(|c| !c.churn_pending) {
            return;
        }
        if !force && self.dashboard.in_flight == Some(slot) {
            return;
        }
        if force {
            self.dashboard.store(slot, None);
        }
        self.dashboard.cancel_running();
        self.dashboard.generation += 1;
        self.dashboard.cancel = Arc::new(AtomicBool::new(false));
        self.dashboard.in_flight = Some(slot);
        self.dashboard.error = None;

        let Some(repo) = self.repo_handle() else {
            self.dashboard.in_flight = None;
            return;
        };
        let generation = self.dashboard.generation;
        let window = self.dashboard.window();
        let cancel = Arc::clone(&self.dashboard.cancel);
        let Some(sender) = self.event_sender.clone() else {
            // No event loop (`App::mock`, a test without `run()`): do it now.
            for full in [false, true] {
                let result = read_stats(&repo, window, full, &cancel);
                self.on_stats_done(StatsCompletion {
                    generation,
                    window,
                    full,
                    result,
                });
            }
            return;
        };
        self.dashboard.worker = Some(thread::spawn(move || {
            for full in [false, true] {
                if cancel.load(Ordering::Acquire) {
                    break;
                }
                let result = read_stats(&repo, window, full, &cancel);
                let failed = result.is_err();
                let _ = sender.send(AppEvent::StatsDone(StatsCompletion {
                    generation,
                    window,
                    full,
                    result,
                }));
                if failed {
                    break;
                }
            }
        }));
    }

    /// `AppEvent::StatsDone` arrived. A stale generation is dropped; a good
    /// result is cached whether or not the screen is still up, an error only
    /// shows when it is.
    pub(super) fn on_stats_done(&mut self, completion: StatsCompletion) {
        if completion.generation != self.dashboard.generation {
            return;
        }
        let Some(slot) = WINDOWS.iter().position(|w| *w == completion.window) else {
            return;
        };
        match completion.result {
            Ok(stats) => {
                self.dashboard.store(
                    slot,
                    Some(Cached {
                        stats: *stats,
                        churn_pending: !completion.full,
                    }),
                );
                self.dashboard.error = None;
            },
            Err(message) => {
                if self.full_screen == FullScreen::Dashboard {
                    self.dashboard.error = Some(message);
                }
            },
        }
        if completion.full || self.dashboard.error.is_some() {
            if let Some(worker) = self.dashboard.worker.take() {
                let _ = worker.join();
            }
            self.dashboard.in_flight = None;
        }
    }

    /// Every key while the dashboard is up (after the popups, a pending
    /// confirmation and the help overlay, which own input before it).
    pub(super) fn dashboard_key(&mut self, key: KeyEvent) {
        match key.code {
            KeyCode::Esc | KeyCode::Char('q' | 'D') => self.close_dashboard(),
            KeyCode::Char('?') => self.show_help = true,
            KeyCode::Char('t') => self.cycle_window(true),
            KeyCode::Char('T') => self.cycle_window(false),
            KeyCode::Char('r') => self.ensure_stats(true),
            KeyCode::Char('n') => self.dashboard.show_counts = !self.dashboard.show_counts,
            KeyCode::Char('k') | KeyCode::Up => {
                self.dashboard.scroll = self.dashboard.scroll.saturating_sub(1);
            },
            KeyCode::Char('j') | KeyCode::Down => {
                self.dashboard.scroll = self.dashboard.scroll.saturating_add(1);
            },
            KeyCode::PageUp => {
                self.dashboard.scroll = self.dashboard.scroll.saturating_sub(PAGE);
            },
            KeyCode::PageDown => {
                self.dashboard.scroll = self.dashboard.scroll.saturating_add(PAGE);
            },
            KeyCode::Home => self.dashboard.scroll = 0,
            KeyCode::End => self.dashboard.scroll = usize::MAX,
            _ => {},
        }
    }

    fn cycle_window(&mut self, forward: bool) {
        let count = WINDOWS.len();
        self.dashboard.window = if forward {
            (self.dashboard.window + 1) % count
        } else {
            (self.dashboard.window + count - 1) % count
        };
        self.dashboard.scroll = 0;
        self.ensure_stats(false);
    }
}

/// One pass of the statistics behind the panic boundary, its errors flattened
/// into text so they cross the channel.
fn read_stats(
    repo: &git::Repo,
    window: Window,
    full: bool,
    cancel: &AtomicBool,
) -> Result<Box<RepoStats>, String> {
    let options = StatsOptions {
        churn: full,
        ..StatsOptions::default()
    };
    run_worker(WorkerKind::Stats, || {
        repo.stats_with(window, &options, cancel)
    })
    .map_err(|error| error.to_string())
    .and_then(|result| result.map_err(|error| error.to_string()))
    .map(Box::new)
}
