# Plan: phase 9, remote

**Deviation: a bare `git fetch` with zero remotes is a silent no-op, not
an error.** The "Edge cases" table below guessed `git`'s message would be
something like "no remote repository specified". Checked empirically: with
no remotes configured, `git fetch` (no arguments) has nothing to do and
exits 0 with empty output. `git pull` still fails in that case (it needs
*something* to merge from) with "no tracking information for the current
branch", so `P`'s existing 0-remotes short-circuit (`repo.remotes()` before
ever asking git) is the only place this actually mattered; `f` needed no
special-casing at all.

**Deviation: a conflicting `pull` is a `PullFailed` error, not a dedicated
outcome.** The plan below asks for pull-conflict to get "the same treatment
as phase 8's conflicted merge" — a non-error outcome the UI shows as an
informational note rather than a failure. Shipped simpler: `pull()` has no
`PullOutcome` mirroring `branch::MergeOutcome`, so a conflicting pull
(merge- or rebase-based, both exit non-zero on conflict, checked
empirically the same way phase 8's merge exit code was) surfaces as an
ordinary `GitError::PullFailed`, git's own conflict message shown verbatim
in `last_error`. Still satisfies "not resolved, not pretended-resolved" —
the Files pane already shows `Change::Conflicted` after the follow-up
`refresh()` regardless — just styled as an error rather than a neutral
note. Revisit alongside phase 11's real conflict-resolution UI if the
distinction turns out to matter in practice.

## Goal

Talk to a remote: fetch, pull, push, and a "Remotes" tab on the Branches
pane that lists what `origin` (and friends) actually are. Upstream tracking
and ahead/behind counts are *not* new work here — phase 2 (`refs.rs`,
`status.rs`) already reads them for the Status header and every branch row.
What phase 8's local fast-forward (`docs/PLAN_8_BRANCHES.md`) could not do
is make that remote-tracking data *current*: it only ever moves a ref from
what is already on disk. Phase 9 is what puts fresh data there, plus the
two actions that were always going to need the network no matter how much
of the rest ferrit could do locally.

Scope is `PLAN_0_GENERAL.md`'s phase 9 line: "fetch, pull, push, upstream
tracking, ahead/behind" — the last two read as "make sure phase 2's numbers
stay meaningful once fetch/pull/push exist to change them," not new reads.
Out, on purpose:

- **Adding, removing, or renaming a remote.** `git remote add/remove/rename`
  are one-shot setup commands a user runs once per clone, not a repeated
  TUI action — and `PLAN_0_GENERAL.md`'s own north star says ferrit is "not
  a git porcelain replacement on the command line." The Remotes tab reads;
  it does not manage. Revisit if real usage says otherwise.
- **Checkout from a remote-tracking ref** (`origin/feat-x` with no local
  branch yet). Needs this phase's remote list *and* phase 8's checkout
  machinery; a short follow-up once both exist, not core to either.
- **Interactive rebase on pull** (`pull.rebase` beyond honouring whatever
  the user already configured — see "Approach"). Phase 11 territory.
- **Force-push.** Real footgun, no design consensus yet on what guard rail
  (if any) makes it safe enough for a keybinding; left for a deliberate
  follow-up rather than bolted on here. `git push --force-with-lease` from
  the shell still works today, same as everything ferrit does not (yet)
  wrap.
- **Multi-remote push/fetch target picker.** S1 handles the common case (0
  or 1 remote, or an already-set upstream); see "Deferred out of phase 9".

## Approach part 1: honour the user's config, same as `diff`/`commit`

`git pull` with no flags does whatever `pull.rebase` / `pull.ff` say to do.
ferrit passes none of `--rebase`, `--no-rebase`, `--ff`, `--no-ff`: same
reasoning as `docs/PLAN_3_DIFF_VIEW.md` letting the user's `diff.*` config
drive `git diff`, and `docs/PLAN_7_COMMIT.md` letting `commit.gpgSign` drive
`git commit`. A ferrit user who set `pull.rebase = true` in their global
config gets rebase-on-pull from ferrit without ferrit knowing or caring
that preference exists.

## Approach part 2: this is the first *slow* git operation — do not block

Every subprocess ferrit has shelled out to so far (`diff`, `apply`,
`commit`, phase 8's `checkout`/`branch`/`merge`) is local and fast: reading
or writing objects on disk, done in milliseconds. `fetch`/`pull`/`push`
cross the network — anywhere from instant to "the connection is bad and
this hangs for ten seconds" to "waiting on an SSH passphrase or a 2FA
prompt that will never come because ferrit did not forward a TTY for it."
Running one on `App::run`'s single thread, the way every earlier phase's
`Repo` calls do, would freeze the whole UI — including the keypress needed
to notice something is stuck.

`PLAN_0_GENERAL.md`'s own "What ferrit should take from all of this" names
this exact discipline: "gitui's async-git-off-the-UI-thread". This is the
phase that actually needs it, because it is the first phase where the *no*
readable answer ("done" or "failed") arrives in milliseconds.

`src/events.rs` already multiplexes independent sources onto one channel
(`Events`, `AppEvent`) so `App::run` can block on a single `recv()` — fetch,
pull and push become a fourth source, not a new synchronization primitive:

```rust
// src/events.rs
pub enum AppEvent {
    Input(Event),
    Refresh,
    /// A background `fetch`/`pull`/`push` finished. `message` is already a
    /// user-facing string (`Ok` success line or `Err` failure text) —
    /// events.rs stays git-agnostic, so the spawned thread converts
    /// `GitError` with `.to_string()` before sending, the same boundary
    /// `App` already draws between itself and `git::`.
    RemoteDone { op: RemoteOp, message: Result<String, String> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteOp { Fetch, Pull, Push }

impl Events {
    /// A cloneable handle so `App` can hand a background thread a way back
    /// onto the same channel `next()` reads. Exists only for this; nothing
    /// else outside `events.rs` has needed to *send* before now.
    pub fn sender(&self) -> Sender<AppEvent> { self.tx.clone() }
}
```

(`Events` currently drops its `Sender` after spawning the input/poll/watch
threads; it starts keeping one field, `tx: Sender<AppEvent>`, to hand out.)

`App` spawns the op on `std::thread::spawn`, not `tokio`/`async-std`: one
blocking call, one result, no cancellation, no concurrency inside the
thread — exactly what `std::thread` is for, and gitui's own "worker
threads" discipline (`INSPIRATION.md`) is threads, not an async runtime.
Adding an executor for one blocking subprocess call would be the opposite
of `AGENTS.md`'s "the best code is the code you never wrote."

```rust
fn start_remote_op(&mut self, op: RemoteOp, sender: Sender<AppEvent>) {
    if self.remote_busy.is_some() {
        return; // one at a time; see "Edge cases"
    }
    self.remote_busy = Some(op);
    let Some(repo) = self.repo_handle() else { return }; // see below
    thread::spawn(move || {
        let result = match op {
            RemoteOp::Fetch => repo.fetch(None),
            RemoteOp::Pull => repo.pull(),
            RemoteOp::Push => repo.push(false),
        };
        let message = result.map_err(|e| e.to_string());
        let _ = sender.send(AppEvent::RemoteDone { op, message });
    });
}
```

`repo_handle()`: the spawned closure needs its own handle to open the
repository, not a borrow of `self.repo` (an `App` field cannot cross a
`thread::spawn`'s `'static` bound). `git::Repo::open(path)` is cheap
(`git2::Repository::discover`, no I/O beyond opening `.git`), so the
closure just re-opens the same path rather than `App` reaching for
`Arc<Mutex<Repo>>` — no shared mutable state, no lock, nothing for the
main thread's own `Repo` calls (still all synchronous, still all fast) to
contend with. This is the same "cheap enough to redo" call `App::open`
already makes once at startup.

## Backend: `src/git/remote.rs`

New module under `src/git/`, sibling of `branch.rs`. No `ratatui`.
Subprocess `git`, for the same reason `apply.rs`/`commit.rs`/`branch.rs`
all are: hooks (`post-receive` is the *server's* hook, out of ferrit's
hands either way, but `pre-push` is local and must run), and — the reason
that actually matters here — **credential handling**. `git2`'s remote
callbacks require the caller to implement SSH-agent lookup, credential
helpers, and interactive prompts by hand; the user's own `git` already has
all of that solved (`credential.helper`, `core.sshCommand`, an
`askpass` program, whatever they configured once and forgot about).
Reimplementing it is real security-sensitive work for a strictly worse
result. Every git TUI that supports push (lazygit included) shells out for
exactly this.

```rust
//! Fetch, pull and push by shelling out to `git`, so credentials
//! (SSH agent, credential helpers, askpass), hooks (`pre-push`), and
//! pull.rebase-style config all work the way they do for the user's own
//! git. See docs/PLAN_9_REMOTE.md. Run off the main thread — see that
//! plan's "Approach part 2".

/// One configured remote, `git remote -v`'s own model (fetch and push URLs
/// can differ; usually do not).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteEntry {
    pub name: String,
    pub fetch_url: String,
    pub push_url: String,
}

impl Repo {
    /// Configured remotes, alphabetical. Read via `git2` (`repo.remotes()`
    /// + `find_remote`), not a subprocess: listing config needs no
    /// credentials and no hooks, the same reasoning phase 2's `refs.rs`
    /// already applies to reading local branches.
    pub fn remotes(&self) -> GitResult<Vec<RemoteEntry>>;

    /// `git fetch <remote>`, or plain `git fetch` (every remote, git's own
    /// default) when `remote` is `None`.
    pub fn fetch(&self, remote: Option<&str>) -> GitResult<String>;

    /// `git pull`. No flags: `pull.rebase` / `pull.ff` decide the shape,
    /// same as any other `git config` this backend already defers to.
    pub fn pull(&self) -> GitResult<String>;

    /// `git push`, or `git push -u <remote> <branch>` when `set_upstream`
    /// is true (the current branch has none yet — see `GitError::NoUpstream`
    /// below). `<remote>` for the `-u` form is chosen by `App`, not here;
    /// see "No upstream: push picks (or asks for) a remote".
    pub fn push(&self, set_upstream: Option<&str>) -> GitResult<String>;
}
```

`fetch`/`pull`/`push` return `GitResult<String>` — unlike phase 6/7/8's
`GitResult<()>` — because success itself is worth a line in the Status
pane (`"Fetched origin"`, `"Already up to date."`, `"3 commits pushed to
origin/main"`), the same instinct `git`'s own CLI output already has; the
`String` is `stdout` + `stderr` trimmed and joined (git puts progress and
the human summary on `stderr`, the machine-parseable bits, when there are
any, on `stdout` — ferrit does not parse either, just shows them).

`GitError` gains:

```rust
#[error("git fetch failed: {0}")]
FetchFailed(String),
#[error("git pull failed: {0}")]
PullFailed(String),
#[error("git push failed: {0}")]
PushFailed(String),
/// The current branch has no upstream to push to. Distinct from
/// `PushFailed` because git's message for this is stable
/// ("fatal: The current branch <name> has no upstream branch.", exit 128)
/// and ferrit acts on it specifically (offers `-u`), the same shape as
/// phase 7's `NothingStaged` next to the generic `CommitFailed`.
#[error("no upstream configured for the current branch")]
NoUpstream,
```

Three variants instead of one shared `RemoteFailed`, unlike phase 7's
collapse of `HookRejected` into `CommitFailed`: fetch, pull and push fail
for different *reasons* a user benefits from telling apart at a glance in
the Status pane (a fetch that can't reach the network vs. a push rejected
as non-fast-forward are not the same problem), where a commit failure and
a hook failure read the same either way ("the commit did not happen, here
is why"). The one truly indistinguishable-and-not-worth-it case (auth
failure vs. a mid-transfer network drop vs. a server-side `pre-receive`
hook) still lands as a generic `PushFailed(String)` with git's message
verbatim — same policy as before, just not the *only* variant.

## App wiring

### `f` / `p` / `P`: global, not Branches-only

Reserved since phase 1's mock keybar (`Push: P | Pull: p`, present before
any of it was real) and, unlike phase 8's branch actions, meaningful
regardless of which pane has focus — fetch/pull/push act on the repo and
its current branch, not on a selected row. `on_key` gets these three
checked early, the same tier as `r` (refresh) already is, not inside the
per-pane match:

```
f  -> start_remote_op(Fetch)
p  -> start_remote_op(Pull)
P  -> start_remote_op(Push)     (routes through NoUpstream handling first)
```

### No upstream: push picks (or asks for) a remote

```
P pressed, current branch has no upstream
  -> repo.remotes()
       0 remotes -> last_error "no remote configured"
       1 remote  -> start_remote_op(Push) with set_upstream: Some(that name)
       2+        -> Popup::RemotePick(remotes, selected index)
                    Enter on a highlighted remote name, rendered by the
                    reusable `components::ui::SelectList`.
```

The 2+-remotes picker is the one piece of real UI this phase adds beyond
"press a letter, wait, see a status line" — everything else reuses
`Popup::Note` (failure) and a new `busy` status line (in flight), both
already-established shapes.

### One remote op at a time

```rust
/// `Some` while a background fetch/pull/push is running. `f`/`p`/`P`
/// pressed again while `Some` are ignored outright — not queued, not
/// stacked — the same "do nothing rather than something surprising"
/// choice phase 6 made for a double-apply mid-refresh.
remote_busy: Option<RemoteOp>,
```

Two git processes racing over the same `index.lock` is a real failure
mode, not a hypothetical one; refusing a second op while one is in flight
sidesteps it entirely rather than trying to serialize or merge them.

### Handling `AppEvent::RemoteDone`

```
App::run's match:
  AppEvent::RemoteDone { op, message } =>
    self.remote_busy = None
    match message:
      Ok(line)  -> self.last_error = None; self.status_note = Some(line)
      Err(line) -> self.last_error = Some(line)
    self.refresh()     // ahead/behind, branches, commits, files all
                        // potentially moved (pull can fast-forward or
                        // rebase local commits; push moves nothing local
                        // but the ahead count changes)
```

`status_note`: a new, small sibling of `last_error` — a *success* line
("Fetched origin", "3 commits pushed") shown the same place, styled with
`theme::status_line`'s green rather than `theme::error_line`'s red, cleared
by the next keypress or the next `refresh()`. Phase 2's Status pane already
special-cases `last_error`; this is the same mechanism with a second,
non-error slot rather than overloading one field with a colour flag.

### The busy indicator

```rust
pub fn remote_busy_label(&self) -> Option<&'static str> {
    match self.remote_busy? {
        RemoteOp::Fetch => Some("Fetching…"),
        RemoteOp::Pull => Some("Pulling…"),
        RemoteOp::Push => Some("Pushing…"),
    }
}
```

Rendered as a Status-pane line and inline beside the checked-out branch,
with a short animated loader like lazygit. It remains an activity indicator,
not a progress bar: ferrit has no way to know fetch/push percentages without
parsing git's `--progress` output, which is meant for a terminal's own
carriage-return redraws, not structured data.

## Rendering (`src/ui.rs`)

- Status pane: `remote_busy_label()` and `status_note()` render as an extra
  line under the existing `ferrit -> main ↑2` line. The checked-out branch
  also shows an animated `Pushing` / `Pulling` / `Fetching` indicator.
  Busy status stays visible alongside an earlier error; success and busy
  status remain mutually exclusive.
- **Remotes tab**: the Branches pane's `[3] Local branches - Remotes -
  Tags` title gains a real second tab. `Tab`/`BackTab` already cycle
  *panes*; a new pane-*internal* tab needs its own key — `Ctrl-Right` /
  `Ctrl-Left`, unclaimed, and already the "the shifted version of an
  existing movement key means a bigger/different move" pattern
  (`Shift-Tab` reverses `Tab`, `Ctrl-d`/`Ctrl-u` half-page vs. `J`/`K`
  single-line). Rows: `name  fetch: <url>  push: <url>` (push URL omitted
  when identical to fetch, the common case). No selection cursor needed
  yet — nothing is actionable here in phase 9 (see "Out, on purpose");
  it is `Repo::remotes()` rendered plainly, the same "just a list" shape
  the Local Branches tab had for the entirety of phase 2.
- Shared `Panel`, `SelectList` and `KeyBar` components own rounded panel
  shells, selected-row fill and key-hint styling. The remote picker uses
  `SelectList`; other panes and popup footers use the same building blocks.
- Keybar (`mock::KEYBAR`) regains `Fetch: f | Pull: p | Push: P` (the
  phase-1 placeholders phase 6 trimmed for space, now real); `HELP`
  documents the busy/no-upstream/multi-remote-picker behaviour.

## Keybindings (new in phase 9)

| Key | Context | Action |
| --- | --- | --- |
| `f` | Nav, any pane | fetch (all remotes) |
| `p` | Nav, any pane | pull (honours `pull.rebase`/`pull.ff`) |
| `P` | Nav, any pane | push; offers `-u <remote>` when there is no upstream yet |
| `Ctrl-Right` / `Ctrl-Left` | Nav, Branches focused | switch the pane's own tab (Local branches / Remotes) |
| `Enter` | `Popup::RemotePick` | push with `-u` to the highlighted remote |
| `Esc` | `Popup::RemotePick` | cancel, no push |

`f`/`p`/`P` are inert (no-op, not an error) while `remote_busy` is `Some`.

## Edge cases

| Case | Behaviour |
| --- | --- |
| fetch/pull/push with no network reachable | git's own timeout/DNS-failure message, `FetchFailed`/`PullFailed`/`PushFailed` verbatim |
| an SSH passphrase or 2FA prompt git would normally show interactively | ferrit's subprocess has no TTY to prompt on; git fails (or the underlying `ssh` does) rather than hanging forever, same known limitation `docs/PLAN_7_COMMIT.md` already notes for GPG — configuring `ssh-agent` / a credential helper avoids ever hitting this prompt, which is most real setups already |
| push rejected as non-fast-forward (someone else pushed first) | `PushFailed`, git's "Updates were rejected because the remote contains work that you do not have locally" verbatim; ferrit suggests nothing beyond showing it — `p` (pull) is one keypress away |
| pull with local uncommitted changes that would be overwritten | git refuses (same message class as phase 8's checkout case) -> `PullFailed`, worktree untouched |
| pull that starts a rebase and conflicts | same treatment as phase 8's conflicted merge: not resolved, not pretended-resolved; `Files` pane shows `Change::Conflicted`; message notes conflict resolution is phase 11. `git status` during a rebase is enough to retry `git rebase --continue`/`--abort` from the shell meanwhile. |
| `f`/`p`/`P` pressed while one is already running | ignored outright (`remote_busy.is_some()`), no queueing |
| a background fetch/pull/push finishes while a *local* `refresh()` (fs-watch, poll, `r`) also fires | both call `refresh()`; idempotent, same as phase 7's "commit finishes, fs-watch also fires" case |
| `App::mock()` (no repo) | `f`/`p`/`P` no-op immediately, no thread spawned — `repo_handle()` returns `None` before `thread::spawn` |
| zero remotes configured, `f`/`p` pressed | git's own "fatal: No remote repository specified" / similar, shown verbatim; `P` short-circuits earlier with `"no remote configured"` (see "No upstream" flow) since ferrit already knows the answer without asking git |
| quitting ferrit (`q`/`Ctrl-c`) while a fetch/pull/push is mid-flight | the spawned thread is detached (`thread::spawn`, not joined); the process exits and the child `git` either finishes writing (harmless, nothing left to read the result) or is killed with it — no different from `Ctrl-c`-ing `git fetch` running standalone in a shell |

## Self-testing (see `PLAN_SELF_TESTING.md`)

Fetch/pull/push need a *second* fixture repo to act as the remote — every
earlier phase's fixtures were self-contained. `tests/git_remote.rs` builds
two `TempDir` repos and points one at the other with a `file://` (or plain
path) remote URL, no network, no real GitHub involved — the same trick
`git`'s own test suite uses.

- `remotes()` on a repo with two `git remote add`-ed remotes lists both,
  fetch/push URLs correct.
- `fetch(None)` against a bare-init'd second repo with a new commit updates
  the remote-tracking ref (`git rev-parse refs/remotes/origin/main` moves)
  without touching the local branch or the worktree.
- `pull()` on a fast-forwardable case moves the local branch and the
  worktree; on a diverged case (local commit + remote commit, default
  `pull.rebase=false`) produces a merge commit, matching whatever
  `pull.rebase` the fixture's config says (test both `true` and default).
- `push(None)` with an upstream already set moves the remote's ref to
  match local `HEAD`.
- `push(Some("origin"))` with **no** upstream set sets one
  (`git rev-parse --abbrev-ref @{u}` afterward is `origin/<branch>`) and
  still pushes.
- `push` with no upstream and `push(None)` (the plain call) returns
  `GitError::NoUpstream` without running any subprocess that could fail a
  different way.
- a push rejected as non-fast-forward (push from repo A, then commit and
  push a *conflicting* commit from repo B against the same remote) returns
  `PushFailed` and the remote's ref is unchanged from B's failed attempt.
- **Threading**, in `tests/app_remote.rs` rather than `git_remote.rs`
  (needs `App`, not just `Repo`): `start_remote_op` on a real two-repo
  fixture, then poll `app.remote_busy_label()` until it clears (a tight
  loop with a short sleep is fine here — this is the one place ferrit's
  tests wait on a real background thread) and assert the eventual
  `AppEvent::RemoteDone` reached `App` and `refresh()` ran (ahead/behind
  changed). A second `start_remote_op` call while the first is still
  running is a no-op (`remote_busy` unchanged, no second thread observed
  via a counter in the test's fake — or, simpler, assert the *same*
  `RemoteOp` value is still there immediately after the second call).
- `tests/render.rs`: the Remotes tab renders `name`, `fetch:`/`push:` URLs;
  the busy label and a `status_note` each render on the Status pane and
  are mutually exclusive.
- `test/scripts/70-remote.script` + inline git golden (lands when ST1..ST3
  do, and once the harness can express "two repos, one fixture"):

  ```
  size 120x40
  fixture two-repos canonical origin
  key f
  wait-for "Fetched"
  git rev-parse refs/remotes/origin/main  -> "<origin's HEAD>"
  key p
  wait-for-not-busy
  git status --porcelain=v2  -> ""
  ```

  (`wait-for` / `wait-for-not-busy`: new replay-script primitives this
  phase needs that no earlier phase did, since every earlier action was
  synchronous within one `key` step. `PLAN_SELF_TESTING.md` gains this once
  the harness itself is built — noted here so it is not a surprise then.)

## Milestones

- ✅ **S0** `src/git/remote.rs`: `RemoteEntry`, `remotes()`,
  `GitError::FetchFailed`/`PullFailed`/`PushFailed`/`NoUpstream`. Still
  synchronous at this milestone (call it straight from `on_key`, no thread
  yet) so the git-level behaviour can be proven before the threading
  layer goes on top. `tests/git_remote.rs`'s two-repo fixture and the
  `remotes()`/`fetch`/`pull`/`push` cases (including `NoUpstream` and the
  non-fast-forward rejection) green.
- ✅ **S1** `events.rs` gains `AppEvent::RemoteDone`/`RemoteOp`/
  `Events::sender`. `App` gains `remote_busy`, `status_note`,
  `start_remote_op`, the `RemoteDone` arm in `run`'s match. `f`/`p`/`P`
  wired for the common case (upstream already set or not needed).
  `tests/app_remote.rs`'s threading tests green. Deviation from the
  plan's own pseudocode: `on_remote_done` runs `refresh()` *before*
  applying the op's `Ok`/`Err`, not after — seen up top would have let a
  routine post-op `refresh()` silently clear the very failure line
  `RemoteDone` exists to report.
- ✅ **S2** the no-upstream flow: `push_current_branch` checks
  `self.header.upstream` before ever calling `Repo::push`;
  `Popup::RemotePick` for 2+ remotes, the 0-remote and 1-remote
  short-circuits. `Repo::push`'s own `NoUpstream` detection stays as a
  defensive fallback for a direct call, not the primary path.
- ✅ **S3** Remotes tab rendering (`Snapshot`/`App` gain `remotes`,
  refreshed like every other pane's data), `Ctrl-Right`/`Ctrl-Left` tab
  switch on the Branches pane, `draw_remote_pick_popup`. Busy/status-note
  lines needed no `ui.rs` change: `status_lines()` already produces them.
- ✅ **S4** keybar + `HELP` + `mock::KEYBAR` updated — `Fetch/Pull/Push:
  f/p/P` restored as one combined segment (three separate ones did not
  fit the 120-column budget; `Scroll: J/K` / `Hunk: ]/[` trimmed to make
  room, both stay in `HELP`); `cargo clippy --all-targets -- -D warnings`
  and `cargo fmt --check` both clean on every file this phase touched;
  every edge case in the table has a test or an explicit inert path;
  `tests/render.rs` snapshots for the Remotes tab and the busy/note
  lines. Not done: `70-remote.script` — still blocked on the replay
  harness's `wait-for` primitive, exactly as this plan's own
  "Self-testing" section already expected.

## Definition of done (phase 9)

- `f` fetches every remote, `p` pulls honouring the user's `pull.*` config,
  `P` pushes — offering to set an upstream via a remote picker when the
  current branch has none — without ever freezing the keyboard while the
  network is slow.
- Only one fetch/pull/push runs at a time; a second attempt while one is in
  flight is a silent no-op, never a second subprocess.
- A failure (no network, rejected push, auth) shows git's own message; a
  success shows a short confirmation line; both clear on the next action.
- The Branches pane's Remotes tab lists every configured remote's fetch and
  push URLs.
- Ahead/behind (Status header, every Branches row) reflects reality
  immediately after a fetch or pull, with no extra keypress.
- `src/git/` still has no `ratatui` import (`cargo tree` check, unchanged
  since phase 2). Remote operations and repository snapshots run on
  background threads; snapshot results cross into `app.rs`, where refresh
  events coalesce while one read is in flight.
- `cargo clippy --all-targets` clean; `tests/git_remote.rs`,
  `tests/app_remote.rs` pass, including the two-repo fixture and the
  threading assertions.
- Quitting ferrit mid-fetch/pull/push does not hang or panic.

## After phase 9

Phase 10 is stash (push, pop, apply, drop) — the Stash pane has been
read-only since phase 2 (G6) the same way Branches was before this pair of
phases; the same "shell out, confirm before anything destructive" playbook
applies almost unchanged. Phase 11 is rebase, which is also where a pull
that started a rebase and conflicted (this phase's own "Edge cases" table)
finally gets a resolution UI instead of a "go to the shell" note.

Deferred out of phase 9, revisit with their own follow-up:

- `git remote add`/`remove`/`rename` — deliberately out of phase 9's scope
  entirely (see "Out, on purpose"), not just deferred within it.
- checkout from a remote-tracking ref with no local branch yet (needs this
  phase's remote list plus phase 8's checkout).
- force-push, with whatever guard rail (a distinct confirm wording,
  `--force-with-lease` always, a "this branch looks pushed and shared"
  heuristic) turns out to be worth it.
- fetch/push progress reporting beyond a static "Fetching…" label — would
  need parsing git's `--progress` stderr stream, a genuinely different
  (streaming) shape than every other subprocess call in `git::` today.
- a real "two repos" replay-fixture primitive and `wait-for` in
  `PLAN_SELF_TESTING.md`'s harness — needed by this phase's own golden
  script, not built by it.
