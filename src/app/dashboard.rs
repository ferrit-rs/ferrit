//! What the keys do in `App` for `dashboard`: the glue between the interface, the git code and the app's state.

use crate::app::App;
use crate::app::events::AppEvent;
use crate::app::keymap::{Action, Context, KeyBinding};
use crate::app::state::dashboard::Cached;
use crate::app::state::dashboard::StatsCompletion;
use crate::app::state::dashboard::read_stats;
use crate::app::state::dashboard::{PAGE, WHEEL_ROWS, WINDOWS};
use crate::app::state::sheet::Sheet;
use ratatui::crossterm::event::KeyCode;
use ratatui::crossterm::event::KeyEvent;
use ratatui::crossterm::event::MouseEvent;
use ratatui::crossterm::event::MouseEventKind;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

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
    pub(crate) fn prepare_dashboard_sheet(&mut self) {
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
    pub(crate) fn on_stats_done(&mut self, completion: StatsCompletion) {
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
    pub(crate) fn dashboard_key(&mut self, key: KeyEvent) {
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
    pub(crate) fn dashboard_mouse(&mut self, ev: MouseEvent) {
        let point = ratatui::layout::Position::new(ev.column, ev.row);
        match ev.kind {
            MouseEventKind::Down(ratatui::crossterm::event::MouseButton::Left)
                if !self
                    .render
                    .sheet
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
