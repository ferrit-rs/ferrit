# Plan: phase 11, rebase and conflict flow

**Status: done (R0 to R6).** Scripts: `test/scripts/90-rewrite`, `91-operation`,
`92-fixup`, `93-skip`, `95-conflict` and `96-detached`. The prompt after rewriting pushed commits now
says `diverged (ahead N, behind M)`: the phase 9 guard already fired (it keys
on `behind > 0`), but its text said only "behind upstream", which is false
for a branch that is also ahead.

**Deviation (R5): `a` checks before it rewrites, and the Commits keybar drops
`f/p/P`.** `git rebase -i --autosquash` over a range with no foldable commit
succeeds and changes nothing, which would look like a broken key. So `a` first
looks for a `fixup! <subject>` / `squash! <subject>` among the rows from the
selected one up whose target subject is a row further down, still inside the
range, and otherwise says `no fixup! or squash! commit above this one to
fold`. Only `fixup!` is created (`F`); a `squash!` commit with its own message
(`CommitKind::Squash`) stays without a key, since `s` on an existing commit
covers the common case. The Commits keybar with `New fixup!: F` and
`Autosquash: a` is 105 columns; adding the fetch / pull / push segment would
make it 129, over the 120 the render tests hold, so those keys (still bound
everywhere) are listed in the default bar and in `HELP` only.

**Deviation (R4): the reword popup for an older commit hides sign-off and
no-verify.** Found by a render test: the commit editor always drew
`sign-off: off   no-verify: off` and its `Ctrl-O/N` hint, but a rebase reword
runs `git commit --amend -F` itself and neither applies, so the line was
false and the keys toggled nothing visible. For a reword target the footer is
the hints line alone, the hints say `Enter: reword`, `Ctrl-O/N` are ignored,
and cancelling does not save the text as the next `c`'s draft (it would have
come back as a new commit's message). The refusal rule follows the new-branch
popup: a refused reword (dirty worktree) keeps the popup and the typed text
for a retry, a stop or success closes it. `s` / `S` / `e` / `d` act at once
except `d`, which asks; squashing the oldest commit is an error message (git
has nothing below it), not an inert key, so the user learns why. While an
operation is stopped the keys answer `finish or abort the operation in
progress first (m)` in the Status line instead of doing nothing silently.

**Deviation (R3): a reword is `pick` plus `exec git commit --amend -F`, not a
`reword` line with `GIT_EDITOR=cp`.** The table below planned the editor
route. It loses the message when the rebase stops on a conflict earlier in the
range: the later `--continue` runs with `GIT_EDITOR=true`, so the reword step
keeps the old message silently. An `exec` line sits in git's own copy of the
todo and still runs after any number of stops (checked: drop, conflict,
resolve, continue, and the amend still fired). The message file therefore
outlives the run and is removed only once no rebase is in progress.
`GIT_EDITOR=true` for everything else, `GIT_SEQUENCE_EDITOR="cp '<todo>'"`
for the todo, both quoted for paths with spaces or quotes (tested in a
directory called `sp ace's`). Also: the "operation already in progress" guard
runs before anything else, because mid-rebase `HEAD` is a half-rewritten
history and the other checks would answer with a misleading message (found by
a test). A commit-msg hook rejecting the amend leaves the rebase stopped and
returns the hook's output as `RebaseFailed`; the badge and `m` are the way out.

**Deviation (R2): all four operations, one outcome type.** The milestone said
"a merge in progress". The menu is generic and the four command lines are one
each, so rebase, cherry-pick and revert ship with it, and their `--continue`,
`--skip` and `--abort` are tested here (R1 had promised the cherry-pick and
revert rows would be tested; they are, in `tests/git_rebase.rs`). The outcome
type is `OperationOutcome { Done, Stopped { conflicted } }` in
`domain/git/operation.rs`, not `RebaseOutcome` in `rebase.rs`, because a merge
continue is not a rebase; R3 reuses it. `Repo::operation_step(step)` takes no
operation argument: it reads the current state itself, so a stale value cannot
pick the wrong command. Checked: `git merge --skip` does not exist ("unknown
option"), and a `--continue` over an unresolved file prints no `CONFLICT (`
line while one that reaches a new conflict does, which is how the two are told
apart. While an operation is stopped, the `Continue / skip / abort: m` keybar
replaces the default one on every pane, Branches and Stash included, since
most of their keys are refused mid-operation anyway.

**Deviation (R1): the keybar hint ships with the menu, in R2.** R1 was
planned to add a `Menu: m` hint. `m` does nothing until R2 builds the menu,
so the hint would point at a dead key. R1 ships the state read and the
Status badge only.

**Checked while implementing R1.** In a stopped interactive rebase
`rebase-merge/msgnum` is the current step and `end` the total, and a `drop`
counts as a step: dropping the oldest of three commits and conflicting on the
next one gives `2/3`. The `--apply` backend uses `rebase-apply/next` and
`last`. Bisect leaves `BISECT_LOG` and maps to no operation; a stopped
`git am` (`ApplyMailbox`) also maps to none, since libgit2 cannot tell
`ApplyMailboxOrRebase` from a rebase.

## Goal

Let a user rewrite recent history and get out of a stopped operation without
leaving ferrit. Two halves that share one state machine:

1. **Rewrite**, from the Commits pane: reword any commit, drop one, squash or
   fixup one into the commit below it, stop at one to edit it, create a
   `fixup!` commit against one and fold every pending `fixup!` with an
   autosquash. Each is one `git rebase -i` run with a todo ferrit generates.
2. **Recover**, from anywhere: when git stops (a conflict, or an `edit`
   stop), ferrit shows that an operation is in progress, lets the user mark a
   conflicted file resolved, and offers continue, skip and abort in one menu.
   The same menu closes the loops earlier phases left open: the conflicted
   merge from phase 8, and the pull that started a rebase and conflicted in
   phase 9.

lazygit does both (its Commits panel keys and its "merge / rebase options"
menu on `m`); gitu covers autosquash. Neither reference checkout is present
here (`../ferrit-references/` is absent), so no `path:line` is cited and the
bindings below were not re-verified against their source.

Out, on purpose (see "Out of scope"): moving commits, an in-TUI conflict
editor, `rebase --onto`, rebasing onto a branch, cherry-pick, and any
merge-commit handling.

## The gap this fixes

- `src/app/branch_actions.rs` (`merge_selected_branch`) ends a conflicted
  merge with a note saying "conflict resolution UI is phase 11". Nothing in
  ferrit can continue or abort it.
- `src/domain/git/commit.rs` already has `CommitKind::Fixup { target }` and
  `CommitKind::Squash { target }`, and `docs/PLAN_7_COMMIT.md` C3 (the UI for
  them) is still open. It also defers "applying the fixup / squash commits via
  autosquash" to this phase.
- `w` rewords only `HEAD` (`CommitKind::Reword`), whatever row the Commits
  pane has selected.
- The Status pane (`App::status_lines`) prints a conflict count but does not
  know that a merge or rebase is in progress.
- **A bug in shipped code, found while planning (fixed by R0).** `docs/PLAN_6_STAGING.md`
  says `<space>` on a conflicted file is inert. It is not:
  `App::stage_selected_file` sees `worktree != Change::None` and runs
  `git add -- <path>`. Checked with a throwaway test on a fresh `UU` file:
  `<space>` turned `UU f` into `M  f` while the file still contained
  `<<<<<<<` markers. `a` (`git add -A`) does the same to every conflicted
  file. This is fixed first (R0), because "mark resolved" is exactly the
  action this phase gives that key.

## Approach

### Rewrite: one `git rebase -i` with a generated todo

Same rule as phase 8: shell out, so hooks and git's own refusals apply.
ferrit never opens `$EDITOR` (the terminal is in raw mode on an alternate
screen, an editor would hang). Instead it hands git two scripted editors.
Everything below was checked on git 2.43 in a scratch repo:

| Need | Mechanism | Checked |
| --- | --- | --- |
| provide the todo | `GIT_SEQUENCE_EDITOR="cp '<todo file>'"`, git appends the todo path | yes, incl. a path with spaces |
| reword | `reword` line plus `GIT_EDITOR="cp '<message file>'"` | yes, subject and body kept |
| squash message | `GIT_EDITOR=true` keeps git's combined default | yes |
| continue / merge continue | `GIT_EDITOR=true git rebase --continue`, `git merge --continue` | yes |
| autosquash | `GIT_SEQUENCE_EDITOR=true git rebase -i --autosquash <base>` | yes |
| root commit in range | `git rebase -i --root` | yes |

Helper files (todo, message) go in `<git dir>/ferrit/`, never in the
worktree: a first attempt with them in the worktree left `?? msg.txt` behind.
They are removed after the run. `cp` limits this to Linux and macOS, the
platforms `PLAN_0_GENERAL.md` names for v1.0.

For a selected commit `H` the range and todo are:

```
history (oldest at the top)         action on H = reword | drop | edit
  A   <- base = H^   (or --root       todo = every commit of base..HEAD,
  H   <- selected      when H is the        oldest first, all `pick`,
  B                    root commit)         except H's line, which carries
  C   <- HEAD                               the action

action = squash | fixup   (into the commit below, i.e. its parent P = H^)
  Z   <- base = P^   (or --root)
  P                                       todo:  pick P
  H   <- selected                                squash H   (or fixup H)
  B                                              pick B ...
  C                                       H is the root commit: nothing below,
                                          key is inert
```

Refusals, all before any subprocess mutates anything:

| Case | Behaviour |
| --- | --- |
| range contains a merge commit (`git rev-list --merges <base>..HEAD` is not empty) | note: ferrit rebases linear history only |
| dirty worktree | git's own `cannot rebase: You have unstaged changes.` shown verbatim, no rebase state left behind (checked). No `--autostash`: same rule as phase 8's checkout, ferrit does not stash for the user. With phase 10 the message is one `s` away from solved |
| an operation already in progress | keys are inert, the menu is the way forward |
| squash / fixup on the oldest commit | inert |

### Outcome: decide by state, not by exit code

Checked: a conflict exits 1 and leaves `<git dir>/rebase-merge/`; an `edit`
stop exits **0** and also leaves it. So the outcome comes from the repository
state after the run, the same idea as phase 8's `MergeOutcome`:

```rust
pub enum RebaseOutcome {
    /// No rebase in progress afterwards.
    Done,
    /// Git stopped and waits for the user.
    Stopped { conflicted: bool },
}
```

`Stopped { conflicted: true }` when the index has conflicts, `false` for an
`edit` stop. A non-zero exit with no rebase state is
`GitError::RebaseFailed(stderr)` (the dirty-tree refusal above).

### Recover: read the state, offer one menu

`git2::Repository::state()` distinguishes merge, rebase (three variants),
cherry-pick and revert. ferrit maps it into an owned type (no `git2` type
leaves `src/domain/git/`) and reads rebase progress from the files git
writes:

```rust
pub enum Operation {
    Merge,
    Rebase { step: usize, total: usize },   // rebase-merge/msgnum, end
    CherryPick,
    Revert,
}
```

`rebase-merge/msgnum` and `end` exist in the state directory listing
checked above. For the older `rebase-apply` backend the equivalents are
`next` and `last`. Bisect and mailbox states map to `None`: ferrit has no
flow for them. `Snapshot` gains `operation: Option<Operation>`, so the
existing refresh path carries it (live refresh from phase 2.5 already
notices `.git` changes).

Menu entries per operation, each one command:

| Operation | Continue | Skip | Abort |
| --- | --- | --- | --- |
| Rebase | `git rebase --continue` | `git rebase --skip` | `git rebase --abort` |
| Merge | `git merge --continue` | none | `git merge --abort` |
| CherryPick | `git cherry-pick --continue` | `--skip` | `--abort` |
| Revert | `git revert --continue` | `--skip` | `--abort` |

The cherry-pick and revert rows follow git's documented command line and
were not exercised here; R1 tests them. All run with `GIT_EDITOR=true`.
Continue with unresolved files makes git refuse ("needs merge", checked);
ferrit shows that message and stays put.
`Abort` asks first (it throws away the resolution work so far); the other two
do not. After a continue or skip git may stop again on the next commit
(checked: skipping one conflicting pick reached the next conflict), so every
action returns a `RebaseOutcome` and the UI simply re-reads the state.

## What it has to resolve

```
 Commits pane, Nav                           a rebase is stopped on a conflict
+---------------------------+              +---------------------------------+
| 5e04050 docs: layout      |  s squash    | [1] Status                      |
| 23023d9 docs: inspiration |  S fixup     | repo -> main   REBASING 2/4     |
| 2f9bd4f docs: drop stub   |  w reword    | x 1 merge conflict(s)           |
| d5bc03c chore: initial    |  d drop      +---------------------------------+
+---------------------------+  e edit      | [2] Files                       |
 F fixup! commit  a autosquash            | UU src/main.rs                  |
                                          +---------------------------------+
                                           <space> mark resolved   m menu
  m (operation in progress)
+--------------------------------+
| Rebase 2/4                     |
| > Continue                     |
|   Skip this commit             |
|   Abort rebase                 |
+--------------------------------+
```

Bindings. All are new only where marked, all guarded by focus so they do
not collide with the global keys (`f` / `p` / `P` remote, `c` / `A` commit):

| Where | Key | Action |
| --- | --- | --- |
| Commits (Nav, not drilled) | `r`, `w` | reword the selected commit (HEAD keeps the phase 7 path, no rebase); `r` is lazygit's key and the one the key bar shows, `w` stays for existing users |
| Commits | `d` | drop, asks first |
| Commits | `s` | squash into the commit below, keeps both messages |
| Commits | `S` | fixup into the commit below, drops this message |
| Commits | `e` | rebase stops at it (`edit`), leaving the rebase in progress |
| Commits | `F` | `git commit --fixup=<hash>` against the selected commit, needs staged changes (`NothingStaged` otherwise) |
| Commits | `a` | autosquash every `fixup!` / `squash!` commit into its target, base = selected commit's parent |
| Files | `<space>` | on a conflicted file: mark resolved (`git add`), refused while conflict markers remain |
| anywhere | `m` | menu for the operation in progress, inert when none |

`w` on the Commits pane now follows the selection. Before, it always
reworded `HEAD`; that is a visible change, noted in the changelog.

`m` is unbound today; `M` (merge, Branches) is a different key. The menu is
a new `Popup::Menu` primitive rendered with the existing `SelectList`
(`src/components/ui/select_list.rs`, currently unused) inside a `Dialog`.
Phase 12 reuses it for the `x` menu, so it is built generic now: title,
items with a label, a shortcut letter and an action, `j` / `k`, `Enter`,
`Esc`, or the shortcut.

### Conflict markers guard (R0)

`<space>` and `a` on a conflicted path scan the file for a line starting
with `<<<<<<<` **and** a line starting with `>>>>>>>`. A lone `=======` does
not count: it is also a Markdown heading underline, and treating it as a
marker would make such a file impossible to stage. Markers present: refuse
with an error naming the file, stage nothing. No markers: `git add`, which is
what marks it resolved. `a` runs `git add -A -- . ':(exclude,literal)<path>'`
for the files it must refuse (checked: an excluded unmerged path stays `UU`,
and a path such as `we ird [1].txt` still stages), then names what it left. A
file that no longer exists (deleted on one side of the conflict) has no
markers. A file that cannot be read counts as having them: refusing is the
safe side of an unknown. A whole-file "take ours / take theirs" is out of
scope (see below), so the user edits the file elsewhere and comes back.

## State on `App`

- `operation: Option<git::model::Operation>` from `Snapshot`; `None` in
  `App::mock()`.
- `Popup::Menu(MenuState)`; `MenuState { title, items, selected }` with
  `MenuItem { label, shortcut, action: MenuAction }` and
  `MenuAction::{ Continue, Skip, Abort }` for this phase (phase 12 adds more
  variants). `PopupView::Menu(..)` and a `PopupKind::Menu` arm in
  `src/app/popups.rs`.
- `ConfirmAction::{ DropCommit { hash }, AbortOperation }` in `run_confirm`.
- `CommitDraft` gains `target: Option<String>` so the reword popup knows
  whether it is amending `HEAD` or rewording an older commit.
- New file `src/app/rebase_actions.rs` (one responsibility per file, like
  `branch_actions.rs`): `reword_selected`, `drop_prompt`, `squash_selected`,
  `fixup_selected`, `edit_selected`, `create_fixup`, `autosquash`,
  `open_operation_menu`, `run_menu_action`, `finish_rebase(outcome)`.
- Status pane: `status_lines` adds `REBASING 2/4` or `MERGING` (warning
  colour) whenever `operation` is `Some`. The keybar shows `Menu: m` in
  place of the default hint while an operation is in progress.
- No new refresh path: every action ends in `request_refresh()`, which
  re-reads `operation`.

### Backend surface

```rust
// src/domain/git/rebase.rs (new), thin wrappers in mod.rs
pub enum RebaseEdit { Reword(String), Drop, Edit, Squash, Fixup }

pub fn rebase_edit(&self, hash: &str, edit: RebaseEdit) -> GitResult<RebaseOutcome>;
pub fn autosquash(&self, hash: &str) -> GitResult<RebaseOutcome>;
pub fn operation(&self) -> GitResult<Option<Operation>>;
pub fn operation_step(&self, op: &Operation, step: Step) -> GitResult<RebaseOutcome>; // Continue | Skip | Abort
pub fn commit_message(&self, hash: &str) -> GitResult<String>;   // pre-fill reword
pub fn has_conflict_markers(&self, path: &Path) -> GitResult<bool>;
```

`GitError` gains `RebaseFailed(String)` and `OperationFailed(String)` (the
`--continue` / `--abort` refusals). The todo builder is a pure function
`build_todo(commits, hash, edit) -> String` so it is unit-testable without a
repository.

## Synchronous on purpose

A rebase of a handful of recent commits is local and fast, like phase 8's
merge, so it runs on the input thread. A long range, or a slow
`post-rewrite` hook, would freeze the UI; phase 9's `RemoteOp` worker is the
pattern to reuse if that shows up in practice. Deferred, named below.

## Edge cases

| Case | Behaviour |
| --- | --- |
| conflict during reword / drop / squash | `Stopped { conflicted: true }`: Status shows REBASING, Files shows `UU`, the menu is one `m` away |
| `edit` stop | `Stopped { conflicted: false }`, HEAD is on the selected commit, the user amends or commits, then `m`, Continue |
| continue with an unresolved path | git refuses (`needs merge`), message shown, state unchanged |
| skip reaches another conflict | outcome `Stopped` again, UI re-reads |
| abort | confirm, then `--abort`, the branch returns to its pre-rebase tip |
| reword hook or `commit-msg` rejects | rebase stops, `m` offers abort or continue; git's hook output shown |
| detached HEAD | works, git updates `HEAD` |
| selected commit is the root | reword / drop / edit use `--root`; squash / fixup inert |
| rewriting pushed commits | the branch is now ahead and behind its upstream; the phase 9 `P` guard (force-with-lease confirm when behind) must fire. Covered by an explicit test, see below |
| external `git rebase` running | `operation` is `Some`, our rewrite keys are inert |
| Commits pane drilled or Branches drill | rewrite keys inert, as `d` already is there |

## Out of scope

- **Move a commit up / down** (lazygit `Ctrl-j` / `Ctrl-k`). `Ctrl-j` is a
  line feed, indistinguishable from `Enter` in most terminals, and `J` / `K`
  scroll the right pane. Needs the keymap work of phase 12.
- **In-TUI conflict editor** and whole-file take ours / take theirs. The user
  edits the file elsewhere; the marker guard keeps them honest. Candidate
  for the phase 12 `x` menu (two commands: `git checkout --ours|--theirs`).
- **Rebase the current branch onto another** (lazygit `r` on Branches) and
  `--onto`. One command, its own follow-up, plus a design question on which
  key.
- **Rebasing merge commits** (`--rebase-merges`). Refused with a note.
- **Editing the remaining todo** of a stopped rebase, and showing the
  pending todo in the Commits pane. `REBASING 2/4` is the progress signal.
- **Cherry-pick and revert as ferrit actions.** They appear in the menu only
  when something else started them.
- **Running on a worker thread.** See "Synchronous on purpose".
- **`--autostash`.** Deliberate, see the refusal table.
- **A squash message editor.** `s` keeps git's combined default; a custom
  message means reword afterwards.

## Self-testing (see `PLAN_SELF_TESTING.md`)

`tests/git_rebase.rs` (backend, temp repos like `tests/git_stash.rs`):

- `build_todo` for each `RebaseEdit`, root commit, and squash / fixup into
  the parent (pure, no repo)
- reword an older commit: subject and body replaced, later commits intact,
  worktree clean
- drop removes the commit and its file; squash and fixup merge into the
  parent, fixup dropping the message
- edit: `Stopped { conflicted: false }`, `operation()` is `Rebase`
- a conflicting drop: `Stopped { conflicted: true }`, `Operation::Rebase`
  with `step` / `total`, index has conflicts
- continue with an unresolved file is `OperationFailed`; after resolving,
  `Done`; skip and abort return the branch to a sane state
- merge conflict from phase 8: `operation()` is `Merge`, abort clears it,
  continue after resolving commits
- dirty worktree: `RebaseFailed`, no rebase state directory
- range with a merge commit: refused before any mutation
- autosquash folds a `fixup!` commit into its target
- `git2` state mapping: each `RepositoryState` variant lands on the intended
  `Operation` (or `None`)

`tests/app_rebase.rs` (`App` seams, like `tests/app_stash.rs`):

- Commits `w` on an older commit opens the popup pre-filled, `Enter` rewords;
  `w` on the first row keeps the phase 7 amend path
- `d` asks, `y` drops, `n` keeps; `s` / `S` / `e` / `F` / `a` happy paths
- a conflicting action shows REBASING in Status and `m` opens the menu;
  Continue, Skip, Abort each behave, Abort behind a confirm
- `<space>` on `UU` with markers stages nothing and says why; without
  markers it stages; `a` skips conflicted files with markers (regression
  test for the R0 bug)
- rewrite keys inert while an operation is in progress, in a drill, and for
  the wrong pane
- rewriting a pushed branch then `P` asks for force-with-lease
- prior-phase keys still work: `c`, `A`, `f` / `p` / `P`, `d` on Files and
  Branches, stash keys

`tests/render.rs`: Status with REBASING, the menu popup, and the keybar
swap.

## Milestones

- ✅ **R0** conflict-marker guard on `<space>` and `a`
  (`Repo::has_conflict_markers`, `Repo::stage_all_except`,
  `App::has_markers` / `unresolved_conflicts`). `tests/git_conflict.rs` (7)
  and `tests/app_conflict.rs` (6) green; disabling the guard makes the two
  regression tests fail, so they do guard the bug. Shipped alone.
- ✅ **R1** state read: `Operation` (`model.rs`), `operation.rs`,
  `Snapshot.operation`, the Status badge. `tests/git_rebase.rs` (9) and
  `tests/app_operation.rs` (6) green; mutating the progress file names, the
  revert mapping, the badge position or the refresh assignment fails them.
  The keybar `Menu: m` hint moved to R2 (see the note at the top): showing it
  before `m` does anything would advertise a dead key.
- ✅ **R2** `Popup::Menu` primitive (`src/app/menu.rs`, `SelectList` inside a
  `Dialog`) and the `m` menu. Covers all four operations, not only a merge
  (see the note at the top). `tests/git_rebase.rs` (18) and
  `tests/app_menu.rs` (14) green; each of these fails a test when broken: the
  skip flag, the `CONFLICT (` check, the abort confirm, a merge row for skip.
  Closes phase 8's dangling note: the conflicted-merge popup now points at
  `m`.
- ✅ **R3** `rebase.rs`: `build_todo`, `rebase_edit`, `autosquash`,
  `commit_message`, `GitError::RebaseFailed`, and `operation::settle` shared
  with `step`. 24 new cases in `tests/git_rebase.rs` (38 in the file); each of
  these fails a test when broken: the squash anchor, the reword exec line, the
  merge-commit guard, the idle guard, keeping the scratch file while stopped.
  No UI yet (R4).
- ✅ **R4** Commits pane keys `w` / `d` / `s` / `S` / `e` (`src/app/rebase_actions.rs`),
  the reword popup with a `RewordTarget` (`commit.rs`), `finish_operation`
  shared with the `m` menu (`menu.rs`), a Commits keybar. `tests/app_rewrite.rs`
  (13) green; each of these fails a test when broken: the in-progress guard,
  the drill guard, not saving an older reword as the next draft, hiding the
  sign-off line. The rebase entries of the `m` menu shipped with R2.
- ✅ **R5** `F` (fixup commit, closes `PLAN_7` C3), `a` autosquash.
  `tests/app_rewrite.rs` now 20 cases; each of these fails a test when
  broken: the fold pre-check, its range bound, the fixup's target.
- ✅ **R6** polish: `cargo fmt --check`, clippy `-D warnings` and rustdoc
  `-D warnings` clean, `src/domain/git/` has no `ratatui` import, 333 tests
  green, `CHANGELOG.md` lines, `PLAN_0` and `PLAN_7` C3 flipped. Every row of
  the edge-case table has a test:
  - conflict during a rewrite, `edit` stop, unresolved continue, skip onto a
    new conflict, abort: `tests/git_rebase.rs` (`step-*`, `rw-drop`,
    `rw-edit`) and `tests/app_menu.rs`;
  - a rejecting hook: `a_rejecting_hook_leaves_the_rebase_stopped_...`;
  - detached HEAD: `a_detached_head_can_be_rewritten`;
  - the root commit: reword, drop (`the_root_commit_can_be_dropped_...`),
    edit, and squash / fixup refused with `no commit below`;
  - rewriting pushed commits: `rewriting_pushed_commits_leaves_the_branch_
    ahead_and_behind` (backend, `2` ahead and `2` behind) and
    `pushing_after_rewriting_pushed_commits_asks_for_force_with_lease` (`P`
    asks, `n` pushes nothing);
  - a rebase started from another shell: `rewrite_keys_are_inert_while_
    another_shell_has_a_rebase_running`;
  - the pull-that-starts-a-rebase note from phase 9:
    `a_pull_that_starts_a_rebase_and_conflicts_is_a_stopped_rebase`, then
    aborted from the same state machine;
  - Commits drilled: `rewrite_keys_are_inert_inside_a_commit_...`.
  The definition of done's "never opens `$EDITOR`" is
  `no_rewrite_ever_opens_the_users_editor`, with `core.editor` and
  `sequence.editor` pointing at a script that records a call. Note for
  whoever runs it: this shell exports `GIT_EDITOR=true`, which hides a
  regression, so the check is only meaningful with the variable unset
  (`env -u GIT_EDITOR cargo test --test git_rebase`); under that condition
  removing the override makes the suite hang on git's default editor rather
  than fail cleanly.

## Definition of done (phase 11)

- `<space>` and `a` never stage a file that still has conflict markers.
- Reword, drop, squash, fixup and edit work on any commit of a linear range;
  a merge commit in the range is refused before anything changes.
- `F` creates a `fixup!` commit and `a` folds every pending one.
- A stopped rebase, merge, cherry-pick or revert shows in Status with its
  progress, and `m` offers continue, skip (where git has it) and abort;
  abort asks first.
- Conflicts show in Files, can be marked resolved once the markers are gone,
  and continue after that finishes the operation.
- ferrit never opens `$EDITOR`; a failed hook or refusal shows git's own
  message and leaves the repository in a state `m` can leave.
- `w` on the Commits pane follows the selection (changelog entry).
- `src/domain/git/` still has no `ratatui` import.
- `cargo clippy --all-targets` clean; `tests/git_rebase.rs`,
  `tests/app_rebase.rs`, `tests/render.rs` and all earlier test files pass.

## After phase 11

Phase 12 is polish. It reuses `Popup::Menu` for the `x` context menu, and its
keymap work is what unlocks moving commits and clickable hints. Its command
log is best landed before this phase's code if possible: this phase adds the
most `git` subprocesses so far, and seeing them makes them far easier to
debug.
