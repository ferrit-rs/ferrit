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

use super::keymap::{Action, Context, KeyBinding};
use super::sheet::Sheet;
use super::{
    App, AppError, AppEvent, KeyCode, KeyEvent, MouseEvent, MouseEventKind, WorkerKind, run_worker,
};
use crate::domain::git::port::GitPort;
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
/// Rows one wheel notch scrolls.
const WHEEL_ROWS: usize = 3;

/// One answer of the worker.
#[derive(Debug)]
pub struct StatsCompletion {
    generation: u64,
    window: Window,
    /// The full pass (with the numstat), not the quick one.
    full: bool,
    result: Result<Box<RepoStats>, AppError>,
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
        self.snapshot.header.branch.hash(&mut hasher);
        self.snapshot
            .commits
            .first()
            .map(|c| &c.full_hash)
            .hash(&mut hasher);
        for branch in &self.snapshot.branches {
            (&branch.name, branch.tip_time, branch.ahead, branch.behind).hash(&mut hasher);
        }
        self.snapshot.stashes.len().hash(&mut hasher);
        hasher.finish()
    }

    /// `D`: show the dashboard over the panes, computing what is not cached.
    pub fn open_dashboard(&mut self) {
        self.open_sheet(Sheet::Dashboard);
    }

    /// The sheet is about to open: at the top, no old error, statistics asked for.
    pub(super) fn prepare_dashboard_sheet(&mut self) {
        self.sheets.dashboard.error = None;
        self.sheets.dashboard.scroll = 0;
        self.ensure_stats(false);
    }

    /// The renderer's word on how far the page scrolls: keep the offset in it.
    pub(crate) fn clamp_dashboard_scroll(&mut self, max: usize) {
        self.sheets.dashboard.scroll = self.sheets.dashboard.scroll.min(max);
    }

    /// Back to the panes. A running computation is told to stop; its result, if
    /// it still arrives, is kept only when it is good.
    pub fn close_dashboard(&mut self) {
        self.close_sheet();
        self.sheets.dashboard.cancel_running();
    }

    /// Make sure the current window has statistics, or is on its way to have
    /// them. `force` recomputes even when the cache is fresh (`r`).
    fn ensure_stats(&mut self, force: bool) {
        let fingerprint = self.refs_fingerprint();
        if fingerprint != self.sheets.dashboard.fingerprint {
            self.sheets.dashboard.cache = std::array::from_fn(|_| None);
            self.sheets.dashboard.fingerprint = fingerprint;
        }
        let slot = self.sheets.dashboard.window;
        let cached = self.sheets.dashboard.cached();
        if !force && cached.is_some_and(|c| !c.churn_pending) {
            return;
        }
        if !force && self.sheets.dashboard.in_flight == Some(slot) {
            return;
        }
        if force {
            self.sheets.dashboard.store(slot, None);
        }
        self.sheets.dashboard.cancel_running();
        self.sheets.dashboard.generation += 1;
        self.sheets.dashboard.cancel = Arc::new(AtomicBool::new(false));
        self.sheets.dashboard.in_flight = Some(slot);
        self.sheets.dashboard.error = None;

        let Some(repo) = self.repo_handle() else {
            self.sheets.dashboard.in_flight = None;
            return;
        };
        let generation = self.sheets.dashboard.generation;
        let window = self.sheets.dashboard.window();
        let cancel = Arc::clone(&self.sheets.dashboard.cancel);
        let Some(sender) = self.workers.sender.clone() else {
            // No event loop (`App::mock`, a test without `run()`): do it now.
            for full in [false, true] {
                let result = read_stats(repo.as_ref(), window, full, &cancel);
                self.on_stats_done(StatsCompletion {
                    generation,
                    window,
                    full,
                    result,
                });
            }
            return;
        };
        self.sheets.dashboard.worker = Some(thread::spawn(move || {
            for full in [false, true] {
                if cancel.load(Ordering::Acquire) {
                    break;
                }
                let result = read_stats(repo.as_ref(), window, full, &cancel);
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
        if completion.generation != self.sheets.dashboard.generation {
            return;
        }
        let Some(slot) = WINDOWS.iter().position(|w| *w == completion.window) else {
            return;
        };
        match completion.result {
            Ok(stats) => {
                self.sheets.dashboard.store(
                    slot,
                    Some(Cached {
                        stats: *stats,
                        churn_pending: !completion.full,
                    }),
                );
                self.sheets.dashboard.error = None;
            },
            Err(message) => {
                if self.dashboard_is_open() {
                    self.sheets.dashboard.error = Some(message.to_string());
                }
            },
        }
        if completion.full || self.sheets.dashboard.error.is_some() {
            if let Some(worker) = self.sheets.dashboard.worker.take() {
                let _ = worker.join();
            }
            self.sheets.dashboard.in_flight = None;
        }
    }

    /// Every key while the dashboard is up (after the popups, a pending
    /// confirmation and the help overlay, which own input before it).
    pub(super) fn dashboard_key(&mut self, key: KeyEvent) {
        // The key that opens the dashboard closes it, whatever it is bound to.
        let toggles = self
            .prefs
            .keymap
            .resolve(&[Context::Global], KeyBinding::from_event(key))
            == Some(Action::Dashboard);
        if toggles {
            self.close_dashboard();
            return;
        }
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => self.close_dashboard(),
            KeyCode::Char('?') => self.open_help(),
            KeyCode::Char('t') => self.cycle_window(true),
            KeyCode::Char('T') => self.cycle_window(false),
            KeyCode::Char('r') => self.ensure_stats(true),
            KeyCode::Char('n') => {
                self.sheets.dashboard.show_counts = !self.sheets.dashboard.show_counts;
            },
            KeyCode::Char('k') | KeyCode::Up => {
                self.sheets.dashboard.scroll = self.sheets.dashboard.scroll.saturating_sub(1);
            },
            KeyCode::Char('j') | KeyCode::Down => {
                self.sheets.dashboard.scroll = self.sheets.dashboard.scroll.saturating_add(1);
            },
            KeyCode::PageUp => {
                self.sheets.dashboard.scroll = self.sheets.dashboard.scroll.saturating_sub(PAGE);
            },
            KeyCode::PageDown => {
                self.sheets.dashboard.scroll = self.sheets.dashboard.scroll.saturating_add(PAGE);
            },
            KeyCode::Home => self.sheets.dashboard.scroll = 0,
            KeyCode::End => self.sheets.dashboard.scroll = usize::MAX,
            _ => {},
        }
    }

    /// The wheel scrolls three rows a notch; a left click outside the drawer closes
    /// it (the panes behind are dimmed and not clickable).
    pub(super) fn dashboard_mouse(&mut self, ev: MouseEvent) {
        let point = ratatui::layout::Position::new(ev.column, ev.row);
        match ev.kind {
            MouseEventKind::Down(ratatui::crossterm::event::MouseButton::Left)
                if !self
                    .sheets
                    .overlay
                    .overlay_rect()
                    .is_some_and(|rect| rect.contains(point)) =>
            {
                self.close_dashboard();
            },
            MouseEventKind::ScrollUp => {
                self.sheets.dashboard.scroll =
                    self.sheets.dashboard.scroll.saturating_sub(WHEEL_ROWS);
            },
            MouseEventKind::ScrollDown => {
                self.sheets.dashboard.scroll =
                    self.sheets.dashboard.scroll.saturating_add(WHEEL_ROWS);
            },
            _ => {},
        }
    }

    fn cycle_window(&mut self, forward: bool) {
        let count = WINDOWS.len();
        self.sheets.dashboard.window = if forward {
            (self.sheets.dashboard.window + 1) % count
        } else {
            (self.sheets.dashboard.window + count - 1) % count
        };
        self.sheets.dashboard.scroll = 0;
        self.ensure_stats(false);
    }
}

/// One pass of the statistics behind the panic boundary.
fn read_stats(
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
