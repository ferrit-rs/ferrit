# Plan: phase 11, rebase and conflict flow

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
| Commits (Nav, not drilled) | `w` | reword the selected commit (HEAD keeps the phase 7 path, no rebase) |
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
- **R1** state read: `Operation`, `Snapshot.operation`, Status line and
  keybar hint. `tests/git_rebase.rs` state-mapping cases green.
- **R2** `Popup::Menu` primitive and the `m` menu for a merge in progress
  (continue, abort with confirm). Closes phase 8's dangling note.
- **R3** `rebase.rs`: `build_todo`, `rebase_edit`, `autosquash`, outcomes,
  errors. Backend tests green, no UI yet.
- **R4** Commits pane keys `w` / `d` / `s` / `S` / `e`, reword popup with
  `target`, `finish_rebase`, the rebase entries of the `m` menu.
- **R5** `F` (fixup commit, closes `PLAN_7` C3), `a` autosquash.
- **R6** polish: `cargo clippy --all-targets` clean, `cargo fmt --check`,
  layering held (`src/domain/git/` has no `ratatui`), every prior test
  green, every edge-case row has a test or an explicit inert path,
  `CHANGELOG.md` lines, `PLAN_0` and `PLAN_7` C3 flipped.

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
