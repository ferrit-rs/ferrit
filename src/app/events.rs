//! Event multiplexer: terminal input, filesystem changes and a slow poll
//! fallback, funnelled onto one channel so `App::run` can block on a single
//! `recv()`. This is what gives ferrit lazygit's "notice the world changed"
//! behaviour: stage a file from another shell and the panes update on their
//! own, no keypress needed.

use std::any::Any;
use std::path::Path;
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::Duration;

use color_eyre::Result;
use notify_debouncer_full::notify::RecursiveMode;
use notify_debouncer_full::{DebounceEventResult, new_debouncer};
use ratatui::crossterm::event::{self, Event};

/// One thing worth waking the render loop for.
#[derive(Debug)]
pub enum AppEvent {
    /// A terminal event (key, resize, ...). Redraw, and act on key presses.
    Input(Event),
    /// The repo or worktree changed, or the poll timer fired. Re-snapshot.
    Refresh,
    /// Repository snapshot finished off-thread. Error is flattened here so
    /// the event boundary carries only owned, sendable application data.
    RefreshDone(Box<crate::app::RefreshCompletion>),
    /// Selected diff read finished. Generation and key reject stale results.
    DiffDone(crate::app::diff_query::DiffCompletion),
    /// Selected image blob read/decode finished. Stale generations are dropped.
    ImageDone(crate::app::image_query::ImageCompletion),
    /// A background `fetch`/`pull`/`push` finished. `message` is already a
    /// user-facing string (`Ok` success line or `Err` failure text) — this
    /// module stays git-agnostic, so the spawned thread converts a
    /// `GitError` with `.to_string()` before sending, the same boundary
    /// `App` already draws between itself and `git::`. See
    /// `docs/PLAN_9_REMOTE.md`.
    RemoteDone {
        op: RemoteOp,
        message: Result<String, String>,
    },
}

/// Which of the three network operations finished. Distinct from
/// `git::error::GitError`'s own per-operation variants: this is *which action ran*,
/// not *why it failed* — `App::remote_busy_label` and the eventual result
/// both need to know which of the three is in flight / just finished.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteOp {
    Fetch,
    Pull,
    Push,
}

/// Debounce window for filesystem bursts. `git` touches a dozen files per
/// operation (`index.lock`, `index`, `ORIG_HEAD`, refs, ...); this folds the
/// burst into a single `Refresh`.
const FS_DEBOUNCE: Duration = Duration::from_millis(150);

/// Poll fallback, matching lazygit's default `refresher.refreshInterval`.
/// Covers changes a watcher can miss: network filesystems, dropped inotify
/// events, editors that swap files in place.
const POLL_INTERVAL: Duration = Duration::from_secs(10);

/// Process bursts of keys without repainting after every repeated key, while
/// keeping redraws frequent enough that a paste/repeat storm cannot starve UI.
const MAX_EVENT_BATCH: usize = 256;

/// Live sources feeding `AppEvent`s. Keep the value alive for the whole run:
/// dropping it stops the watcher and lets the sender threads wind down.
pub struct Events {
    /// Kept so `sender()` can hand out more clones; every earlier phase's
    /// source thread already gets its own clone at spawn time, this is the
    /// first thing outside `events.rs` that needs to *send* rather than
    /// just receive (`docs/PLAN_9_REMOTE.md`'s background fetch/pull/push).
    tx: Sender<AppEvent>,
    rx: Receiver<AppEvent>,
    /// Held only to keep the filesystem watch alive; never read. Boxed so the
    /// debouncer's concrete type never leaks into this signature.
    _watch: Option<Box<dyn Any + Send>>,
    watch_error: Option<String>,
}

impl Events {
    /// Wire up input + poll always, plus a recursive watch on `watch_root`
    /// when one is given (absent for a bare repo with no worktree). A watcher
    /// that fails to start is not fatal: input and poll still run, so `r` and
    /// the 10s poll keep the panes fresh.
    pub fn new(watch_root: Option<&Path>) -> Result<Self> {
        let (tx, rx) = mpsc::channel();
        spawn_input(tx.clone());
        spawn_poll(tx.clone());
        let (watch, watch_error) = match watch_root {
            Some(root) => match spawn_watch(tx.clone(), root) {
                Ok(watch) => (watch, None),
                Err(error) => (None, Some(error.to_string())),
            },
            None => (None, None),
        };
        Ok(Self {
            tx,
            rx,
            _watch: watch,
            watch_error,
        })
    }

    /// Block until the next event. `Err` only once every sender is gone.
    pub fn next(&self) -> Result<AppEvent> {
        Ok(self.rx.recv()?)
    }

    /// Block for one event, then collect the already-queued tail up to a fixed
    /// bound. The app handles the batch in channel order and draws once after
    /// it, matching the input-batching pattern used by responsive TUIs.
    pub fn next_batch(&self) -> Result<Vec<AppEvent>> {
        let mut batch = vec![self.rx.recv()?];
        self.drain_batch(&mut batch);
        Ok(batch)
    }

    /// Wait briefly for input while an animated overlay needs regular frames.
    /// `None` means the timeout elapsed without an event.
    pub fn next_batch_timeout(&self, timeout: Duration) -> Result<Option<Vec<AppEvent>>> {
        let first = match self.rx.recv_timeout(timeout) {
            Ok(event) => event,
            Err(RecvTimeoutError::Timeout) => return Ok(None),
            Err(error @ RecvTimeoutError::Disconnected) => return Err(error.into()),
        };
        let mut batch = vec![first];
        self.drain_batch(&mut batch);
        Ok(Some(batch))
    }

    fn drain_batch(&self, batch: &mut Vec<AppEvent>) {
        while batch.len() < MAX_EVENT_BATCH {
            match self.rx.try_recv() {
                Ok(event) => batch.push(event),
                Err(mpsc::TryRecvError::Empty | mpsc::TryRecvError::Disconnected) => break,
            }
        }
    }

    /// A cloneable handle so `App` can hand a background thread a way back
    /// onto this same channel.
    pub fn sender(&self) -> Sender<AppEvent> {
        self.tx.clone()
    }

    /// Startup failure for the optional filesystem watch. Polling stays on.
    pub fn watch_error(&self) -> Option<&str> {
        self.watch_error.as_deref()
    }
}

/// Blocking terminal reader. Runs until stdin dies or the render loop drops
/// its receiver; the process exits right after either way.
fn spawn_input(tx: Sender<AppEvent>) {
    thread::spawn(move || {
        while let Ok(ev) = event::read() {
            if tx.send(AppEvent::Input(ev)).is_err() {
                break;
            }
        }
    });
}

/// Slow heartbeat so the panes never sit stale even when the watcher misses
/// an event.
fn spawn_poll(tx: Sender<AppEvent>) {
    thread::spawn(move || {
        loop {
            thread::sleep(POLL_INTERVAL);
            if tx.send(AppEvent::Refresh).is_err() {
                break;
            }
        }
    });
}

/// Recursive watch on the worktree (which contains `.git`), debounced, with
/// the pure lock-file and object-store churn filtered out so a single stage
/// does not fire two refreshes.
fn spawn_watch(tx: Sender<AppEvent>, root: &Path) -> Result<Option<Box<dyn Any + Send>>> {
    let mut debouncer = new_debouncer(FS_DEBOUNCE, None, move |res: DebounceEventResult| {
        let Ok(events) = res else { return };
        let relevant = events
            .iter()
            .flat_map(|e| e.paths.iter())
            .any(|p| is_relevant(p));
        if relevant {
            let _ = tx.send(AppEvent::Refresh);
        }
    })?;
    debouncer.watch(root, RecursiveMode::Recursive)?;
    Ok(Some(Box::new(debouncer)))
}

/// Skip the noise. `*.lock` files bracket every git write, and `.git/objects/`
/// fills with loose blobs on `add`; the matching `.git/index` write in the
/// same burst still triggers the refresh.
fn is_relevant(path: &Path) -> bool {
    if path.extension().is_some_and(|e| e == "lock") {
        return false;
    }
    !path.to_string_lossy().contains("/.git/objects/")
}
