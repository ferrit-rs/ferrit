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

use crate::tui::error::AppError;
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
    RefreshDone(Box<crate::tui::RefreshCompletion>),
    /// Selected diff read finished. Generation and key reject stale results.
    DiffDone(crate::tui::components::diff::queries::DiffCompletion),
    /// Selected image blob read/decode finished. Stale generations are dropped.
    ImageDone(crate::tui::components::diff::queries::ImageCompletion),
    /// A background `fetch`/`pull`/`push` finished. `message` is already a
    /// user-facing string (`Ok` success line or `Err` failure text) — this
    /// module stays git-agnostic, so the spawned thread converts a
    /// `GitError` with `.to_string()` before sending, the same boundary
    /// `App` already draws between itself and `git::`. See
    /// `docs/PLAN_9_REMOTE.md`.
    /// A git child asked for a passphrase, password or host-key answer;
    /// the answer (`None` cancels) goes back through `reply`.
    Askpass {
        prompt: String,
        reply: Sender<Option<String>>,
    },
    /// The dashboard statistics worker answered (`app::dashboard`).
    StatsDone(crate::tui::components::dashboard::state::StatsCompletion),
    RemoteDone {
        op: crate::git::remote::RemoteOp,
        message: Result<String, AppError>,
    },
    /// `gh repo create` finished: the repository's web URL, or why not
    /// (`app::create_remote`).
    RemoteCreated(Result<String, AppError>),
    /// `gh` was asked whether it is installed and signed in; `generation`
    /// tells which popup asked (`app::create_remote`).
    GhChecked {
        generation: u64,
        status: crate::git::host::GhStatus,
    },
}

/// Debounce window for filesystem bursts. `git` touches a dozen files per
/// operation (`index.lock`, `index`, `ORIG_HEAD`, refs, ...); this folds the
/// burst into a single `Refresh`.
const FS_DEBOUNCE: Duration = Duration::from_millis(150);

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
    /// Held only to keep the filesystem watch alive. Boxed so the debouncer's
    /// concrete type never leaks into this signature.
    watcher: Option<Box<dyn Any + Send>>,
    watch_error: Option<String>,
}

impl Events {
    /// Wire up input + poll always, plus a recursive watch on `watch_root`
    /// when one is given (absent for a bare repo with no worktree). A watcher
    /// that fails to start is not fatal: input and poll still run, so `r` and
    /// the 10s poll keep the panes fresh.
    pub fn new(watch_root: Option<&Path>, poll: Duration) -> Result<Self> {
        let (tx, rx) = mpsc::channel();
        spawn_input(tx.clone());
        spawn_poll(tx.clone(), poll);
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
            watcher: watch,
            watch_error,
        })
    }

    /// Point the filesystem watch at `root`, replacing the one there was (none,
    /// when ferrit started outside a repository and a `git init` has since made
    /// one). A watcher that fails to start is the same non-fatal polling
    /// fallback as at startup: `watch_error` says so.
    pub fn watch(&mut self, root: &Path) {
        let (watch, error) = match spawn_watch(self.tx.clone(), root) {
            Ok(watch) => (watch, None),
            Err(error) => (None, Some(error.to_string())),
        };
        self.watcher = watch;
        self.watch_error = error;
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
        self.watcher
            .is_none()
            .then_some(self.watch_error.as_deref())
            .flatten()
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
/// an event: network filesystems, dropped inotify events, editors that swap
/// files in place. `poll` is `[ui] poll_secs`, 10 seconds by default, matching
/// lazygit's `refresher.refreshInterval`.
fn spawn_poll(tx: Sender<AppEvent>, poll: Duration) {
    thread::spawn(move || {
        loop {
            thread::sleep(poll);
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

#[cfg(test)]
mod tests {
    use std::sync::mpsc;
    use std::time::Duration;

    use crate::tui::events::{AppEvent, spawn_poll};

    #[test]
    fn the_poll_fires_at_the_configured_interval_not_a_fixed_one() {
        let (tx, rx) = mpsc::channel();
        spawn_poll(tx, Duration::from_millis(20));
        // Ten seconds would be the old fixed interval; a 20 ms poll answers at
        // once, and keeps answering.
        for _ in 0..3 {
            assert!(
                matches!(
                    rx.recv_timeout(Duration::from_secs(2)),
                    Ok(AppEvent::Refresh)
                ),
                "a poll tick"
            );
        }
    }
}
