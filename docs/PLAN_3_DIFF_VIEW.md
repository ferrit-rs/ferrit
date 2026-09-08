# Plan: phase 3, diff view

## Goal

Replace the two remaining mock diff bodies in the right pane — Files'
`RIGHT_DIFF` and Commits' `RIGHT_COMMIT` — with real diffs computed by
`git2`, syntax-highlighted, scrollable, with hunk-to-hunk navigation. Read
only: no staging, no `git apply`, no index writes. Turning a hunk or a line
into something you can act on is phase 4.

Branches' "Log" body and a stash entry's diff stay mock for now; see "Out of
scope" below.

## Backend: `src/git/diff.rs`

`git2::Patch` does the hunk/line extraction we'd otherwise hand-roll: one
`Patch` per changed file, `None` for a binary file, `patch.hunk(i)` /
`patch.line_in_hunk(i, j)` for the rest. No external diff crate needed — the
`git2` dependency phase 2 already accepted covers this too.

```rust
use git2::{Patch, Repository};

pub struct Repo { /* ... */ }

impl Repo {
    /// One file's diff. `side` picks which two trees `git2` compares.
    pub fn file_diff(&self, path: &Path, side: DiffSide) -> GitResult<FileDiff>;

    /// A commit's diff against its first parent (the empty tree for a root
    /// commit). One `FileDiff` per file the commit touched, `git show` order.
    /// `hash` is `CommitEntry::full_hash`.
    pub fn commit_diff(&self, hash: &str) -> GitResult<Vec<FileDiff>>;
}
```

`DiffSide::Worktree` -> `repo.diff_index_to_workdir` (index vs workdir, with
`include_untracked(true)` so a new file diffs as all-additions instead of
being skipped). `DiffSide::Staged` -> `repo.diff_tree_to_index` (HEAD's tree
vs index). Same two comparisons `git::status::files()` already reasons about
via `FileEntry::staged` / `FileEntry::worktree`; `diff.rs` just computes the
content instead of the one-letter code.

`commit_diff` diffs `commit.tree()` against `commit.parent(0).tree()`, or
against no tree (`repo.diff_tree_to_tree(None, Some(&tree), ..)`) when the
commit has no parent. A merge commit diffs against its first parent only;
a real combined/conflict view is deferred (see "Out of scope").

## Data model

Plain owned structs, matching phase 2's rule: no `git2` type escapes
`src/git/`.

```rust
pub enum DiffSide { Staged, Worktree }

pub struct FileDiff {
    pub path: PathBuf,
    /// Empty when `binary` is true.
    pub hunks: Vec<Hunk>,
    pub binary: bool,
}

pub struct Hunk {
    /// The full `@@ -a,b +c,d @@ context` line, unstyled.
    pub header: String,
    pub lines: Vec<DiffLine>,
}

pub struct DiffLine {
    pub origin: LineOrigin,
    pub old_lineno: Option<u32>,
    pub new_lineno: Option<u32>,
    /// No trailing newline.
    pub content: String,
}

pub enum LineOrigin { Context, Addition, Deletion }
```

`CommitEntry` (in `src/git/model.rs`, landed in phase 1) gains one field:

```rust
pub struct CommitEntry {
    pub short_hash: String,
    /// New: full 40-hex-char hash, so `commit_diff` can look the commit up
    /// without a `git2::Oid` leaking past `src/git/`.
    pub full_hash: String,
    pub author: String,
    pub summary: String,
    pub time: i64,
}
```

`FileEntry::binary` (added in phase 2 G1, always `false` so far) stays a
stub — the diff view doesn't read it. A binary file simply comes back as
`FileDiff { binary: true, hunks: vec![], .. }` from `Patch::from_diff`
returning `None`, and the right pane shows a one-line note instead of hunks.

## App wiring

A second cached, rebuilt-on-nav value alongside `preview: Preview` (phase 2
G6), not a replacement for it — an image selection still wins:

```rust
enum DiffView {
    /// Status, Branches or Stash focused; nothing to diff.
    None,
    /// A read failed, or there's nothing to show. Never a panic.
    Note(String),
    Files(git::FileDiff),
    Commit(git::CommitEntry, Vec<git::FileDiff>),
}
```

`App` adds `diff: DiffView` and `right_scroll: usize`. Both rebuild wherever
`update_preview()` already runs today (focus change, selection change,
`refresh()`) — folded into that same method, renamed `update_right_pane()`,
which also resets `right_scroll = 0` so a new selection always opens at the
top.

Files pane picks `DiffSide::Worktree` when `FileEntry::worktree != Change::None`,
else `DiffSide::Staged` (a fully-staged file still shows its diff instead of
going blank) — this is a heuristic, not a toggle; showing both stacked, or a
key to flip sides, is a candidate follow-up, not phase 3.

## Rendering deltas (ui/)

`draw_right_pane`'s precedence becomes: image preview (unchanged, phase 2
G6) → `DiffView` when Files or Commits is focused → the existing mock
`Paragraph` for Status / Branches / Stash (unchanged for now).

- Hunk header: cyan (`theme::HUNK`, already used for the mock version).
- Line prefix (` ` / `+` / `-`): gray / green / red (`theme::IDLE` / `ADD` /
  `DEL`), same palette as the mock `diff_text`.
- The rest of an added/removed/context line's text: syntax-highlighted (see
  below) rather than one flat colour — closer to `delta`'s look
  (`docs/INSPIRATION.md`) than the phase-1 placeholder.
- `DiffView::Commit`: a small metadata block (hash, author, date, subject)
  above the files, each file introduced by a `diff --git a/.. b/..`-style
  separator line, bold/idle — same shape as the mock `RIGHT_COMMIT` text.
- Binary: one dim line, `"binary file, no preview"`. Empty diff (clean file,
  or a `FileDiff` with no hunks): `"no changes to show"`.
- Scrolling: `Paragraph::scroll((app.right_scroll as u16, 0))`. Clamping to
  "last line minus pane height" needs the last rendered height, which `App`
  doesn't track; for phase 3, clamp only to the diff's total line count and
  let an over-scroll render a blank tail. Pixel-perfect bottom clamping is a
  polish item if it turns out to matter.

## Syntax highlighting

`syntect`, language picked from the file's extension
(`SyntaxSet::find_syntax_by_extension`, falling back to plain text). Use the
`default-fancy` feature (pure-Rust `fancy-regex` engine) instead of
`default-onig`, so this doesn't add a second C dependency next to git2's
vendored libgit2. Syntax definitions and the colour theme come from
syntect's bundled defaults — no asset directory to ship or find at runtime.

`docs/INSPIRATION.md` already names `syntect` as the candidate ("syntax
highlighting engine delta uses") and `delta` as the quality bar to aim for.

## Keybindings (new in phase 3)

Only active while the right pane is showing a `DiffView` (Files or Commits
focused with a real diff, not an image or a note); inert everywhere else,
same rule as phase 1's keybar.

| Key | Action |
| --- | --- |
| `Ctrl-d` / `Ctrl-u` | scroll the right pane half a page down / up |
| `]` / `[` | jump to the next / previous hunk (Files); next / previous file's diff (Commits, when the commit touches more than one file) |

## Dependencies

```toml
syntect = { version = "5", default-features = false, features = ["default-fancy"] }
```

No new dependency for the diff computation itself — `git2::Patch` (already
in the tree since phase 2) covers it.

## Out of scope

- Staging or unstaging anything (phase 4).
- A real commit-graph render for the Branches pane's "Log" body — it stays
  the mock `RIGHT_LOG` text; `theme::commit_line`'s graph glyph stays a
  static `o` (noted as deferred back in `PLAN_1_LAYOUT.md`).
- A stash entry's diff (Stash pane's right side stays `(no stash entries)` /
  mock) — natural fit for phase 8, or a small follow-up before it.
- Status pane's right side stays the mock summary text (`mock::RIGHT_STATUS`);
  it's not a diff, wiring it to the real header is unrelated polish.
- Word-level (intra-line) diff highlighting, side-by-side layout, combined
  merge-commit diffs, difftastic-style structural diff. All named as later
  ambitions in `docs/INSPIRATION.md`, none needed to call phase 3 done.
- A line-count / file-size cap before highlighting. A very large diff may be
  slow; revisit if it's visibly a problem, don't pre-optimize.

## Self-testing (see `PLAN_SELF_TESTING.md`)

The replay harness (`xtask fixture`, `--replay`, ST1..ST3) still hasn't
landed — phase 2 shipped G0/G1/G3..G6 against throwaway `git2` repos built
directly in the test file instead, and phase 3 does the same:

- `tests/git_diff.rs`: a modified tracked file, a new untracked file, a
  binary file (write bytes containing a NUL), a fully-staged file, a
  first-parent commit, and the root commit (empty-tree diff) — each
  asserted against `Repo::file_diff` / `Repo::commit_diff` directly.
- `tests/render.rs`: one targeted `TestBackend` snapshot of the right pane
  built from a fixed `FileDiff` (no repo needed), mirroring how `App::mock()`
  already carries the embedded PNG for the phase-2 image-preview test.
- `PLAN_SELF_TESTING.md`'s per-phase table already has phase 3's row
  ("modify a file, `git::diff` hunks match golden; right pane shows them;
  hunk nav moves the viewport") — wire the real script + golden once
  ST1..ST3 land, whichever phase gets there first.

## Milestones

- **D0** `src/git/diff.rs`: `DiffSide`, `FileDiff`, `Hunk`, `DiffLine`,
  `LineOrigin`. `Repo::file_diff` for `DiffSide::Worktree`, unit tested
  against a modified tracked file and a new untracked file.
- **D1** `Repo::file_diff` for `DiffSide::Staged`. Binary files come back as
  `FileDiff { binary: true, .. }`, unit tested.
- **D2** `CommitEntry::full_hash` added (`git::log::commits`, `mock::mock_commits`
  both updated). `Repo::commit_diff`, unit tested against a normal commit,
  a merge commit (first-parent only), and the root commit.
- **D3** `DiffView` in `App`, folded into the rename of `update_preview()` to
  `update_right_pane()`. Files and Commits panes render real diffs: hunk
  header colour, `+`/`-` line colour, per-file separators for commits,
  binary/empty notes. `right_scroll` + `Ctrl-d`/`Ctrl-u`.
- **D4** `]`/`[` hunk-jump keys. `syntect` wired: language from extension,
  highlighted spans layered under the existing diff-line colour.
- **D5** polish: `cargo clippy --all-targets` clean; resizing or scrolling
  past the end of an empty/short/very long diff never panics;
  `tests/render.rs` region snapshot; `tests/git_diff.rs` covers every case
  in D0..D2.

## Definition of done (phase 3)

- Files pane's right side shows the selected file's real diff (worktree vs
  index, falling back to index vs HEAD for a fully-staged file),
  syntax-highlighted, scrollable.
- Commits pane's right side shows the selected commit's real metadata and
  diff for every file it touched, including the root commit and a merge
  commit (first-parent).
- Binary and empty diffs show a plain note; never raw bytes, never a panic.
- `]`/`[` hunk navigation and `Ctrl-d`/`Ctrl-u` half-page scroll work and
  reset to the top on a new selection.
- `src/git/` still has no `ratatui` import (`cargo tree` check from phase 2
  still holds).
- `cargo clippy --all-targets` clean; `tests/git_diff.rs` passes.

## After phase 3

Phase 4 builds directly on `Hunk` / `DiffLine`: a hunk (then a line) becomes
selectable and stageable, via `git2` index surgery or `git apply` on a
constructed patch. Branches' "Log" body, a stash entry's diff, and Status's
right-side summary stay mock/static until their own phases (6, 8) or a
dedicated small follow-up.
