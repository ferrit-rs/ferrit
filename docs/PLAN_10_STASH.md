# Plan: phase 10, stash

**Deviation: `Stash: s` is not in the default keybar.** The bindings section
below planned to add it to `mock::KEYBAR`. That bar is 116 columns and
`Stash: s` makes it 127, over the 120 columns `tests/render.rs` holds it to.
`s` is documented in `HELP` (two lines, with the Stash pane keys); the
Stash pane gets its own `mock::STASH_KEYBAR`. To fit the help overlay at 40
rows, three existing `HELP` lines were merged (`j` / `k`, and the new-branch
popup `Enter` / `Esc`).

**Deviation: the conflict note does not list paths.** `App::files` is stale
until the background refresh lands, so naming conflicted paths right after
the command would show the previous state. The note says the stash was kept
and to resolve in Files.

## Goal

Make the Stash pane (`[5]`) do something. Since phase 2 (G6) it lists
`stash@{n}: message` rows and nothing else: no diff on the right, no way to
create, apply, pop or drop an entry. Phase 10 adds the four actions and a
real right-pane preview, so a user can park work, switch branches (phase 8),
and come back without leaving ferrit. lazygit's Stash panel does the same:
`<space>` apply, `g` pop, `d` drop, and `s` from the Files panel to stash.

This is the last phase `PLAN_0_GENERAL.md` counts toward "v1.0" (phases 1
through 10).

Out, on purpose (see "Out of scope" for where each lands):

- stash a single file, `--keep-index`, `--staged` (lazygit's stash menu)
- rename a stash, create a branch from a stash
- drill into a stash's files like the Commits pane does
- apply with `--index`

## The gap this fixes

`src/domain/git/stash.rs` is read only, its whole surface:

```rust
pub(super) fn stashes(repo: &mut Repository) -> GitResult<Vec<StashEntry>> { .. }
```

`src/app/input.rs` has no arm for the Stash pane, so `<space>` falls through
to `stage_selected_file`, which returns early because `focus != Pane::Files`
(`src/app/staging.rs:238`), and `d` reaches `discard_prompt`, which does
nothing off Files. The right pane shows `mock::RIGHT_STASH`, the literal
text `(no stash entries)`, even when entries exist (`src/app/screens/mod.rs:557`).

## Approach

Same playbook as phase 8: shell out to `git` for every write so hooks and
git's own safety messages apply, keep reads on `git2`, one subprocess per
action, synchronous (a stash push or apply is local, unlike phase 9's
network calls). New calls live in `src/domain/git/stash.rs` (`run_git` copied
in the phase 8 way: two modules map failure to different `GitError`
variants, so a shared helper would hide more than it saves).

References: the lazygit gestures above are from lazygit's published
keybinding docs. `../ferrit-references/` is not present in this checkout, so
no `path:line` is cited and the bindings were not re-verified against the
lazygit source. Re-check when the references are back.

### Commands

| Action | Command | Notes |
| --- | --- | --- |
| stash | `git stash push --include-untracked [-m <msg>]` | no `-m` when the message is empty, git writes `WIP on <branch>: ...` |
| apply | `git stash apply stash@{n}` | entry stays |
| pop | `git stash pop stash@{n}` | entry removed only on a clean apply |
| drop | `git stash drop stash@{n}` | asks first |
| diff | `git stash show -p --include-untracked <oid>` | see "Right pane" |

`--include-untracked` on push (staged, unstaged and untracked all go) matches
what ferrit's commit flow already treats as "everything changed"
(`stage all` includes untracked). `git stash show --include-untracked`
needs git 2.32 or newer; on an older git the untracked files are simply
missing from the preview, nothing else breaks.

### Resolve by oid, not by index

`StashEntry` carries `index` and a stable `oid` (`src/domain/git/model.rs`);
the app already keys selection on the oid (`SelectionKey::Stash`) because
indices shift when an entry is dropped, possibly from another shell. The
backend therefore takes the **oid** and resolves it to `stash@{n}` itself,
right before running the command:

```rust
fn resolve(repo: &mut Repository, oid: &str) -> GitResult<usize> {
    stashes(repo)?
        .into_iter()
        .find(|e| e.oid == oid)
        .map(|e| e.index)
        .ok_or_else(|| GitError::StashFailed("stash entry no longer exists".to_owned()))
}
```

so a background refresh between the keypress and the command cannot make
`d` drop the wrong entry. `git stash drop` refuses a bare oid
(it wants a stash reference), which is why resolving is not optional.
`git stash show` does accept a stash-like commit oid, so the diff needs no
resolve and cannot go stale.

### Outcomes

`apply` and `pop` can conflict. Same shape as phase 8's merge:

```rust
pub enum StashOutcome {
    /// Exit 0.
    Done,
    /// Exit non-zero and the index has conflicts: ordinary git behaviour,
    /// the stash is kept (pop does not drop on conflict), Files shows
    /// `Change::Conflicted`. Not a `GitError`.
    Conflicted,
}
```

Any other non-zero exit (a dirty worktree the apply would clobber:
"Your local changes to the following files would be overwritten") is
`GitError::StashFailed(stderr)`, verbatim. `stash push` with nothing to save
exits 0 and prints `No local changes to save`; the backend maps that stable
substring to `GitError::NothingToStash`, the same technique `NothingStaged`
uses.

## What it has to resolve

```
 Files pane (focus 2)                     Stash pane (focus 5)
+-----------------------+                +---------------------------+
| M  src/main.rs        |  s  popup      | stash@{0}: WIP on main    |
|  M README.md          | -------------> | stash@{1}: fix parser     |
| ?? notes.txt          |                +---------------------------+
+-----------------------+                 <space> apply  g pop  d drop
                                          right pane: `git stash show -p`
 Stash changes
+--------------------------------+
| message (optional)             |
| Stash: Enter | Cancel: Esc     |
+--------------------------------+
```

Decisions:

```
s (Files focused, Nav)
  |-- files empty?        -> report "nothing to stash", no popup
  '-- open Popup::Stash(TextInput)
        Enter -> repo.stash_push(msg)
                   |-- Ok            -> close popup, request_refresh, focus stays on Files
                   |-- NothingToStash-> close popup, report error
                   '-- other Err     -> keep popup open, report error (phase 8 new-branch rule)

<space> / g (Stash focused, Nav, entry selected)
  '-- repo.stash_apply|pop(oid)
        |-- Ok(Done)       -> request_refresh
        |-- Ok(Conflicted) -> request_refresh + Popup::Note("stash applied with conflicts. The stash was kept. ...")
        '-- Err            -> report_error

d (Stash focused, Nav, entry selected)
  '-- pending_confirm "drop stash@{n}: <message>?"
        y -> repo.stash_drop(oid) -> request_refresh
```

Pop needs no confirm: it removes the entry only after a clean apply, so
nothing is lost that a plain apply would not also keep. Drop is the
irreversible one (recoverable only through `git fsck --unreachable`), so it
asks, in the keybar prompt the discard and branch-delete already use.

## State on `App`

No new selection or cache state: `stashes`, `SelectionKey::Stash` and the
clamp after a shrinking snapshot already exist (`src/app/mod.rs:481`, `:1335`).

- `Popup::Stash(TextInput)`: message input, `Enter` submits (like
  `Popup::NewBranch`, unlike the commit popup). Rendered through the same
  `CommitPopupView` shape, title `"Stash changes"`, hints
  `"Stash: Enter | Cancel: Esc"`; `PopupView::Stash(CommitPopupView)` and a
  `PopupKind::Stash` arm in `src/app/popups.rs`.
- `ConfirmAction::DropStash { oid: String }` in `run_confirm`
  (`src/app/staging.rs:355`).
- `RightKey::Stash { oid }`, `DiffQueryResult::Stash(Diff)` in
  `src/app/diff_query.rs`, and `DiffView::Stash(StashEntry, Diff)`, built like
  `DiffView::Commit(entry, diff)`: the entry is looked up in `self.stashes`
  by oid, and a vanished entry yields no view, the same as a vanished commit.
  `right_is_diff()` must count it so `J` / `K` / `]` / `[` scroll it.
- New file `src/app/stash_actions.rs` (one responsibility per file, like
  `branch_actions.rs`): `open_stash_popup`, `do_stash_push`,
  `apply_selected_stash`, `pop_selected_stash`, `drop_stash_prompt`. Each
  guards `focus` and `popup.is_none()` first, like the phase 8 handlers.

### Backend surface

```rust
// src/domain/git/mod.rs, thin wrappers like the phase 8 ones
pub fn stash_push(&self, message: &str) -> GitResult<()>;
pub fn stash_apply(&mut self, oid: &str) -> GitResult<StashOutcome>;
pub fn stash_pop(&mut self, oid: &str) -> GitResult<StashOutcome>;
pub fn stash_drop(&mut self, oid: &str) -> GitResult<()>;
pub fn stash_diff(&self, oid: &str, opts: DiffOpts) -> GitResult<Diff>;
```

`stash_apply` / `pop` / `drop` take `&mut self` because `resolve` reads the
stash list, the one read that needs `&mut git2::Repository` (already noted
on `Repo::snapshot`). `stash_diff` goes through `DiffCmd` in `diff.rs`:
`DiffCmd::base("stash", opts).after_subcommand("show").arg("-p").arg("--include-untracked").arg(oid)`.
`after_subcommand` is a new one-line helper: `base` puts the diff flags
right after the subcommand, and `git stash --no-ext-diff show` fails with
`unknown option` (found by `tests/git_stash.rs`, the flags must come after
`show`). Checked on git 2.43: with that order `git stash show` accepts the
flags `DiffCmd::base` emits (`--no-ext-diff --color=never --unified=N
--find-renames=N% --submodule`), a bare oid works, and untracked files appear
in the patch. Also checked: `git stash drop <oid>` fails with `is not a stash
reference`, and `git stash push` on a clean tree exits 0 printing
`No local changes to save`.

`GitError` gains two variants, `StashFailed(String)` and `NothingToStash`.

### Bindings

| Pane | Key | Action |
| --- | --- | --- |
| Files | `s` | open stash popup |
| Stash | `<space>` | apply |
| Stash | `g` | pop |
| Stash | `d` | drop (asks first) |

`input.rs` gets the arms before the generic `<space>` / `d` ones, guarded by
`focus`, the phase 8 way (`KeyCode::Char('d') if self.focus == Pane::Stash`).
`s` is unclaimed today; `g` is unclaimed. `r` stays refresh (lazygit's stash
rename lives there, deferred). Keybar: a `mock::STASH_KEYBAR`
(`"Apply: <space> | Pop: g | Drop: d | Help: ? | Quit: q"`) swapped in by
`draw_keybar` for the Stash pane, exactly like `BRANCHES_KEYBAR`
(`src/app/screens/mod.rs:630`), and `mock::HELP` documents `s` and the three Stash keys (see the deviation note
up top for why `mock::KEYBAR` is unchanged).

### Right pane

The empty-stash text stays as the empty state. With an entry selected, the
right pane shows `git stash show -p` for its oid through the existing async
diff worker (`diff_query::load`), so it inherits scroll, the scrollbar,
delta rendering, hunk / file jumps and the refresh rule from `PLAN_0`
("rebuilt on a background `AppEvent::Refresh` without discarding scroll for
an unchanged selection", which the oid key gives for free).

## Edge cases

| Case | Behaviour |
| --- | --- |
| `s` with a clean tree | report `nothing to stash`, no popup; the popup path also maps `NothingToStash` if a refresh was stale |
| empty message | no `-m`, git's own `WIP on <branch>` message |
| apply onto a dirty tree that overlaps | `StashFailed`, git's message verbatim, stash untouched |
| apply / pop conflicts | `StashOutcome::Conflicted`, note popup, stash kept, Files shows conflicts |
| entry dropped from another shell mid-action | `resolve` fails, `stash entry no longer exists`, no wrong drop |
| drop the last entry | list empties, selection clamps to 0, empty-state text returns |
| stash in a detached HEAD / unborn branch | git decides, error surfaced verbatim |
| Stash pane empty, `<space>` / `g` / `d` | no-op |
| popup up, any Stash key | no-op (popup owns input) |
| `s` outside Files | no-op, the key stays free for later phases |

## Out of scope

- **Single-file stash, `--keep-index`, `--staged`.** lazygit's stash menu.
  Lands in phase 12 with the menu (`x`) that `PLAN_0` reserves for the long
  tail. Safe to defer: `s` covers the common case.
- **Rename a stash, branch from a stash.** No design question, but `r` is
  refresh here. Phase 12 (keymap work).
- **Drill into a stash's files.** The Commits pane has a file drill; a stash
  is one commit with a diff, so the plain diff preview is enough for now.
- **`apply --index`.** Git restores staged state only on request; ferrit
  never asks. Revisit if users lose their staging on pop.
- **Stash while a merge / rebase is in progress.** Phase 11.

## Self-testing (see `PLAN_SELF_TESTING.md`)

`tests/git_stash.rs` (backend, temp repo like `tests/git_branch.rs`):

- push with message: `stashes()` returns it, worktree clean, untracked file gone
- push with empty message: default `WIP on` message
- push on a clean tree: `NothingToStash`
- apply keeps the entry; pop removes it; content restored both times
- pop into a conflicting worktree: `Conflicted`, entry kept, `has_conflicts`
- apply onto an overlapping dirty file: `StashFailed`
- drop removes exactly the entry with that oid even after another entry was
  pushed on top (index shifted)
- unknown oid: `stash entry no longer exists`
- `stash_diff` contains the stashed hunk and the untracked file

`tests/app_stash.rs` (`App` seams, like `tests/app_branch.rs`):

- Files `s` opens the popup, typing plus `Enter` stashes, popup closed,
  Files empty after refresh
- `s` on a clean tree opens no popup and reports the message
- Stash `<space>` applies, `g` pops (entry gone), `d` asks then `y` drops,
  `n` / `Esc` keeps it
- `d` on Files still discards, `<space>` on Files still stages (prior phases)
- right pane shows the stash diff for the selected entry and is empty-state
  when none
- conflict on pop shows the note popup
- keys are no-ops with the popup up and on an empty Stash pane

`tests/render.rs`: snapshot of the stash popup and of `STASH_KEYBAR`.
`test/scripts/80-stash.script` (push with a message, apply, drop asks, pop,
drop) runs in `tests/replay.rs`.

## Milestones

- ✅ **S0** `stash.rs` backend: `stash_push`, `apply`, `pop`, `drop`, `resolve`,
  `StashOutcome`, `GitError::{StashFailed, NothingToStash}`, `stash_diff`
  (`DiffCmd::after_subcommand`). `tests/git_stash.rs` green (10 cases),
  existing tests untouched.
- ✅ **S1** right pane: `RightKey::Stash`, `DiffView::Stash`, worker wiring,
  scroll and refresh behaviour. Empty state kept.
- ✅ **S2** Stash pane actions: `stash_actions.rs`, `<space>` / `g` / `d` arms,
  `ConfirmAction::DropStash`, conflict note.
- ✅ **S3** Files `s` popup: `Popup::Stash`, `PopupView::Stash`, submit and
  failure rules. `tests/app_stash.rs` green (12 cases).
- ✅ **S4** `mock::STASH_KEYBAR`, `HELP`, `tests/render.rs` cases
  (`keybar_swaps_for_the_stash_pane`, `stash_popup_renders_title_and_hints`),
  `CHANGELOG.md` line (this also adds the `## [Unreleased]` heading, which
  0.5.0 had consumed), `PLAN_0` status flipped to done.
- ✅ **S5** polish: `cargo fmt --check` clean, `cargo test` green,
  `src/domain/git/` has no `ratatui` import, and this phase adds no clippy
  diagnostic. The repo-wide CI gates were red on `main` before this phase
  (`cargo clippy --all-targets --all-features -- -D warnings`: 5 errors and
  78 warnings, and the rustdoc `-D warnings` gate: 2 broken links, in
  `.github/workflows/ci.yml` terms) and were cleaned afterwards without any
  `#[allow]` or lint-config change (commits `b1a6951`, `c91b8b3`, `b614d65`,
  `69ff912`).
  Every row of the edge case table has a test or an explicit inert path:
  - detached HEAD: `stash_round_trips_on_a_detached_head` (git allows it);
  - unborn branch: `push_on_an_unborn_branch_fails_with_gits_own_message`
    (git exits 1, `You do not have the initial commit yet`, worktree
    untouched);
  - a stash dropped from another shell while the confirm is up:
    `drop_after_the_stack_shifted_still_drops_the_selected_entry` and
    `drop_of_an_entry_already_gone_reports_and_spares_the_others` in
    `tests/app_stash.rs`. The first fails if `resolve` stops going through
    the oid (checked by mutating it to a fixed `stash@{1}`).

## Definition of done (phase 10)

- `s` on Files stashes everything, untracked included, with an optional
  message; a clean tree says so and opens nothing.
- On Stash, `<space>` applies, `g` pops, `d` drops after a confirm.
- The right pane shows the selected entry's diff and scrolls like any diff.
- A conflicting apply or pop keeps the stash and shows the conflicted files.
- Actions resolve the entry by oid: a concurrent stash change never drops or
  applies the wrong entry.
- A failed git command shows git's own message and changes nothing.
- Every "does nothing" case in the edge table is inert, not an error.
- `src/domain/git/` still has no `ratatui` import.
- `cargo clippy --all-targets` clean; `tests/git_stash.rs`,
  `tests/app_stash.rs`, `tests/render.rs` and all earlier test files pass.

## After phase 10

Phase 11 is rebase. It reuses this phase's `StashOutcome`-style "conflict is
an outcome, not an error" split, and is where a stash pop that conflicted
during an in-progress rebase finally gets a resolution UI. Phase 12 collects
the stash menu (single file, keep-index, rename, branch from stash).
