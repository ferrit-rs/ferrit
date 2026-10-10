//! Dashboard statistics state, loading and input handling.

use crate::git::Snapshot;
use crate::git::port::GitPort;
use crate::git::stats::{RepoStats, StatsOptions, Window};
use crate::tui::error::AppError;
use crate::tui::event::Event;
use crate::tui::events::AppEvent;
use crate::tui::workers::{WorkerKind, run_worker};
use ratatui::crossterm::event::MouseButton;
use ratatui::crossterm::event::{KeyCode, KeyEvent, MouseEvent, MouseEventKind};
use ratatui::layout::Position;
use ratatui::layout::Rect;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread;
use std::thread::JoinHandle;

fn refs_fingerprint(snapshot: &Snapshot) -> u64 {
    let mut hasher = DefaultHasher::new();
    snapshot.header.branch.hash(&mut hasher);
    snapshot
        .commits
        .first()
        .map(|commit| &commit.full_hash)
        .hash(&mut hasher);
    for branch in &snapshot.branches {
        (&branch.name, branch.tip_time, branch.ahead, branch.behind).hash(&mut hasher);
    }
    snapshot.stashes.len().hash(&mut hasher);
    hasher.finish()
}

/// What the dashboard needs of the app to ask for statistics.
pub(crate) struct StatsCtx<'a> {
    pub(crate) snapshot: &'a Snapshot,
    pub(crate) repo: Option<&'a dyn GitPort>,
    pub(crate) sender: Option<mpsc::Sender<AppEvent>>,
    pub(crate) open: bool,
}

impl Dashboard {
    pub(crate) fn prepare(&mut self, ctx: &StatsCtx<'_>) {
        self.error = None;
        self.scroll = 0;
        self.ensure_stats(false, ctx);
    }

    pub(crate) fn clamp_scroll(&mut self, max: usize) {
        self.scroll = self.scroll.min(max);
    }

    fn ensure_stats(&mut self, force: bool, ctx: &StatsCtx<'_>) {
        let fingerprint = refs_fingerprint(ctx.snapshot);
        if fingerprint != self.fingerprint {
            self.cache = std::array::from_fn(|_| None);
            self.fingerprint = fingerprint;
        }
        let slot = self.window;
        if !force && self.cached().is_some_and(|cache| !cache.churn_pending) {
            return;
        }
        if !force && self.in_flight == Some(slot) {
            return;
        }
        if force {
            self.store(slot, None);
        }
        self.cancel_running();
        self.generation += 1;
        self.cancel = Arc::new(AtomicBool::new(false));
        self.in_flight = Some(slot);
        self.error = None;

        let Some(repo) = ctx.repo.and_then(|repo| repo.reopen().ok()) else {
            self.in_flight = None;
            return;
        };
        let generation = self.generation;
        let window = self.window();
        let cancel = Arc::clone(&self.cancel);
        let Some(sender) = ctx.sender.clone() else {
            for full in [false, true] {
                let result = read_stats(repo.as_ref(), window, full, &cancel);
                self.on_done(
                    StatsCompletion {
                        generation,
                        window,
                        full,
                        result,
                    },
                    ctx.open,
                );
            }
            return;
        };
        self.worker = Some(thread::spawn(move || {
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

    pub(crate) fn on_done(&mut self, completion: StatsCompletion, open: bool) {
        if completion.generation != self.generation {
            return;
        }
        let Some(slot) = WINDOWS
            .iter()
            .position(|window| *window == completion.window)
        else {
            return;
        };
        match completion.result {
            Ok(stats) => {
                self.store(
                    slot,
                    Some(Cached {
                        stats: *stats,
                        churn_pending: !completion.full,
                    }),
                );
                self.error = None;
            },
            Err(message) => {
                if open {
                    self.error = Some(message.to_string());
                }
            },
        }
        if completion.full || self.error.is_some() {
            if let Some(worker) = self.worker.take() {
                let _ = worker.join();
            }
            self.in_flight = None;
        }
    }

    pub(crate) fn key(&mut self, key: KeyEvent, toggles: bool, ctx: &StatsCtx<'_>) -> Vec<Event> {
        if toggles {
            return vec![Event::CloseDashboard];
        }
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => return vec![Event::CloseDashboard],
            KeyCode::Char('?') => return vec![Event::OpenHelp],
            KeyCode::Char('t') => self.cycle_window(true, ctx),
            KeyCode::Char('T') => self.cycle_window(false, ctx),
            KeyCode::Char('r') => self.ensure_stats(true, ctx),
            KeyCode::Char('n') => self.show_counts = !self.show_counts,
            KeyCode::Char('k') | KeyCode::Up => self.scroll = self.scroll.saturating_sub(1),
            KeyCode::Char('j') | KeyCode::Down => self.scroll = self.scroll.saturating_add(1),
            KeyCode::PageUp => self.scroll = self.scroll.saturating_sub(PAGE),
            KeyCode::PageDown => self.scroll = self.scroll.saturating_add(PAGE),
            KeyCode::Home => self.scroll = 0,
            KeyCode::End => self.scroll = usize::MAX,
            _ => {},
        }
        Vec::new()
    }

    pub(crate) fn mouse(&mut self, ev: MouseEvent, overlay: Option<Rect>) -> Vec<Event> {
        let point = Position::new(ev.column, ev.row);
        match ev.kind {
            MouseEventKind::Down(MouseButton::Left)
                if !overlay.is_some_and(|rect| rect.contains(point)) =>
            {
                return vec![Event::CloseDashboard];
            },
            MouseEventKind::ScrollUp => self.scroll = self.scroll.saturating_sub(WHEEL_ROWS),
            MouseEventKind::ScrollDown => self.scroll = self.scroll.saturating_add(WHEEL_ROWS),
            _ => {},
        }
        Vec::new()
    }

    fn cycle_window(&mut self, forward: bool, ctx: &StatsCtx<'_>) {
        let count = WINDOWS.len();
        self.window = if forward {
            (self.window + 1) % count
        } else {
            (self.window + count - 1) % count
        };
        self.scroll = 0;
        self.ensure_stats(false, ctx);
    }
}

pub(crate) const WINDOWS: [Window; 5] = [
    Window::Days7,
    Window::Days30,
    Window::Days90,
    Window::Year,
    Window::All,
];
pub(crate) const DEFAULT_WINDOW: usize = 2;
pub(crate) const PAGE: usize = 10;
pub(crate) const WHEEL_ROWS: usize = 3;

#[derive(Debug)]
pub struct StatsCompletion {
    pub(crate) generation: u64,
    pub(crate) window: Window,
    pub(crate) full: bool,
    pub(crate) result: Result<Box<RepoStats>, AppError>,
}

#[derive(Debug)]
pub(crate) struct Cached {
    pub(crate) stats: RepoStats,
    pub(crate) churn_pending: bool,
}

#[derive(Debug)]
pub struct Dashboard {
    pub(crate) window: usize,
    pub(crate) cache: [Option<Cached>; WINDOWS.len()],
    pub(crate) generation: u64,
    pub(crate) cancel: Arc<AtomicBool>,
    pub(crate) worker: Option<JoinHandle<()>>,
    pub(crate) in_flight: Option<usize>,
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

    pub(crate) fn store(&mut self, slot: usize, value: Option<Cached>) {
        if let Some(entry) = self.cache.get_mut(slot) {
            *entry = value;
        }
    }

    pub fn stats(&self) -> Option<&RepoStats> {
        self.cached().map(|cache| &cache.stats)
    }

    pub fn churn_pending(&self) -> bool {
        self.cached().is_some_and(|cache| cache.churn_pending)
    }

    pub fn computing(&self) -> bool {
        self.cached().is_none() && self.in_flight.is_some()
    }

    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    pub fn scroll(&self) -> usize {
        self.scroll
    }

    pub fn show_counts(&self) -> bool {
        self.show_counts
    }

    pub(crate) fn is_busy(&self) -> bool {
        self.in_flight.is_some()
    }

    pub(crate) fn cancel_running(&mut self) {
        self.cancel.store(true, Ordering::Release);
        self.worker = None;
        self.in_flight = None;
    }

    pub(crate) fn stop_and_join(&mut self) {
        self.cancel.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        self.in_flight = None;
    }
}

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
