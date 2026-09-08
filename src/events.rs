//! Event multiplexer: terminal input, filesystem changes and a slow poll
//! fallback, funnelled onto one channel so `App::run` can block on a single
//! `recv()`. This is what gives ferrit lazygit's "notice the world changed"
//! behaviour: stage a file from another shell and the panes update on their
//! own, no keypress needed.

use std::any::Any;
use std::path::Path;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Duration;

use color_eyre::Result;
use notify_debouncer_full::notify::RecursiveMode;
use notify_debouncer_full::{DebounceEventResult, new_debouncer};
use ratatui::crossterm::event::{self, Event};

/// One thing worth waking the render loop for.
pub enum AppEvent {
    /// A terminal event (key, resize, ...). Redraw, and act on key presses.
    Input(Event),
    /// The repo or worktree changed, or the poll timer fired. Re-snapshot.
    Refresh,
}

/// Debounce window for filesystem bursts. `git` touches a dozen files per
/// operation (`index.lock`, `index`, `ORIG_HEAD`, refs, ...); this folds the
/// burst into a single `Refresh`.
const FS_DEBOUNCE: Duration = Duration::from_millis(150);

/// Poll fallback, matching lazygit's default `refresher.refreshInterval`.
/// Covers changes a watcher can miss: network filesystems, dropped inotify
/// events, editors that swap files in place.
const POLL_INTERVAL: Duration = Duration::from_secs(10);

/// Live sources feeding `AppEvent`s. Keep the value alive for the whole run:
/// dropping it stops the watcher and lets the sender threads wind down.
pub struct Events {
    rx: Receiver<AppEvent>,
    /// Held only to keep the filesystem watch alive; never read. Boxed so the
    /// debouncer's concrete type never leaks into this signature.
    _watch: Option<Box<dyn Any + Send>>,
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
        let watch = match watch_root {
            Some(root) => spawn_watch(tx, root)?,
            None => None,
        };
        Ok(Self { rx, _watch: watch })
    }

    /// Block until the next event. `Err` only once every sender is gone.
    pub fn next(&self) -> Result<AppEvent> {
        Ok(self.rx.recv()?)
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
