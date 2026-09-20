//! Fetch / pull / push: background remote ops and their completion.

use super::{App, AppEvent, Popup, RemotePick, Result, events, mpsc, thread};

impl App {
    /// `f` / `p`: fetch / pull. Global, not Branches-only — unlike phase
    /// 8's branch actions, these act on the repo and its current branch,
    /// not a selected row. A no-op with no `event_sender` set
    /// (`App::mock()`, or a test driving `on_key` without `run()`).
    pub(super) fn trigger_remote_op(&mut self, op: events::RemoteOp) {
        let Some(sender) = self.event_sender.clone() else {
            return;
        };
        self.start_remote_op(op, None, sender);
    }

    /// `P`: push. Unlike `f`/`p`, push needs to know *before* running
    /// whether the current branch has an upstream at all (`self.header
    /// .upstream`, already read by phase 2 for the ahead/behind count) —
    /// `Repo::push`'s own `NoUpstream` detection exists as a defensive
    /// fallback, not the primary path, because ferrit already knows the
    /// answer without asking git. No upstream: 0 remotes is an immediate
    /// `last_error`, 1 remote pushes straight there with `-u`, 2+ opens
    /// `Popup::RemotePick` to choose one.
    pub(super) fn push_current_branch(&mut self) {
        if self.popup.is_some() {
            return;
        }
        if self.header.upstream.is_some() {
            self.trigger_remote_op(events::RemoteOp::Push);
            return;
        }
        let Some(repo) = &self.repo else { return };
        match repo.remotes() {
            Ok(remotes) if remotes.is_empty() => {
                self.last_error = Some("no remote configured".to_owned());
            },
            Ok(mut remotes) if remotes.len() == 1 => {
                self.push_with_upstream(remotes.remove(0).name);
            },
            Ok(remotes) => {
                self.popup = Some(Popup::RemotePick(RemotePick {
                    remotes,
                    selected: 0,
                }));
            },
            Err(e) => self.last_error = Some(e.to_string()),
        }
    }

    /// `git push -u <remote> <branch>`: the 1-remote short-circuit and
    /// `Popup::RemotePick`'s `Enter` both land here.
    pub(super) fn push_with_upstream(&mut self, remote: String) {
        let Some(sender) = self.event_sender.clone() else {
            return;
        };
        self.start_remote_op(events::RemoteOp::Push, Some(remote), sender);
    }

    /// Spawn `op` on its own thread, `sender` its way back onto the same
    /// channel `Events::next()` reads (`docs/PLAN_9_REMOTE.md`, "Approach
    /// part 2": network calls are the first slow ones in `git::`, and
    /// running one on this thread would freeze the whole UI). One at a
    /// time: a second call while one is already running is ignored
    /// outright, not queued — two git processes racing over the same
    /// `index.lock` is a real failure mode, not a hypothetical one. A
    /// no-op with no repo to reopen (`App::mock()`, a bare repo).
    /// `push_upstream` is only ever `Some` for `RemoteOp::Push`, from
    /// `push_with_upstream`; `f`/`p`/a plain `P` all pass `None`.
    ///
    /// Takes `sender` as a parameter rather than reading `self.event_sender`
    /// directly so a test can call this with its own channel, no `run()`
    /// (and its `Events`) required. `pub`, integration-test seam like
    /// `feed_key`: a test drives the resulting `AppEvent::RemoteDone`
    /// itself, into `on_remote_done`, with no `run()` loop to receive it.
    #[doc(hidden)]
    pub fn start_remote_op(
        &mut self,
        op: events::RemoteOp,
        push_upstream: Option<String>,
        sender: mpsc::Sender<AppEvent>,
    ) {
        if self.remote_busy.is_some() {
            return;
        }
        let Some(repo) = self.repo_handle() else {
            return;
        };
        self.remote_busy = Some(op);
        self.status_note = None;
        thread::spawn(move || {
            let result = match op {
                events::RemoteOp::Fetch => repo.fetch(None),
                events::RemoteOp::Pull => repo.pull(),
                events::RemoteOp::Push => repo.push(push_upstream.as_deref()),
            };
            let message = result.map_err(|e| e.to_string());
            let _ = sender.send(AppEvent::RemoteDone { op, message });
        });
    }

    /// `AppEvent::RemoteDone` arrived: clear the busy flag, show a success
    /// line or the failure, then refresh — ahead/behind, branches, commits
    /// and files may all have moved (`pull` can fast-forward or rebase
    /// local commits; `push` moves nothing local but the ahead count
    /// changes). `pub`: `App::run`'s own match arm calls this, and so does
    /// a test that drove `start_remote_op` with its own channel and has no
    /// `run()` loop to receive the result for it.
    pub fn on_remote_done(&mut self, _op: events::RemoteOp, message: Result<String, String>) {
        self.remote_busy = None;
        // `refresh()` first, not last: it sets `last_error` on its own
        // (`None` on a successful snapshot, `Some` on a failed one), and
        // the remote op's own message is the one that should have the
        // final word on what the Status pane shows — reversing the order
        // would let a routine post-op `refresh()` silently clear the
        // very failure line it is meant to report.
        self.refresh();
        match message {
            Ok(line) => {
                self.last_error = None;
                self.status_note = Some(line);
            },
            Err(line) => {
                self.status_note = None;
                self.last_error = Some(line);
            },
        }
    }

    /// A short label for the Status pane while a fetch/pull/push is in
    /// flight, or `None` when none is. Not a progress bar: ferrit has no
    /// way to know fetch/push percentages without parsing git's
    /// `--progress` stream, which is meant for a terminal's own
    /// carriage-return redraws, not structured data.
    pub fn remote_busy_label(&self) -> Option<&'static str> {
        match self.remote_busy? {
            events::RemoteOp::Fetch => Some("Fetching\u{2026}"),
            events::RemoteOp::Pull => Some("Pulling\u{2026}"),
            events::RemoteOp::Push => Some("Pushing\u{2026}"),
        }
    }
}
