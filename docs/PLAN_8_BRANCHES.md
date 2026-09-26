# Plan: phase 8, branches

**Deviation: a real merge conflict exits non-zero, not 0.** The "Backend"
section below assumed `git merge`'s two success shapes (a clean merge and a
left-conflicted one) both exit 0, distinguished only by whether `MERGE_HEAD`
got written. Checked empirically at implementation time: a conflicting
`git merge` exits 1 like any other failure. `merge_branch` tells the two
apart with `repo.state() == RepositoryState::Merge` on the non-zero path —
`git2` read, not a subprocess, same split the rest of this module makes —
rather than trusting the exit code at all. `MergeOutcome`'s two variants and
everything ferrit does with them are otherwise exactly as planned.

**Deviation: the confirm accessor is `App::confirm_message`, not
`discard_prompt_message`.** The plan's "App wiring" section renames the
*state* (`pending_discard` -> `pending_confirm`, `DiscardPrompt` ->
`ConfirmPrompt`) but says nothing about the public accessor
`ui::draw_keybar` and the tests read it through. Since it now speaks for a
branch-delete confirm as much as a discard one, it is renamed too; `d` and
`n`/`y`/`Esc` behave identically either way.

## Goal

Turn the read-only Local Branches list from phase 2 (G7: list, live
branch -> log preview, Enter to drill into a branch's own commit log) into
something you act on: checkout, create, delete, fast-forward, and merge.
`HEAD` moves for the first time in ferrit's history; this phase and phase 9
(remote) are the two that make that true.

Scope is exactly `PLAN_0_GENERAL.md`'s phase 8 line: "checkout, create,
delete, fast-forward, merge". Out, on purpose:

- **Rebase.** Its own phase (`PLAN_11_REBASE.md`); interactive rebase is a
  different UI (a todo-list editor) and a different risk profile.
- **Rename a branch.** One `git branch -m`, no design questions, but no line
  in the phase 0 scope either; a trivial follow-up once this phase's
  keybindings settle.
- **Checkout / branch-from a *remote-tracking* ref** (`origin/feat-x`).
  Needs phase 9's remote-tracking knowledge (which remotes exist, which
  ref belongs to which); the Branches pane's "Remotes" tab stays an inert
  label until then, same as it has since phase 1.
- **Tags.** Same story as Remotes: the tab exists as a label
  (`Pane::title`: `"[3] Local branches - Remotes - Tags"`), nothing behind
  it yet. Not claimed by any numbered phase; revisit in `PLAN_12_POLISH.md`
  or its own follow-up if it turns out to matter before then.
- **Conflict resolution.** A merge *can* conflict (see "Edge cases"); ferrit
  shows that it happened and gets out of the way. Resolving it in the UI is
  phase 11 territory, same boundary conflicts hit there.
- **Worktrees.** `PLAN_0_GENERAL.md`'s "Not scheduled" list (see `gwm` in
  `INSPIRATION.md`).

## Approach: shell out, same reasons as phases 6 and 7

`checkout`/`branch -d`/`merge` all have a `git2` equivalent, but ferrit
already chose subprocess `git` over library calls for exactly this class of
operation (`docs/PLAN_6_STAGING.md`, `docs/PLAN_7_COMMIT.md`), and every
reason still applies:

- **Hooks run.** `post-checkout`, `pre-merge-commit`,
  `post-merge`. `git2::Repository::set_head` + `checkout_tree` run none of
  them.
- **Safety messaging is git's own.** "Your local changes to the following
  files would be overwritten by checkout" (dirty worktree),
  "error: branch 'x' is not fully merged" (unsafe delete), a merge conflict
  summary — all worded and reasoned about by git itself. Reproducing that
  logic over `git2` (dirty-file detection, merge-base computation, conflict
  markers) is real, error-prone work for strictly worse messages.
- **One code path, not two.** `apply.rs`/`commit.rs` already run
  `git -C <workdir> ...` and read `stderr`/exit code; `branch.rs` reuses the
  identical shape.

`git checkout <name>` over the newer `git switch <name>`: matches what
lazygit itself runs (`pkg/commands/git_commands/branch.go`), and works on
every git version ferrit might meet, not just the ones with `switch`.

## The five actions

```
        Branches pane                              current branch: main
   ┌ [3] Local branches ────┐
   │ 2d  * main         ↑1  │  <- checked out (HEAD), can't delete or
   │ 5h    feat/staging     │     check itself out again
   │ 3d  > feat/commit  ↓2  │  <- selected (> cursor)
   │                        │
   └────────────────2 of 3 ─┘

   space  -> checkout the SELECTED branch          (git checkout feat/commit)
   n      -> new branch from the CURRENT HEAD       (git checkout -b <name>)
   d      -> delete the SELECTED branch, confirmed  (git branch -d / -D)
   u      -> fast-forward the SELECTED branch       (see "u: fast-forward
             to its upstream, checked out or not        any branch" below)
   M      -> merge the SELECTED branch into current (git merge <name>)
```

`u` rather than a global-looking `f`: `f` is reserved for phase 9's
*fetch* (talks to the network, works from any pane), and fast-forward here
is a strictly local, Branches-pane operation — sharing a letter between "go
talk to the remote" and "move a local ref from data already on disk" would
read as the same weight of action when they are not.

`space` is ferrit's one already-established "primary action" key, meaning
something different per pane/mode: stage a file (Files, `Mode::Nav`), stage
a hunk/selection (`Mode::Diff`), and now checkout a branch (Branches). Enter
stays the drill-down into a branch's own log from phase 2 (G7) — checkout
and "look at this branch's history" are different intents and keep
different keys, same as `d` already means "discard a worktree change" in
Files/`Mode::Diff` and, from this phase, "delete a branch" in Branches: the
letter carries the same *shape* of action (destroy something, ask first),
not the same target.

## Backend: `src/domain/git/branch.rs`

New module under `src/domain/git/`, sibling of `apply.rs` / `commit.rs`. No
`ratatui`. Reuses the `git -C <workdir> ...` spawn pattern.

```rust
//! Checkout, create, delete, fast-forward and merge branches by shelling
//! out to `git`, so hooks and git's own safety messaging apply. See
//! docs/PLAN_8_BRANCHES.md.

impl Repo {
    /// `git checkout <name>`. Fails (message verbatim) on a dirty worktree
    /// that checkout would clobber; ferrit does not stash-and-pop on the
    /// user's behalf.
    pub fn checkout(&self, name: &str) -> GitResult<()>;

    /// `git checkout -b <name>`, always from the current `HEAD` (branching
    /// from an arbitrary commit is a deferred nicety, see "After phase 8").
    pub fn create_branch(&self, name: &str) -> GitResult<()>;

    /// `git branch -d <name>` (or `-D` when `force`). Refuses the currently
    /// checked-out branch the same way `git` does; that error surfaces
    /// verbatim rather than being pre-checked here (`git` is the one source
    /// of truth for "is this actually HEAD").
    pub fn delete_branch(&self, name: &str, force: bool) -> GitResult<()>;

    /// Fast-forward `name` to its upstream. Two mechanisms depending on
    /// whether `name` is checked out, picked automatically:
    /// - checked out: `git merge --ff-only @{u}` (git refuses to let
    ///   anything else write to `HEAD`'s branch).
    /// - not checked out: `git fetch . <upstream-shorthand>:refs/heads/<name>`
    ///   — a *local* fetch (source `.`, this same repository) that moves
    ///   `name`'s ref to match its already-known upstream ref, entirely
    ///   from data already on disk. No network, no auth: this is lazygit's
    ///   own "fast-forward a branch you're not on" trick
    ///   (`pkg/commands/git_commands/sync.go`), and it is what makes
    ///   fast-forward a *branches* feature rather than something that has
    ///   to wait for phase 9's real network fetch. A plain (non-`+`-forced)
    ///   `git fetch` ref update already refuses a non-fast-forward, so no
    ///   extra safety check is needed on ferrit's side.
    pub fn fast_forward(&self, name: &str) -> GitResult<()>;

    /// `git merge <name>` into the current branch. A real merge, not
    /// `--ff-only`: may create a merge commit, may conflict (see "Edge
    /// cases"). No `--no-edit`; ferrit passes a merge message the same way
    /// `commit.rs` passes a commit message, `-m <default git message>`, so
    /// no `core.editor` spawn.
    pub fn merge_branch(&self, name: &str) -> GitResult<MergeOutcome>;
}
```

`MergeOutcome` (not a plain `()`, unlike the others) because "it worked" has
two shapes ferrit's UI treats differently:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeOutcome {
    /// Exit 0: a merge commit (or a fast-forward git decided to do anyway)
    /// landed clean.
    Merged,
    /// Exit 0, but git left `MERGE_HEAD` and conflict markers instead of
    /// committing. Distinguished from a hard failure (below) because it is
    /// not an error to surface as `MergeFailed` — it is expected, ordinary
    /// git behaviour that ferrit currently has no UI for finishing.
    Conflicted,
}
```

`GitError` gains, matching phases 6/7's shape:

```rust
#[error("git checkout failed: {0}")]
CheckoutFailed(String),
#[error("git branch failed: {0}")]
BranchFailed(String),   // create / delete / fast-forward
#[error("git merge failed: {0}")]
MergeFailed(String),    // a real failure, distinct from MergeOutcome::Conflicted
```

No dedicated "branch already exists" / "not fully merged" variants: same
reasoning as phase 7 dropping `HookRejected` — git's message is already the
right thing to show verbatim, and the one case ferrit *does* act on
specially (delete refused because it is unmerged) is detected by matching
that one stable substring, same technique `commit.rs` already uses for
`NothingStaged`.

## App wiring

### Generalizing phase 6's discard confirm into `Popup::Confirm`

Phase 7 wrote its own `Popup` sketch (`docs/PLAN_7_COMMIT.md`) with a
`Confirm { .. }` variant commented "reused from phase 6's discard confirm"
— foresight that turned out right, but nothing forced the issue yet because
`c` opened a very different kind of popup (a `TextInput`). Deleting a
branch is the first *other* thing that needs a yes/no gate, so this phase
is where `pending_discard: Option<DiscardPrompt>` actually generalizes:

```rust
/// A pending confirmation. Replaces phase 6's `DiscardPrompt`, which becomes
/// one `ConfirmAction` variant among others rather than the only shape.
struct ConfirmPrompt {
    message: String,
    action: ConfirmAction,
}

enum ConfirmAction {
    DiscardFile(PathBuf),           // phase 6, unchanged behaviour
    DiscardGranule(Granule),        // phase 6, unchanged behaviour
    DeleteBranch { name: String, force: bool },
}
```

`self.pending_discard: Option<DiscardPrompt>` is renamed
`self.pending_confirm: Option<ConfirmPrompt>` and the `y`/`n`/`Esc` handling
in `on_key` (already generic — it only calls `self.confirm_discard()`)
grows one more match arm inside that confirm function. Nothing about the
keybar rendering changes: `theme::confirm_line` already takes a plain
message string, not a discard-specific one.

### The two-step "unmerged" delete

`git branch -d` refusing an unmerged branch is not a failure to report and
stop; it is git offering a choice. Detected the same way `NothingStaged`
already is (a stable substring of stderr, `"is not fully merged"`):

```
d on a branch
  -> confirm "delete branch <name>?"
  -> y: delete_branch(name, force: false)
       Ok            -> refresh(), done
       Err(unmerged) -> confirm "'<name>' is not fully merged. Force
                         delete? This may lose commits with no other
                         reference to them." (force: true this time)
                          -> y: delete_branch(name, force: true)
       Err(other)    -> Popup::Note(message), no branch touched
```

Deleting the checked-out branch is not offered a confirm at all: `git`
itself refuses ("error: Cannot delete branch 'main' checked out at ...")
and that message goes straight to `last_error`, the same "explain, do
nothing" path an invalid discard already takes.

### `n`: a new-branch popup, reusing phase 7's `TextInput`

One line of input, not a message body. Rather than a second hand-rolled
widget, `TextInput` (phase 7, `docs/PLAN_7_COMMIT.md`'s deviation note —
still no compatible `tui-textarea`, and now proven useful for more than
commit messages) is reused as-is, with `Enter` reinterpreted:

```rust
enum Popup {
    Commit(CommitDraft),
    /// New-branch name input. Enter *submits* here, unlike the commit
    /// popup, where Enter inserts a newline — the only behavioural
    /// difference from reusing `TextInput` outright.
    NewBranch(TextInput),
    Note(String),
}
```

`n` (Nav, Branches focused) opens `Popup::NewBranch(TextInput::default())`.
`popup_key` grows a `Popup::NewBranch` arm: printable keys and
backspace/arrows go to the buffer exactly like the commit popup; `Enter`
calls `create_branch(&buf.text())` instead of `insert_newline()`; `Esc`
cancels with **no** draft persistence (unlike a commit message, a
half-typed branch name is cheap to retype and there is no natural place to
resurface it — the commit draft's whole reason to exist is that losing a
paragraph hurts, a few characters do not).

### Checkout, fast-forward, merge: no popup, no confirm

All three run straight from the keypress and end the same way:

```
space / f / M
  -> Repo::checkout | fast_forward | merge_branch
  -> refresh()                          (phase 2: HEAD, branches, commits,
                                          files — a checkout changes the
                                          working tree, so Files changes too)
  -> Ok(MergeOutcome::Conflicted) or
     Err(_)                -> last_error / Popup::Note (see next section)
  -> Ok(()) / Ok(Merged)   -> nothing further; the panes already show it
```

No confirm: checkout and fast-forward are exactly as reversible as any
other git command (the reflog has your back, same trust the rest of git
extends), and merge either succeeds cleanly or lands in the well-known
"conflicted, go resolve it" state git itself would leave you in from the
command line — ferrit is not making a worse decision than typing the
command yourself would.

## Rendering (`src/app/screens/mod.rs`)

- No new right-pane view: the phase 2 branch-list / branch-log-preview
  split is untouched.
- `Popup::NewBranch` reuses `draw_commit_popup`'s shape at a smaller fixed
  height (one input line + footer), title `" New branch "`. Given the
  title and the popup enum arm are the only differences, this is a
  parameter on the existing function, not a second one — `draw_commit_popup`
  already takes everything else (`CommitPopupView`-shaped) as data, not as
  hard-coded strings.
- The confirm keybar line (`theme::confirm_line`, unchanged since phase 6)
  now also carries branch-delete messages; no rendering change needed, it
  already takes an arbitrary string.
- Keybar (`mock::KEYBAR`) gains `Checkout: space | New: n | Delete: d` (only
  shown/relevant with Branches focused — see the "context-sensitive keybar"
  note already true of `Stage: <space>` today). `HELP` documents `u` and
  `M` too; keybar segments stay reserved for what fits under 120 columns,
  same trade `docs/PLAN_6_STAGING.md`'s keybar update already made.

## Keybindings (new in phase 8)

| Key | Context | Action |
| --- | --- | --- |
| `<space>` | Nav, Branches focused | checkout the selected branch |
| `n` | Nav, Branches focused | open the new-branch popup (from current `HEAD`) |
| `d` | Nav, Branches focused | delete the selected branch (confirmed; two-step if unmerged) |
| `u` | Nav, Branches focused | fast-forward the selected branch to its upstream (checked out or not) |
| `M` | Nav, Branches focused | merge the selected branch into the current one |
| `Enter` | popup (`NewBranch`) | create the branch, close the popup |
| `y` / `n` / `Esc` | confirm prompt | as phase 6 (now shared by discard and branch delete) |

`Enter` (drill into a branch's log) and the branch-log-preview-on-select
behaviour from phase 2 are unchanged.

The new-branch prompt is titled `New branch name (branch is off of '<current>')`, naming the
branch it starts from, as lazygit's does. The list is the checked-out branch first, then the
most recently committed to (tip time, newest first), as lazygit orders it; before, alphabetical.

After `n` creates a branch, the selection moves to it (it is the checked-out
one, first in the list), not the row the cursor was on. Found by running the
same flow in lazygit and ferrit (`test/flows/feature-workflow.flow`). The row is
remembered by name (`App::select_when_listed`) and selected once a refresh lists
it, so a refresh already in flight when the branch was made does not lose it.

## Edge cases

| Case | Behaviour |
| --- | --- |
| checkout with a dirty worktree that would be overwritten | `CheckoutFailed`, git's own message verbatim, worktree untouched |
| checkout the already-checked-out branch | a no-op `git checkout` exit 0; ferrit does not special-case it |
| delete the checked-out branch | git refuses; message goes to `last_error`, no confirm offered |
| delete an unmerged branch | two-step confirm (see above), `-D` only on the second `y` |
| new branch name that already exists, or is invalid (`git check-ref-format`) | `BranchFailed`, git's message verbatim, popup stays open with the typed text so the user can fix it and retry (same "keep the draft" instinct as the commit popup, just without persisting past `Esc`) |
| fast-forward with no upstream, or the branch is not a strict ancestor of it | `BranchFailed` ("no tracking information" / a rejected non-fast-forward ref update), shown as a `Note` |
| fast-forward the checked-out branch specifically | routed to `git merge --ff-only @{u}` instead of the `git fetch .` trick (git refuses to update `HEAD`'s own ref via fetch); same `fast_forward(name)` call either way, the mechanism choice is internal |
| merge with nothing to merge (branches identical / selected already an ancestor) | git says "Already up to date."; treated as `Ok(Merged)`, no visible change |
| merge conflicts | `Ok(MergeOutcome::Conflicted)` -> `Popup::Note("merge conflict in <files>. Resolve and commit, or `git merge --abort` from the shell — conflict resolution UI is phase 11.")`. `refresh()` still runs: the Files pane already renders `Change::Conflicted` (phase 2's `status.rs`), so the conflicted paths are visible even without a dedicated flow. |
| merge a branch into itself, or the checked-out branch into itself | git's own "Already up to date." / a no-op; not special-cased |
| a checkout or merge started from ferrit finishes while a background `refresh()` fires mid-operation | the subprocess already completed before `refresh()` is called (synchronous, phase 6/7's pattern); no interleaving is possible |

## Self-testing (see `PLAN_SELF_TESTING.md`)

Same shape as phases 6 and 7: throwaway `git2` fixture repos, `git` run
against them; the replay harness (`ferrit::replay`, `--replay`) runs
`test/scripts/60-branches.script` and `65-merge.script`.

- `tests/git_branch.rs`: fixture repo with two branches, then via `Repo`:
  - `checkout` switches `HEAD` (`git symbolic-ref HEAD`), and a dirty
    conflicting worktree file makes it fail without changing anything.
  - `create_branch` creates and checks out from the current `HEAD`; the new
    branch's tip equals the parent's.
  - `delete_branch(force: false)` on a merged branch succeeds; on an
    unmerged one returns an error whose message contains "not fully
    merged"; `delete_branch(force: true)` on that same branch then
    succeeds.
  - `delete_branch` on the checked-out branch is an error, and
    `git branch --list` still shows it afterward.
  - `fast_forward` moves a branch's tip to match its upstream commit when
    it is a strict ancestor, both for the checked-out branch and for one
    that is not (asserting the *other* one's `HEAD` never moved); errors
    when the branches have diverged.
  - `merge_branch` returns `Merged` for a clean fast-forwardable merge and
    for one that needs a real merge commit (`git log --merges` shows one);
    returns `Conflicted` for two branches with a real conflicting edit to
    the same line, and `git status --porcelain=v2` shows the conflict (`u`
    entries) afterward.
- `tests/app_branch.rs` (extends phase 6/7's fixture pattern): open a
  fixture with two branches, `<space>` checks out the selected one and
  `app.status_lines()` reflects the new branch name; `n`, type a name,
  `Enter` creates and switches; `d` on the current branch shows
  `last_error` and does not open a confirm; `d` on an unmerged branch shows
  the confirm, `y` shows the *second* confirm, `y` again deletes.
- `tests/render.rs`: one `TestBackend` snapshot of the new-branch popup and
  one of the two-step delete confirm text in the keybar.
- `test/scripts/60-branches.script` + inline git golden (lands when
  ST1..ST3 do):

  ```
  size 120x40
  fixture canonical
  key 3                       # focus Branches
  key n
  type "feat/replay"
  key enter
  git rev-parse --abbrev-ref HEAD  -> "feat/replay"
  key j                       # back to main (or wherever it sorts)
  key space
  git rev-parse --abbrev-ref HEAD  -> "main"
  ```

## Milestones

- ✅ **S0** `src/domain/git/branch.rs`: `checkout`, `create_branch`,
  `delete_branch`, `GitError::CheckoutFailed`/`BranchFailed`.
  `tests/git_branch.rs` checkout + create + delete (merged and unmerged)
  cases green.
- ✅ **S1** `fast_forward`, `merge_branch`, `MergeOutcome`,
  `GitError::MergeFailed`. Merge/conflict cases in `tests/git_branch.rs`
  green (see the exit-code deviation note up top for how `Conflicted` is
  actually detected).
- ✅ **S2** `pending_discard` generalized to `pending_confirm` /
  `ConfirmAction`; `<space>` checkout, `u` fast-forward, `M` merge wired,
  each `refresh()`ing after. `Popup::Note` shows a conflict / failure.
- ✅ **S3** `d` delete with the two-step confirm flow. `n` +
  `Popup::NewBranch` reusing `TextInput`, `Enter` submits.
  `tests/app_branch.rs` green.
- ✅ **S4** keybar + `HELP` updated (both are generated from the keymap since
  `PLAN_12_POLISH.md` P3, so the `mock::KEYBAR` constants this milestone named
  are gone); `cargo clippy --all-targets -- -D warnings` and `cargo fmt
  --check` both clean; every edge case in the table has a test or an explicit
  inert path; `tests/render.rs` snapshots for the new-branch popup and the
  delete confirm; `test/scripts/60-branches.script` (checkout, create, delete,
  the checked-out branch) and `65-merge.script` run in `tests/replay.rs`.

## Definition of done (phase 8)

- `<space>` checks out the selected branch; a dirty worktree that would be
  overwritten blocks it with git's own message, unchanged.
- `n` creates and checks out a new branch from `HEAD`, named from a popup.
- `d` deletes the selected branch after a confirm; an unmerged branch gets
  a second, explicit force-delete confirm instead of silently losing work.
- `u` fast-forwards the selected branch to its upstream when possible,
  checked out or not; `M` merges the selected branch into the current one,
  landing a merge commit, a fast-forward, or a conflicted state git itself
  would also leave.
- A merge conflict is visible (Files pane, `Note` popup) and does not
  pretend to be resolved; ferrit does not attempt to resolve it.
- `src/domain/git/` still has no `ratatui` import (`cargo tree` check, unchanged
  since phase 2).
- `cargo clippy --all-targets` clean; `tests/git_branch.rs`,
  `tests/app_branch.rs` pass.
- Deleting the checked-out branch, an already-checked-out `<space>`, and an
  invalid new-branch name each surface a clear message without a panic.

## After phase 8

Phase 9 is remote: fetch, pull, push, and the Branches pane's "Remotes" tab
(reading which remotes exist so checkout-from-a-remote-tracking-ref becomes
possible). Phase 11 is rebase, including finishing a merge this phase left
conflicted the "proper" way (`git rebase --continue`'s sibling story for
merges is just `git commit`, already phase 7 — so a conflicted merge is
already *finishable* today, just not resolvable, inside ferrit).

Deferred out of phase 8, revisit with their own follow-up:

- rename a branch (`git branch -m`) — no design questions, just an
  unclaimed keybinding to pick.
- creating a branch from an arbitrary commit (a Commits-pane row) instead
  of always the current `HEAD`.
- a merge-strategy / `--no-ff` toggle; phase 8 always lets git pick.
