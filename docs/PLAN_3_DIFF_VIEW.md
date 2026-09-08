# Plan: phase 3, diff view

## Goal

Replace the two remaining mock diff bodies in the right pane, Files'
`RIGHT_DIFF` and Commits' `RIGHT_COMMIT`, with real diffs, coloured,
scrollable, with hunk-to-hunk navigation. Read only: no staging, no
`git apply`, no index writes. Turning a hunk or a line into something you can
act on is phase 4.

Branches' "Log" body and a stash entry's diff stay mock for now; see "Out of
scope".

## Approach: do what lazygit does

lazygit does not compute diffs with a library. It shells out to `git diff` /
`git show` and renders the output. The reference trees confirm it:

- `../ferrit-references/tui/lazygit/pkg/commands/git_commands/working_tree.go`
  `WorktreeFileDiffCmdObj`: `git diff --no-ext-diff --unified=<n>
  --find-renames=<t>% --submodule --color=always [--cached]
  [--no-index -- /dev/null] [--ignore-all-space] -- <paths>`.
- `.../commit.go` `ShowCmdObj`: `git show --no-ext-diff --unified=<n>
  --find-renames=<t>% --submodule --color=always --stat --decorate -p <hash>`.
- `../ferrit-references/tui/gitu/src/git/mod.rs` (`diff_unstaged`,
  `diff_staged`, `show`): the same, in Rust, via `std::process::Command`,
  then parsed by `gitu_diff::Parser` into structs whose every field is a
  byte `Range<usize>` into the original text.

Consequences ferrit takes on deliberately:

- **User `git config` is honoured for free.** `diff.algorithm`,
  `diff.wsErrorHighlight`, `diff.mnemonicPrefix`, `core.pager` (ignored, we
  don't page), `diff.noprefix`, and anything else the user set applies,
  because it is their `git` running. `git2::Patch` would have ignored all of
  it.
- **No `syntect`.** lazygit shows git's own colouring: hunk header, `+`/`-`
  lines, and (when enabled) whitespace-error and word-diff. It does not do
  language syntax highlighting; the "delta look" is delta, an *external*
  diff renderer the user opts into (`diff.external`). ferrit matches that:
  phase 3 colours exactly what git colours, and an external renderer is a
  named follow-up, not phase 3. Dropping `syntect` also drops a dependency.
- **Phase 4 staging gets easier, not harder.** A stageable patch is a byte
  slice of the diff text (`file header + one hunk`, or `file header + hunk
  header + selected lines`), fed to `git apply --cached`. Same technique as
  `../ferrit-references/tui/gitu/src/git/diff.rs` `format_hunk_patch` /
  `format_line_patch` and lazygit `pkg/commands/patch/`.
- **`git` must be on `PATH`.** It already is a hard runtime dep of a git TUI.
  Tests need it too; the fixture repos are still built with `git2` in-test
  (phase 2 style), then `git diff` is run against them.

### Data flow

```
  selection change  |  focus change  |  AppEvent::Refresh (fs-watch / poll)
                    \        |        /
                     v       v       v
        DiffCmd::build(side, &DiffOpts)
           git diff --no-ext-diff --color=never
             --unified=<ctx> --find-renames=<t>%
             [--cached] [--no-index -- /dev/null] [--ignore-all-space]
             -- <path>
                     |
                     v
        std::process::Command  --stdout bytes-->  String (from_utf8_lossy)
                     |
                     v
        parse::diff(&text) -> Diff {
            text: String,                 // owned, never mutated after parse
            files: Vec<FileMeta> {        // every field below is Range<usize>
                header,                   //   into `text`
                old_path, new_path,
                status: Added|Deleted|Modified|Renamed|Copied,
                binary: bool,
                hunks: Vec<HunkMeta> { header, body },
            },
        }
                     |
                     v
        render::to_text(&diff, &Theme) -> ratatui::text::Text<'static>
            (colour +/-/@@/diff --git/index lines ourselves;
             this is what `git --color=always` would have emitted)
                     |
                     v
        DiffView::Files(diff)   cached on App, drawn with
        Paragraph::scroll((right_scroll, 0))
```

`--color=never` on purpose: parsing our own plain text is deterministic for
snapshot tests, and re-colouring from the parse is trivial. lazygit uses
`--color=always` and an ANSI parser because Go; we get the same picture
without the escape-sequence round-trip.

## Backend: `src/git/diff.rs` + `src/git/diff/parse.rs`

```rust
use std::process::Command;

/// Which pair of trees `git diff` compares. NOT a reuse of `blob::Rev`
/// (see schema below).
pub enum DiffSide { Staged, Worktree }

/// Knobs ferrit exposes, mirroring lazygit's `git.*` config. Defaults match
/// lazygit's defaults so behaviour is familiar out of the box.
pub struct DiffOpts {
    pub context: u32,            // lazygit git.diffContextSize, default 3
    pub ignore_whitespace: bool, // lazygit git.ignoreWhitespaceInDiffView, default false
    pub rename_threshold: u32,   // lazygit git.renameSimilarityThreshold, default 50
}
impl Default for DiffOpts { /* 3, false, 50 */ }

impl Repo {
    /// One file's worktree-or-staged diff. Runs `git diff` in `workdir()`.
    pub fn file_diff(&self, path: &Path, side: DiffSide, opts: &DiffOpts)
        -> GitResult<Diff>;

    /// A commit against its first parent (empty tree for a root commit).
    /// Runs `git show <hash>`. `hash` is `CommitEntry::full_hash`.
    pub fn commit_diff(&self, hash: &str, opts: &DiffOpts) -> GitResult<Diff>;
}
```

`Diff` holds *all* files (a commit touches many; a single-file `file_diff`
just yields a one-element `files`). One owned `Diff` type, not a
`FileDiff` vs `Vec<FileDiff>` split.

### The two sides, and why `DiffSide` is its own enum

```
        git diff                    git diff --cached           git show <hash>
 workdir <----------> index   index <-----------> HEAD    HEAD tree <------> parent tree
    DiffSide::Worktree            DiffSide::Staged                commit_diff(hash)

 ferrit runs the SAME argv lazygit runs. `git diff` (no --cached) = worktree
 vs index. `git diff --cached` = index vs HEAD. New/untracked file: add
 `--no-index -- /dev/null <path>` so it shows as all-additions.

 blob::Rev { Workdir, Head }  = ONE version of ONE path's bytes  (image preview)
 DiffSide  { Staged, Worktree } = a PAIR of trees to hand to `git diff`
 A shared "which version" enum would have to mean both and fit neither.
```

Files pane picks `DiffSide::Worktree` when `FileEntry::worktree != Change::None`,
else `DiffSide::Staged`, so a fully-staged file still shows its diff instead
of going blank. Heuristic, not a toggle; a key to flip sides, or showing both
stacked, is a phase-3+ candidate (lazygit shows staged and unstaged as two
selectable sections; ferrit can grow into that).

### `commit_diff`

`git show <hash>` already diffs against the first parent and prints an
empty-tree diff for the root commit, so there is nothing special to code for
either. A merge commit: `git show` defaults to the condensed combined format;
pass `-m --first-parent` to get a plain first-parent diff (a real combined
conflict view is deferred, see "Out of scope").

An unknown hash is a first-class error, not `GitError::Read`. `GitError` is a
`thiserror` enum now, so:

```rust
#[error("no such commit: {0}")]
NoSuchCommit(String),
```

Detect it from `git`'s exit status + stderr (`fatal: bad object <hash>`) and
return `NoSuchCommit(hash)`. Same shape for a non-zero `git diff` exit in
`file_diff`: wrap stderr in a new `#[error("git diff failed: {0}")]
DiffFailed(String)` rather than pretending it was a read.

### The parser (`src/git/diff/parse.rs`)

Small hand-rolled scanner, ~60 lines, byte offsets only, never copies out of
`text`:

```
split text on "\ndiff --git "         -> one FileMeta per chunk
  within a chunk:
    line "diff --git a/X b/X"          -> old_path / new_path ranges
    "new file mode" / "deleted file"   -> status Added / Deleted
    "rename from" / "rename to"        -> status Renamed, both paths
    "Binary files ... differ"          -> binary = true, hunks stay empty
    "--- " / "+++ "                    -> skip (redundant with diff --git)
    split rest on "\n@@ "              -> one HunkMeta per chunk
      first line "@@ -a,b +c,d @@ ctx" -> header range
      remaining lines                  -> body range (used verbatim in phase 4)
```

`../ferrit-references/tui/gitu/src/gitu_diff.rs` is the full-fidelity
version (1351 lines: quoted paths, mode changes, `\ No newline at end of
file`, submodule lines). Keep it as the reference to consult when an edge
case bites; do not vendor it for a read-only view.

## Data model

No `git2` type and no subprocess handle escapes `src/git/`. `Range<usize>`
does, paired with the `text` it indexes, exactly like gitu's public `Diff`.

```rust
pub struct Diff {
    pub text: String,
    pub files: Vec<FileMeta>,
}
pub struct FileMeta {
    pub header: Range<usize>,
    pub old_path: Range<usize>,
    pub new_path: Range<usize>,
    pub status: FileStatus,
    pub binary: bool,
    pub hunks: Vec<HunkMeta>,
}
pub enum FileStatus { Added, Deleted, Modified, Renamed, Copied }
pub struct HunkMeta {
    pub header: Range<usize>,   // the "@@ ... @@ ctx" line
    pub body:   Range<usize>,   // lines after it, up to the next hunk/file
}
```

Line-origin (`Context` / `Addition` / `Deletion`) is not stored: it is the
first byte of each body line (` ` / `+` / `-` / `\`), read at render time.
This keeps the model a thin index over `text` and makes a phase-4 patch a
pure slice.

`CommitEntry` (in `src/git/model.rs`) gains `full_hash`:

```rust
pub struct CommitEntry {
    pub full_hash: String,     // new: 40 hex chars, feeds commit_diff
    pub short_hash: String,    // now derived: full_hash[..7], never drifts
    pub author: String,
    pub summary: String,
    pub time: i64,
}
```

`git::log::commits()` already builds these from `git2`; set
`full_hash = oid.to_string()` and `short_hash = full_hash[..7].to_owned()`
(drop the separate `git::short_hash(&oid)` call for commits so the two can
never disagree). `mock::mock_commits()` updated to carry a plausible 40-char
hash.

`FileEntry::binary` (phase 2 G1 stub, always `false`) stays untouched; the
diff view reads `FileMeta::binary` from the parse instead.

## App wiring

A second cached, rebuilt-on-nav value alongside `preview: Preview` (phase 2
G6), not a replacement: an image selection still wins.

```rust
enum DiffView {
    None,                 // Status / Branches / Stash focused
    Note(String),         // read failed, or nothing to show. never a panic
    Files(git::Diff),
    Commit(git::CommitEntry, git::Diff),
}

struct App {
    // ...
    diff: DiffView,
    right_scroll: usize,
    right_key: Option<RightKey>,   // identity of what `diff` describes
}

enum RightKey {
    File { path: PathBuf, side: DiffSide },
    Commit { full_hash: String },
}
```

### `update_preview()` -> `update_right_pane()`

`update_preview()` has **five call sites** today
(`src/app.rs` ~124 in `mock()`, ~156, ~180 in `refresh()`, ~238, ~398).
The rename touches all five. Every one of them must now also refresh
`diff`, because the fs-watch / 10s poll (`src/events.rs`, phase 2.5) fires
`refresh()` on external changes and the diff has to follow, lazygit-style.

The method must NOT blindly `right_scroll = 0`. That was fine when it only
ran on a keypress; now that it also runs every poll tick and every file
save, zeroing scroll would yank the viewport out from under a user who is
just reading. Reset scroll only when the *selection identity* changes:

```
update_right_pane():
    key_now = right_key_for(focus, selection)     // None | File{path,side} | Commit{hash}

    if key_now != self.right_key {                 // moved to a different thing
        self.right_key    = key_now.clone()
        self.right_scroll = 0                       // open at the top
        self.diff         = build(key_now)          // run git, parse, store
    } else {                                        // same thing, background refresh
        let rebuilt = build(key_now)
        if rebuilt.text != self.diff_text() {       // content changed on disk
            self.diff = rebuilt                     // keep right_scroll as-is
        }
        // unchanged: do nothing. no git re-run cost beyond the one `build`,
        // no realloc, no re-render churn
    }
```

`build()` failure -> `DiffView::Note(msg)`, `right_scroll` left alone.
`key_now == None` -> `DiffView::None`.

This is the concrete form of `PLAN_0_GENERAL.md`'s cross-cutting rule: a
phase-3+ cached right-pane value rebuilds on `AppEvent::Refresh` without
discarding scroll/view state that belongs to an unchanged selection.

### Cheap change detection

`build()` runs `git` every time `update_right_pane()` is called for a live
selection (keypress, poll tick, fs event). That is one subprocess per event,
which is what lazygit does and is not a problem at human interaction rates.
The guard above compares the resulting `text` and skips the *render* (the
expensive part: building a `Text` with per-line styling) when the diff is
byte-identical to what is already shown. No hashing, no blob-oid bookkeeping:
`String` equality on text that is almost always a few KB.

If `git` invocation cost ever shows up in a profile, cache by
`(path, side, opts, index_mtime)` and skip the subprocess too. Not phase 3.

## Rendering (`src/ui/`)

`draw_right_pane` precedence:

```
  image preview (phase 2 G6)          -- unchanged, still wins
    else DiffView::Files / ::Commit   -- when Files or Commits is focused
      else DiffView::Note             -- dim single line
        else mock Paragraph           -- Status / Branches / Stash, unchanged
```

`theme::diff_text(raw: &'static str) -> Text<'static>` goes away. It
string-sniffs a `&'static str` and cannot take an owned runtime diff. Replace
with:

```rust
// src/ui/diff.rs  (or theme.rs, matching where the other renderers live)
pub fn render_diff(diff: &git::Diff, focus_hunk: Option<usize>) -> Text<'static>;
```

walking `diff.files` / `hunks` and slicing `diff.text`:

- file separator `diff --git a/… b/…`: `IDLE` + `BOLD`.
- hunk header `@@ … @@ ctx`: `HUNK` (cyan), as the mock already did.
- body line by first byte: `+` -> `ADD` (green), `-` -> `DEL` (red),
  ` ` -> `IDLE`, `\` (`\ No newline…`) -> `IDLE` + dim.
- `focus_hunk` (from `]` / `[`): that hunk's header gets `REVERSED` so the
  jump target is visible. Optional polish, drop if it complicates D4.

`DiffView::Commit` prepends a metadata block from the `CommitEntry`
(`git show` also prints it, but we already have it parsed and styled by
`theme::commit_line`'s palette):

```
+- Commit ---------------------------------------------------------+
| commit 80d3f04c9a1b2e4f...                                       |  HASH green
| Author: Max Wells <max@…>                                        |  AUTHOR magenta
| Date:   2026-09-08 08:59                                         |  IDLE
|                                                                  |
|     fix(image): wipe leftover graphics pixels when the preview   |  bold
|     stops being an image                                         |
|                                                                  |
| diff --git a/src/image/preview.rs b/src/image/preview.rs         |  IDLE bold
| @@ -40,7 +40,9 @@ impl Preview {                                  |  HUNK cyan
|      fn stop(&mut self) {                                         |  IDLE
| -        self.clear();                                            |  DEL red
| +        self.clear();                                            |  ADD green
| +        self.wipe_graphics();                                    |
+-----------------------------------------------------------------+
   ]/[ next/prev file      Ctrl-d/Ctrl-u half page
```

- binary file: one dim line `binary file, no diff`.
- empty (`diff.files` empty, or a file with no hunks): `no changes to show`.
- Scrolling: `Paragraph::scroll((right_scroll as u16, 0))`. Clamp only to
  the diff's total line count for phase 3; `App` does not track the last
  rendered pane height, so an over-scroll renders a blank tail rather than
  hard-stopping at the last screenful. Pixel-exact bottom clamp is polish.

## Keybindings (new in phase 3)

Active only while the right pane shows a `DiffView::Files` / `::Commit` with
a real diff (not an image, note, or mock). Inert otherwise, same rule as
phase 1's keybar.

| Key | Action |
| --- | --- |
| `Ctrl-d` / `Ctrl-u` | scroll the right pane half a page down / up |
| `]` / `[` | next / previous hunk (Files); next / previous file (Commit, when it touches more than one file) |

Half-page = a fixed constant for phase 3 (the pane height is not threaded
into `on_key`); `]` / `[` set `right_scroll` to the target hunk/file's first
line index, computed from the parse.

## Dependencies

None added. `std::process::Command` runs `git`. `git2` (in the tree since
phase 2) still backs Status / Files / Branches / Commits / Stash and the
commit metadata; it is just not used for diff text.

`syntect` is NOT added (removed from this plan versus the earlier draft).

## Out of scope

- Staging / unstaging (phase 4). The parse is shaped for it: patch = slice
  of `diff.text`.
- An external diff renderer (`delta`, `difftastic`) via `diff.external` /
  a ferrit config key. lazygit supports it; ferrit's `DiffCmd` builder
  leaves room for one `--ext-diff` + `-c diff.external=…` addition later.
  Named follow-up, not phase 3.
- Language syntax highlighting beyond git's own colouring. lazygit does not
  do it natively; `../ferrit-references/tui/gitu` layers tree-sitter and
  `rendering/delta` is the quality bar, both are post-phase-3 ambitions
  (`docs/INSPIRATION.md`).
- Word-level / intra-line highlight, side-by-side layout, combined
  merge-commit diff.
- Branches' "Log" body: stays mock `RIGHT_LOG`; `theme::commit_line`'s graph
  glyph stays a static `o` (deferred in `PLAN_1_LAYOUT.md`).
- A stash entry's diff: Stash right side stays mock. Natural fit for phase 8
  (`git stash show -p`, same subprocess pattern).
- Status pane's right side: stays mock `RIGHT_STATUS`.
- A size cap before rendering. A huge diff may be slow to style; revisit
  with a profile, do not pre-optimize.

## Self-testing (see `PLAN_SELF_TESTING.md`)

The replay harness (`xtask fixture`, `--replay`, ST1..ST3) still has not
landed; phase 3 does what phase 2 did, throwaway `git2` repos built in the
test, then `git` run against them.

- `tests/git_diff.rs`: modified tracked file, new untracked file
  (`--no-index`), binary file (bytes with a NUL), fully-staged file, a
  first-parent commit, a merge commit (`-m --first-parent`), the root commit
  (empty-tree). Each asserted against `Repo::file_diff` / `Repo::commit_diff`:
  file count, status, hunk count, first/last hunk header, an add/del line.
- `tests/diff_parse.rs`: feed `parse::diff` canned `git diff` output
  (rename, mode change, `\ No newline at end of file`, multi-file) and assert
  the ranges slice back to the expected substrings. No repo needed.
- `tests/render.rs`: one `TestBackend` snapshot of the right pane from a
  fixed `Diff` literal, mirroring the embedded-PNG phase-2 image test.
- `tests/app_refresh.rs` (or extend the phase 2.5 test): open a fixture
  repo, select a file, `app.right_scroll = 5`, mutate the file on disk,
  `app.refresh()`, assert `app.diff` text changed **and**
  `app.right_scroll == 5` (unchanged selection keeps its viewport); then
  select a different file and assert `right_scroll == 0`.
- `PLAN_SELF_TESTING.md`'s phase-3 row ("modify a file, hunks match golden;
  right pane shows them; hunk nav moves the viewport") gets its real script +
  golden once ST1..ST3 land.

## Milestones

- **D0** `src/git/diff.rs` + `src/git/diff/parse.rs`: `DiffSide`, `DiffOpts`,
  `Diff`, `FileMeta`, `FileStatus`, `HunkMeta`. `parse::diff` with
  `tests/diff_parse.rs` green (rename / mode / no-newline / multi-file).
- **D1** `Repo::file_diff` (both sides, `--no-index` for untracked, binary
  detection). `GitError::DiffFailed`. `tests/git_diff.rs` file-diff cases.
- **D2** `CommitEntry::full_hash`; `short_hash` derived from it;
  `git::log::commits` + `mock::mock_commits` updated. `Repo::commit_diff`
  (`-m --first-parent`, root commit, `NoSuchCommit`). `tests/git_diff.rs`
  commit cases.
- **D3** `DiffView` + `right_key` + `right_scroll` in `App`;
  `update_preview()` renamed `update_right_pane()` across all five call
  sites, with the change-detect / scroll-preserve logic above. Files and
  Commits render real diffs via `src/ui/diff.rs`; `theme::diff_text`
  removed. `Ctrl-d` / `Ctrl-u`. `tests/app_refresh.rs` green.
- **D4** `]` / `[` hunk/file jump. Commit metadata block. Binary / empty
  notes. `focus_hunk` reverse-highlight (optional).
- **D5** polish: `cargo clippy --all-targets` clean, no warnings; scrolling
  or resizing past the end of an empty / one-line / huge diff never panics;
  `tests/render.rs` region snapshot; every D0..D2 case covered.

## Definition of done (phase 3)

- Files pane's right side shows the selected file's real `git diff` (worktree
  vs index, falling back to index vs HEAD for a fully-staged file),
  coloured like `git --color`, scrollable.
- Commits pane's right side shows the selected commit's metadata and full
  `git show` diff, including the root commit and a merge commit
  (first-parent).
- User `git config` affecting diff output is respected (it is their `git`).
- Binary and empty diffs show a plain note; never raw bytes, never a panic.
- `]` / `[` and `Ctrl-d` / `Ctrl-u` work; scroll resets to the top on a new
  selection and is preserved across a background `refresh()` of an unchanged
  selection.
- `src/git/` still has no `ratatui` import (`cargo tree` check from phase 2).
- `cargo clippy --all-targets` clean; `tests/git_diff.rs`,
  `tests/diff_parse.rs`, `tests/app_refresh.rs` pass.

## After phase 3

Phase 4 builds on `Diff` / `HunkMeta`: a hunk, then a line range, becomes
selectable; the stage action slices `diff.text` into a patch and runs
`git apply --cached` (`git apply --cached -R` to unstage), the technique in
`../ferrit-references/tui/gitu/src/git/diff.rs` and lazygit
`pkg/commands/patch/`. An external diff renderer, the Branches "Log" graph, a
stash entry's diff, and Status's right-side summary stay mock/static until
their own phases or a dedicated follow-up.
