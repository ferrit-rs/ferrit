# Plan: phase 6, staging

**Status: S0-S4 implemented** (`src/git/apply.rs`, `Mode::Diff` in `App`,
`tests/apply_patch.rs` / `git_stage.rs` / `app_stage.rs`). S5's exhaustive
edge-case sweep and the `40-stage.script` golden are still open, and three
deliberate deviations from the plan text below are called out in
"Implementation notes" before "After phase 6".

## Goal

Turn the read-only diff from phase 3 into something you act on: stage and
unstage at **file**, **hunk** and **line** granularity, plus **discard** of a
worktree change, with the panes refreshing after. This is the first phase that
writes to the repository (the index, and for discard the worktree). Nothing
here rewrites history; `HEAD` never moves. Committing the staged index is
phase 7.

Scope is exactly `PLAN_0_GENERAL.md`'s phase 6 line: "stage / unstage at file,
hunk, line; refresh after". Staging a whole directory, a `--patch`-style
interactive builder, and partial-line (intra-line) staging are out.

## Approach: patch = a slice of `diff.text`, piped to `git apply --cached`

Same technique as `../ferrit-references/tui/gitu/src/git/diff.rs`
(`format_hunk_patch` / `format_line_patch`) and lazygit `pkg/commands/patch/`.
Phase 3 was built for this on purpose (`PLAN_3_DIFF_VIEW.md` "Phase 6 staging
gets easier, not harder"): the parser already hands back byte `Range`s into one
owned `Diff::text`, so a stageable patch is a substring plus a synthesized
header, never a re-serialization.

```
  file    ->  no patch:  git add <path>  /  git restore --staged <path>
  hunk    ->  patch = file_header + hunk_slice           (both verbatim)
              git apply --cached [--reverse]  < patch
  line(s) ->  patch = file_header + rewritten hunk body  (see transform)
              git apply --cached --recount [--reverse]  < patch
  discard ->  same patches, git apply --reverse [--index]  (worktree side)
```

Consequences ferrit takes on, matching phase 3's reasoning:

- **`git` does the applying.** We do not re-implement three-way apply. Fuzz,
  whitespace policy, `core.autocrlf`, and `apply.whitespace` are git's problem
  because it is the user's `git` running in their worktree.
- **One subprocess per action.** No long-lived patch buffer, no on-screen
  patch-builder state machine. lazygit keeps one (`pkg/commands/patch/`)
  because it lets you assemble a patch across files before applying; ferrit
  applies immediately and re-reads, lazygit-style, which the phase 2.5
  fs-watch already makes cheap and consistent.
- **`--recount` for line patches only.** A whole hunk's `@@ -a,b +c,d @@`
  counts are already correct, so hunk staging needs no recount. A line patch
  drops or demotes body lines, so the counts are wrong by construction; rather
  than recompute them (lazygit's `transform.go` does, by hand), pass
  `--recount` and let git fix the header. Keeps the transform to one pass.
- **Plain-text diff is the exact input.** Phase 3 runs `git diff
  --color=never` once and colours from the parse, so `diff.text` is byte-for-
  byte what `git apply` expects. No escape-sequence stripping.

### The three granularities

```
        Files pane           right pane (DiffView::Files, phase 3)
   ┌ [2] Files ──────────┐   +- Unstaged changes -------------------------+
   │  M src/app.rs        │   | diff --git a/src/app.rs b/src/app.rs      |  file
   │ >M src/git/diff.rs   │<--| index 1a2b3c..4d5e6f 100644              |  header
   │ ?? docs/notes.md     │   | --- a/src/git/diff.rs                     |
   │                      │   | +++ b/src/git/diff.rs                     |
   └──────────────1 of 3 ─┘   | @@ -10,6 +10,8 @@ fn file_diff(          |> hunk 0
                              |      let workdir = workdir(repo)?;        |  cursor
                              |  +    let started = Instant::now();       |  line  <- V-select
                              |  +    trace!("file_diff {path:?}");       |  line  <- V-select
                              |       let mut cmd = DiffCmd::base(...);   |
                              |  @@ -40,3 +40,4 @@ fn commit_diff(...) {  |  hunk 1
                              |       Ok(Diff::new(text))                 |
                              |  +    // TODO combined merge view         |
                              +--------------------------------------------+

   space on the Files row      -> stage / unstage the whole FILE
   space with a hunk cursor    -> stage / unstage that HUNK
   space with a line V-select  -> stage / unstage those LINES
```

The cursor lives in the right pane now, not just the left. Phase 3's
`right_scroll: usize` grows into a cursor that also scrolls (the phase 3
"After phase 3" note: "adopt a gitui-style `VerticalScroll` that also keeps the
cursor line on screen").

### Stage vs unstage is one key, direction inferred

`<space>` toggles. Direction comes from where the change currently sits, read
from the `FileEntry` phase 2 already provides:

```
  entry.worktree != Change::None  ->  <space> stages   (worktree -> index)
  entry.worktree == Change::None
      && entry.staged != Change::None  ->  <space> unstages  (index -> worktree)
```

A partially-staged file (both sides non-`None`) stages the rest on `<space>`;
an explicit unstage key handles the other direction. lazygit uses `<space>` to
stage and `u` is implicit in context; ferrit keeps `<space>` = toggle for the
common case and adds no second binding unless testing shows it is needed.

## Backend: `src/git/apply.rs`

New module under `src/git/`, sibling of `diff.rs`. No `ratatui`, same rule as
the rest of `git::`. Reuses `diff::DiffCmd`'s spawn pattern (`git -C <workdir>
...`) but for `apply` / `add` / `restore`.

```rust
//! Write the index (and, for discard, the worktree) by piping a patch built
//! from `Diff` byte ranges to `git apply`. See docs/PLAN_6_STAGING.md.

use std::ops::Range;

/// Which way a patch runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyDir { Forward, Reverse }

/// What the patch touches. `Index` alone = stage/unstage. `Worktree` /
/// `WorktreeAndIndex` = discard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyTarget { Index, Worktree, WorktreeAndIndex }

impl Repo {
    /// Stage or unstage a whole file. No patch: `git add` / `git restore
    /// --staged`. Untracked file stages via `git add` too.
    pub fn stage_file(&self, path: &Path, dir: ApplyDir) -> GitResult<()>;

    /// Stage / unstage one hunk. `patch` is `file.header.start .. hunk.body.end`
    /// over `diff.text`, handed in by the caller (App) so this module never
    /// re-runs the diff.
    pub fn apply_hunk(&self, patch: &str, dir: ApplyDir, target: ApplyTarget)
        -> GitResult<()>;

    /// Stage / unstage / discard a set of body lines within one hunk. `lines`
    /// are indices into the hunk body (0-based, context lines included and
    /// ignored). The transform below turns the selection into a valid patch.
    pub fn apply_lines(
        &self,
        file_header: &str,
        hunk_header: &str,
        hunk_body: &str,
        lines: &[usize],
        dir: ApplyDir,
        target: ApplyTarget,
    ) -> GitResult<()>;
}
```

`GitError` gains one variant, same shape as phase 3's `DiffFailed`:

```rust
#[error("git apply failed: {0}")]
ApplyFailed(String),
```

Non-zero exit from `git apply` -> `ApplyFailed(stderr)`. The caller turns it
into a `last_error` line in the Status pane (the phase 2 mechanism), never a
panic, never a partial write we hide: `git apply` is atomic per invocation, so
a failed apply left the index untouched.

### The line transform (`apply_lines`)

Walk the hunk body with `split_inclusive('\n')`, one pass, gitu's
`format_line_patch` rule:

```
  for each body line, by its first byte:
    ' '  (context)          -> keep verbatim
    '+'  selected           -> keep verbatim
    '+'  not selected       -> DROP the line entirely
    '-'  selected           -> keep verbatim
    '-'  not selected       -> DEMOTE to context: replace leading '-' with ' '
    '\'  (\ No newline...)   -> keep verbatim, attach to the line above
  prepend: file_header + hunk_header
  the '@@ -a,b +c,d @@' counts are now wrong -> pass --recount, git recomputes
```

`ApplyDir::Reverse` (unstage / discard) swaps the roles: an unselected `-`
becomes context, an unselected `+` is dropped, then `git apply --reverse` reads
the patch backwards. This is exactly gitu's `PatchMode::Reverse`
(`tui/gitu/src/git/diff.rs`); keep that file open while implementing.

Reference for the full-fidelity version (offset recomputation by hand, no
`--recount`): lazygit `pkg/commands/patch/transform.go`. ferrit does not need
it; `--recount` is the shortcut the phase 3 plan already committed to.

### `stage_file` for the untracked case

`git add <path>` stages an untracked file whole. There is no line/hunk
granularity for a file git has never seen (the phase 3 `--no-index` diff is for
*display*, not `apply`); `<space>` on an untracked row always stages the whole
file, and the keybar/help say so. Unstaging it is `git restore --staged`
(which for a never-committed file just removes it from the index).

## Data model

No new owned type crosses out of `src/git/`. The patch strings are built in
`App` from the phase 3 `Diff` and passed in as `&str`. Phase 3's
`FileMeta` / `HunkMeta` already carry every range needed:

```rust
// full hunk patch (stage/unstage a hunk), no new field, phase 3 said so:
let patch = &diff.text[file.header.start .. hunk.body.end];

// line patch inputs:
let file_header = &diff.text[file.header.clone()];
let hunk_header = &diff.text[hunk.header.clone()];
let hunk_body   = &diff.text[hunk.body.clone()];
```

Body-line iteration yields `(Range<usize>, &str)` per line, the
`split_inclusive('\n') + scan` from `PLAN_3_DIFF_VIEW.md` "Data model". A line
index maps straight back to a `diff.text` range, so the render layer and the
patch builder agree on what "line 3 of hunk 1" is.

### Line origin, recomputed not stored

Still not stored (phase 3 decision holds): the first byte of a body line
(` ` `+` `-` `\`) is its origin, read at render and transform time. Context
lines are `unselectable`: the stage cursor skips them, and the V-select range
silently excludes them.

## App wiring

Phase 3 owns `diff: DiffView`, `right_key: Option<RightKey>`,
`right_scroll: usize`. Phase 6 adds a cursor and a selection anchor, and a
`Mode` so `<space>` / `d` mean different things in the left pane vs the diff.

```rust
/// Where keystrokes go. `Nav` is phase 1..3 behaviour unchanged.
enum Mode {
    Nav,          // left panes; j/k move the row cursor
    Diff,         // right pane focused; j/k move the line cursor, space stages
}

struct DiffCursor {
    /// Body-line index within the flattened diff (all files, all hunks), the
    /// same space `right_scroll` counts in.
    line: usize,
    /// V-select anchor. `None` = single line, `Some(a)` = range a..=line.
    anchor: Option<usize>,
    /// Content hash of the hunk the cursor sits in, so a background refresh
    /// can re-find the same hunk. gitu hashes `file_header + hunk` for its
    /// `Item.id` (tui/gitu/src/items.rs).
    hunk_id: u64,
}

struct App {
    // ...phase 3 fields...
    mode: Mode,
    cursor: DiffCursor,     // meaningful only in Mode::Diff
}
```

- New key **`Enter`** (or `l` / `Right` on the diff): `Nav` -> `Diff`, cursor
  to the first selectable line. **`Esc`** (or `h` / `Left`): back to `Nav`.
  This mirrors lazygit "focus the diff to stage within it".
- In `Mode::Diff`: `j` / `k` move `cursor.line` over selectable lines only,
  scrolling `right_scroll` to keep it visible; `V` toggles `anchor`;
  `<space>` stages/unstages the current granule; `d` discards it (with a
  confirm, see below); `]` / `[` still jump hunks; `Ctrl-d/u` still half-page.
- **Granule resolution** on `<space>`:

  ```
  anchor.is_some()                      -> lines  (anchor..=line, context dropped)
  else cursor covers a whole hunk's
       every +/- line via a "hunk cursor"-> hunk   (see note)
  else                                   -> the single line under the cursor
  ```

  Simplify for S1: `<space>` in `Mode::Diff` with no V-select acts on the
  **hunk** the cursor is in (matches lazygit's default granularity); a
  V-select acts on **lines**. A dedicated "stage exactly this one line"
  without selecting is not worth a separate rule. `<space>` on the **Files
  row** in `Mode::Nav` acts on the **file**.

### After the apply: refresh, keep your place

```
apply ok
  -> App::refresh()                 (phase 2: re-snapshot every pane)
  -> update_right_pane()            (phase 3: rebuild diff, guarded)
  -> re-find the cursor:
       the hunk with cursor.hunk_id still present?  -> cursor to its first line
       gone (fully staged, so it left this side)?   -> clamp to nearest hunk,
                                                       or drop to Mode::Nav if
                                                       the file has no more
                                                       changes on this side
```

This is the phase 3 rule taken one level deeper: phase 3 keeps `right_scroll`
across a refresh of an unchanged *file* selection; phase 6 keeps the *cursor*
across a refresh where the file is the same but a hunk just moved to the index.
`PLAN_3_DIFF_VIEW.md` "After phase 3" called this exact shot ("key the cursor
on a per-hunk content hash").

Staging from another shell (fs-watch `AppEvent::Refresh`) runs the same
re-find, so ferrit's diff cursor tracks the repo whoever moved it.

## Rendering (`src/ui/diff.rs`)

Phase 3's `render_diff(&Diff, focus)` gains a cursor argument:

```rust
pub fn render_diff(
    diff: &git::Diff,
    view: DiffRender,   // { scroll, cursor: Option<usize>, sel: Option<Range<usize>> }
) -> Text<'static>;
```

- **cursor line**: full-width `REVERSED` bar, the phase 1 selection-bar style
  but in the right pane. Only drawn in `Mode::Diff`.
- **V-select range**: `selectedLineBgColor` (the blue bar), same as a left-pane
  multi-select would look.
- **staged vs unstaged**, Files pane: phase 2 already colours the `XY` code via
  `theme::file_line`. No change; a file with both sides non-`None` already
  renders e.g. `MM`.
- **granule hint** in the right-pane title: `Unstaged changes` becomes
  `Unstaged changes  (hunk 1/3)` or `(lines 41-42)` while in `Mode::Diff`, so
  it is obvious what `<space>` will hit. lazygit shows the range in the view
  title the same way.
- **discard confirm**: a small centered modal, the phase 7 popup primitive
  borrowed early if it exists, else a one-line `y/n` prompt in the keybar
  region. `PLAN_0_GENERAL.md` principle: "Anything that loses work asks
  first." Staging never asks; discard always does.

```
  +- discard? --------------------------------------+
  |  discard 2 lines in src/git/diff.rs?            |
  |  this cannot be undone.        [y] yes   [n] no |
  +------------------------------------------------+
```

## Keybindings (new in phase 6)

Active per `Mode`, same "inert otherwise" rule as phase 1's keybar and phase
3's diff keys.

| Key | Mode | Action |
| --- | --- | --- |
| `<space>` | Nav, Files focused | stage / unstage the selected file |
| `Enter` / `l` / `Right` | Nav, Files or Commits | enter the diff (`Mode::Diff`) |
| `Esc` / `h` / `Left` | Diff | back to the left panes (`Mode::Nav`) |
| `j` / `k` | Diff | move the line cursor (selectable lines only) |
| `V` | Diff | start / clear a line V-selection |
| `<space>` | Diff | stage / unstage the hunk (or the V-selection) |
| `d` | Diff or Nav/Files | discard the granule / the file's worktree changes (confirm) |
| `]` / `[` , `Ctrl-d` / `Ctrl-u` | Diff | unchanged from phase 3 |
| `a` | Nav, Files focused | stage / unstage **all** files (`git add -A` / `git restore --staged .`) |

`a` (stage-all) is the one convenience beyond the phase 0 line, because it is
one `git` call, reversible, and every git TUI has it. Keybar gains
`Stage: <space>` becoming real, plus `Stage all: a` and `Discard: d`.

## Edge cases

| Case | Behaviour |
| --- | --- |
| untracked file | `<space>` stages whole via `git add`; no hunk/line granule |
| binary file | `<space>` stages whole; diff body is the phase 3 "binary file" note, `d`/line keys inert |
| rename (`R`) | file-level stage only for S1; hunk staging within a rename is a follow-up |
| mode change only | file-level stage; `git apply` handles the `old mode`/`new mode` lines in the hunk patch |
| `\ No newline at end of file` | kept verbatim by the transform, attached to the line above; `git apply --recount` copes |
| CRLF / `core.autocrlf` | not our problem: the patch is git's own diff output fed back to git's own apply |
| submodule change | `--submodule` (phase 3) already summarizes it; `<space>` stages the pointer via `git add`, no line granule |
| apply fails (context drift after an external edit mid-action) | `ApplyFailed` -> Status pane red line -> auto `refresh()` so the diff re-reads and the user retries |
| conflicted file (`UU`) | `<space>` inert in phase 6; resolving conflicts is phase 11 |
| empty selection (`V` over only context lines) | `<space>` is a no-op, brief keybar note "nothing to stage" |

## Self-testing (see `PLAN_SELF_TESTING.md`)

Same shape as phase 3: throwaway `git2` repos built in-test, then `git` run
against them; the replay harness (`xtask fixture`, `--replay`, ST1..ST3) is
still pending, so scripts wait.

- `tests/apply_patch.rs`: unit-test the line transform in isolation. Feed a
  known hunk body + a line-index set, assert the produced patch string
  byte-for-byte (forward and reverse), including the `\ No newline` and
  demoted-`-` cases. No repo.
- `tests/git_stage.rs`: fixture repo, then via `Repo`:
  - `stage_file` then `git status --porcelain=v2` shows the path staged;
    `stage_file(Reverse)` puts it back.
  - `apply_hunk` on a two-hunk file: exactly one hunk moves to the index
    (`git diff --cached` has hunk 0, `git diff` still has hunk 1).
  - `apply_lines` staging one `+` line of a three-line addition: the index has
    that line, the worktree diff still has the other two.
  - discard: `apply_hunk(Reverse, Worktree)` removes the change from the
    worktree; the file content matches `HEAD` for that hunk.
  - untracked `stage_file` via `git add`; binary file `stage_file`.
  - `apply` context-drift failure returns `ApplyFailed` and left the index
    unchanged.
- `tests/app_stage.rs` (extends phase 3's `app_refresh.rs`): open a fixture,
  `Enter` into the diff, `j` past a context line (assert the cursor skipped
  it), `<space>`, assert `app.files` now shows the file partially staged and
  `app.cursor.hunk_id` still resolves to a present hunk. Then stage the rest
  and assert `mode` fell back to `Nav`.
- `tests/render.rs`: one `TestBackend` snapshot of the right pane in
  `Mode::Diff` with a cursor bar and a two-line V-selection, from a fixed
  `Diff` literal.
- `test/scripts/40-stage.script` + inline git golden (lands when ST1..ST3 do):

  ```
  size 120x40
  fixture canonical
  key 2                       # focus Files
  key enter                   # into the diff
  key j
  key space                   # stage the first hunk
  snapshot hunk-staged
  git diff --cached --name-only  -> "src/main.rs"
  key escape
  key a                       # stage all
  git status --porcelain=v2      -> "1 M. "
  ```

## Milestones

- ✅ **S0** `src/git/apply.rs`: `ApplyDir`, `ApplyTarget`, `GitError::ApplyFailed`.
  `stage_file` (add / restore, incl. untracked). `tests/git_stage.rs`
  file-level cases green.
- ✅ **S1** `apply_hunk` (full-hunk patch = `file.header.start .. hunk.body.end`,
  forward + reverse). `Mode::Diff`, `DiffCursor`, `Enter` / `Esc`, `j` / `k`
  over selectable lines, cursor bar in `render_diff`. `<space>` in
  `Mode::Diff` stages/unstages the cursor's hunk. `<space>` on a Files row
  stages/unstages the file. Post-apply `refresh()` + cursor re-find.
- ✅ **S2** `apply_lines` + the transform, `tests/apply_patch.rs` green. `V`
  V-select, `<space>` stages the selection with `--recount`. Reverse for
  unstage.
- ✅ **S3** `d` discard at file / hunk / line, with the confirm modal (shipped
  as the keybar one-liner the plan names as the fallback, not a popup — see
  "Implementation notes"). `ApplyTarget::Worktree` / `WorktreeAndIndex`.
  `PLAN_0` "asks first" honoured.
- ✅ **S4** `a` stage-all / unstage-all. Right-pane title granule hint
  (`(hunk 1/3)` / `(lines 41-42)`). Keybar + `HELP` updated, `mock::KEYBAR`
  reflects the now-real bindings.
- 🟡 **S5** polish: `cargo clippy --all-targets` is clean and
  `tests/apply_patch.rs` / `git_stage.rs` / `app_stage.rs` all pass, but the
  edge-case table below is only spot-checked (untracked/binary/context-drift
  have tests; rename/mode-change/conflicted do not yet), there is no
  `tests/render.rs` snapshot of the cursor bar, and `40-stage.script` still
  waits on the replay harness like the rest of `PLAN_SELF_TESTING.md`.

## Definition of done (phase 6)

- `<space>` stages and unstages a whole file from the Files pane, and a hunk
  or a line-selection from inside the diff, with the panes reflecting the new
  index immediately.
- `d` discards a worktree change at the same three granularities, always
  after a confirm, never touching the index unless asked.
- Direction (stage vs unstage) is inferred from where the change sits; the
  common cases need no second key.
- A failed `git apply` shows in the Status pane and leaves the index and
  worktree exactly as they were; the diff re-reads so the user can retry.
- The diff cursor sticks to the same hunk across a background `refresh()`,
  including one triggered by staging from another shell.
- `src/git/` still has no `ratatui` import (`cargo tree` check from phase 2).
- `cargo clippy --all-targets` clean; `tests/apply_patch.rs`,
  `tests/git_stage.rs`, `tests/app_stage.rs` pass.
- Untracked, binary, rename, mode-change, and no-newline files each stage
  without a panic (whole-file where line granularity does not apply).

## Implementation notes

Three points where the shipped code reads the plan text above differently
than written, each because the codebase had already moved since this plan
was drafted, or because a literal reading left a gap the plan itself doesn't
resolve:

- **The cursor lives in one of *two* right-pane columns, not one.** This
  plan's ASCII diagram predates the Unstaged/Staged split
  (`ui::draw_files_columns`, landed after `PLAN_3_DIFF_VIEW.md`). `DiffCursor`
  carries a `side: DiffSide` and `Enter`/`l` picks it the same way the
  file-level toggle infers stage-vs-unstage: the worktree side if there's
  still something unstaged, else the staged side. `Tab` is not repurposed to
  swap sides — there was no obvious key left for it this round, so a
  half-staged file's *other* side is reached by leaving (`h`/`Esc`) and
  entering again is not yet wired either; revisit if that turns out to
  matter in practice.
- **`Enter` and `l` only — not `Right`/`Left`.** `Right`/`Left` already cycle
  panes (phase 1, still tested by `arrows_cycle_panes_and_wrap`); repurposing
  them for a Files-only mode would have made them mean two different things
  depending on focus. `Esc`/`h` leave; there is no `Right`-cycles-panes vs.
  `Right`-enters-diff conflict to resolve later since `Right` was never bound
  to entering it.
- **The cursor re-find never switches sides.** "clamp to nearest hunk, or
  drop to `Mode::Nav` if the file has no more changes on this side" is
  implemented literally: `resync_diff_cursor` only ever looks at
  `cursor.side`'s own diff. Once the Worktree side runs out of hunks, `Mode`
  drops to `Nav` even though the Staged side (now larger) is sitting right
  there — switching the user's cursor to the other column on their behalf
  would silently change what the next `<space>` does (stage vs. unstage),
  which seemed worse than one extra `l` press.

## After phase 6

Phase 7 commits the staged index: a message-input popup, `git commit`, plus
`--amend`, reword, and `fixup!` / `squash!` shapes for autosquash. It builds
directly on the index this phase writes; `git status --porcelain=v2`'s staged
column is the precondition it checks before enabling the commit key. See
`PLAN_7_COMMIT.md`.

Deferred out of phase 6, revisit with their own phases or a follow-up:

- hunk / line staging *within* a rename or a copy (file-level only for now).
- an across-files patch builder (assemble, review, then apply once), lazygit
  `pkg/commands/patch/`. ferrit applies immediately; add only if a real
  workflow needs the staged-patch preview.
- `--patch`-style "split this hunk" (`s` in `git add -p`). The parser has the
  hunk body; splitting is a UI affordance on top, not a backend change.
- staging a conflicted file's resolution (phase 11, conflict flow).
- an undo of the last stage/discard via the reflog / `git stash` (see
  `INSPIRATION.md` git-time-machine); phase 12 territory.
