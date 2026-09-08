# Plan: phase 2, read-only git backend

## Goal

Replace `mock.rs` with real repository data, one pane at a time, behind a
headless `src/git/` module that never imports `ratatui`. Read only: no
staging, no commit, no mutation of any kind. The TUI becomes a consumer of
`git::Repo` snapshots instead of hardcoded strings.

This phase starts, per `PLAN_0_GENERAL.md`, with **Status + Files**. Branches,
Commits and Stash are specified here and land in follow-up commits (G3..G5)
against the same module.

## Backend choice

`git2` (libgit2 bindings). Pragmatic default from `docs/INSPIRATION.md`:
mature, covers status / refs / log / stash / blob reads without shelling out.
A C dependency, accepted. `gix` for fast reads and `git` subprocess for the
awkward mutations stay on the table for later phases, not now.

## The module boundary

```
src/git/          <- no `ratatui` import anywhere under here
  mod.rs            Repo: open(path) -> Repo, holds git2::Repository
  error.rs          GitError, converted into color_eyre::Report at the edge
  status.rs         header (branch, upstream, ahead/behind, conflicts)
                    + working-tree entries (staged vs worktree, binary flag)
  refs.rs           local branches, which one is HEAD, upstream per branch
  log.rs            recent commits, bounded count
  stash.rs          stash entries
  blob.rs           raw bytes of a path at a revision (image preview now,
                    diffs in phase 3+)
```

`src/ui/` still has no git logic. `app.rs` is the only glue: it owns a
`git::Repo` and the cached snapshots, and asks the backend to refresh.

## Data model

Plain owned structs, `#[derive(Debug, Clone)]`, no lifetime tied to
libgit2. The UI never sees a `git2::*` type.

`BranchEntry` / `CommitEntry` / `StashEntry` and `CommitEntry::author_initials`
landed early in `src/git/model.rs` as part of phase 1's lazygit re-skin (M6):
`mock.rs` builds them now, G3..G5 fill the same structs from real reads. Also
early: `Repo::name()` for the `ferrit -> main` status line, and the right-pane
contextual titles (`Pane::right_title`).

```rust
pub struct StatusHeader {
    pub branch: String,           // "main", or a short hash when detached
    pub detached: bool,
    pub upstream: Option<String>, // "origin/main"
    pub ahead: usize,
    pub behind: usize,
    pub conflicts: usize,
}

pub struct FileEntry {
    pub path: PathBuf,
    pub staged: Change,           // index vs HEAD
    pub worktree: Change,         // worktree vs index
    pub binary: bool,
}

pub enum Change {
    None, Modified, Added, Deleted, Renamed, Typechange, Untracked, Conflicted,
}

pub struct BranchEntry {
    pub name: String,
    pub is_head: bool,
    pub upstream: Option<String>,
    pub ahead: usize,
    pub behind: usize,
}

pub struct CommitEntry {
    pub short_hash: String,       // 7 chars
    pub summary: String,          // first line of the message
    pub author: String,
    pub time: i64,                // commit time, seconds since epoch
}

pub struct StashEntry {
    pub index: usize,
    pub message: String,
}
```

`Repo` exposes one call per pane, each returning a `Result<Vec<_>>` or
`Result<StatusHeader>`. No iterators leak libgit2 lifetimes.

```rust
impl Repo {
    pub fn open(path: &Path) -> Result<Repo, GitError>;
    pub fn name(&self) -> String;                      // repo dir name, infallible
    pub fn status_header(&self) -> Result<StatusHeader, GitError>;
    pub fn files(&self) -> Result<Vec<FileEntry>, GitError>;
    pub fn branches(&self) -> Result<Vec<BranchEntry>, GitError>;
    pub fn commits(&self, max: usize) -> Result<Vec<CommitEntry>, GitError>;
    pub fn stashes(&self) -> Result<Vec<StashEntry>, GitError>;
    pub fn blob_bytes(&self, path: &Path, rev: Rev) -> Result<Vec<u8>, GitError>;
}
```

`stash` list needs `&mut git2::Repository` in libgit2; wrap the handle in a
`RefCell` inside `Repo`, or take `&mut self` for `stashes`. Decide at
implementation, keep it off the public error surface.

## App wiring

```rust
struct App {
    repo: git::Repo,
    focus: Pane,
    selection: [usize; 5],
    show_help: bool,
    should_quit: bool,

    header: StatusHeader,
    files: Vec<FileEntry>,
    branches: Vec<BranchEntry>,
    commits: Vec<CommitEntry>,
    stashes: Vec<StashEntry>,
    last_error: Option<String>,   // shown in the status pane, never a panic
}
```

- `App::new(path)` opens the repo and does one `refresh()`.
- `refresh()` re-reads every wired pane, catches `GitError`, stores it in
  `last_error` instead of propagating, leaves the old snapshot in place.
- `r` key triggers `refresh()`. Filesystem watching (`notify`) is phase 10.
- Not a git repo: `Repo::open` fails, `main` prints a plain message and
  exits non-zero. No alt-screen garbage.

Reads are synchronous in phase 2. Moving them off the UI thread (gitui's
`asyncgit` discipline) is deferred; note it here, revisit when a pane read
is visibly slow on a large repo.

## mock.rs

Shrinks as panes get wired. Status + Files strings go first. Branches,
Commits, Stash keep their mock until G3..G5. When empty, delete the file
and the `mod mock;`.

## Rendering deltas (ui/)

- Status pane: `header` -> `ferrit <branch> ↑<ahead> ↓<behind>`, a second
  line for conflicts or `✓ no merge conflicts`, and `last_error` in red
  when set.
- Files pane: one row per `FileEntry`, two-column XY status code like
  `git status --porcelain` (`M `, ` M`, `??`, `A `, `UU`, ...), coloured
  by `theme::file_line` (already meaning-aware).
- Empty states: "working tree clean", "no local branches" (won't happen),
  "no commits yet" (fresh repo), "(no stash entries)".
- Right pane stays mock text in phase 2; real diffs are phase 3.

## Image preview (ratatui-image)

Pulled forward into **phase 2** (milestone G6). A compatible release
(`ratatui-image 11`, built against `ratatui 0.30`) exists now, so there is
no reason to hold the whole feature for phase 3. Phase 3 keeps only the
polish: richer sixel / kitty output, size cues, hooking Commits as well.

- Backend: `Repo::blob_bytes(path, rev)` with `Rev::{Workdir, Head}`
  returns the raw bytes of a blob. `FileEntry::binary` flags non-text.
- UI, shipped: when Files is focused and the selected entry's path has an
  image extension (`png jpg jpeg gif webp bmp ico`) and the blob decodes,
  the right pane shows the picture instead of the mock diff. `App` owns a
  `preview::Preview` rebuilt on every nav key and on `refresh()`.
  - `ratatui_image::picker::Picker` starts on `halfblocks()`, which draws
    in any terminal (and in `TestBackend`), so there is always something.
  - `App::detect_graphics()` runs once before the alternate screen and
    upgrades the picker to sixel / kitty / iterm2 via
    `Picker::from_query_stdio()` when the real terminal answers.
  - Decode bytes with the `image` crate, hand the `DynamicImage` to a
    `StatefulImage` widget. Anything that fails to decode or encode falls
    back to a `Preview::Note` line. Never an error, never a panic.
- `src/git/` never imports `ratatui-image` or `image`. Decoding lives in
  `src/preview.rs`, the only module that touches either crate.

## Dependencies

```toml
git2 = { version = "0.21", default-features = false, features = ["vendored-libgit2"] }
ratatui-image = { version = "11", default-features = false, features = ["crossterm", "image-defaults"] }
image = { version = "0.25", default-features = false, features = ["png", "jpeg", "gif", "webp", "bmp", "ico"] }
```

`CommitEntry::time` is a raw epoch `i64` in phase 2 (no date crate). Add
`time` or `jiff` only when the Commits pane needs relative formatting.

`vendored-libgit2` avoids a system libgit2 requirement on contributor
machines and CI. Drop it later if we want the system lib.

`ratatui-image 11` needs rustc 1.86, so `Cargo.toml` sets
`rust-version = "1.86"`.

## Self-testing (see PLAN_SELF_TESTING.md)

Lands in the same commits as the features:

- `xtask fixture canonical` builds the state from `PLAN_1_LAYOUT.md`:
  4 commits on `main`; branches `feat/tui-skeleton`, `fix/parse-args`;
  working tree with 1 modified (`src/main.rs`), 1 untracked
  (`docs/notes.md`), 1 staged (`Cargo.lock`); empty stash.
- `tests/git_backend.rs`: unit tests for `status_header` and `files`
  against `canonical` and against a fresh `git init` (no commits) and a
  non-repo temp dir.
- `test/scripts/20-status-files.script`: focus Status then Files, with an
  inline git golden block:

  ```
  size 120x40
  fixture canonical
  key 1
  snapshot status-pane
  expect-text "main"
  key 2
  snapshot files-pane
  expect-text "M  Cargo.lock"
  expect-text " M src/main.rs"
  expect-text "?? docs/notes.md"
  git status --porcelain=v2 -> "1 .M"
  ```

- `tests/render.rs`: targeted `TestBackend` snapshot of the Status region
  and the Files region rendered from a fixed `Vec<FileEntry>`, so the
  rendering is pinned without needing a real repo.

## Milestones

- **G0** done. `git::Repo::open`, `git::error`, `status::header`. Status
  pane shows the real branch and ahead/behind. Non-repo dir prints one line
  and exits non-zero before the terminal is touched. `tests/git_backend.rs`
  covers header on a fresh `git init`, a committed repo, and a non-repo dir.
  (The `xtask fixture canonical` builder is still pending; tests build their
  own throwaway repos with `git2` for now.)
- **G1** done. `status::files()`: working-tree entries, staged vs worktree,
  sorted, `binary` flag (always `false` until a later milestone). Files pane
  renders them via `theme::file_line`. Empty -> "working tree clean".
- **G2** partial. `r` triggers `App::refresh()`; a `GitError` during
  refresh lands in `last_error` and renders red in the Status pane instead
  of propagating, old snapshot left in place. `20-status-files.script` and
  its git golden wait on the replay harness (`PLAN_SELF_TESTING.md`).
- **G3** done. `git::refs::branches()`: local branches, HEAD first then
  alphabetical, upstream + ahead/behind via `graph_ahead_behind`. Branches
  pane renders them via `theme::branch_line`; folded into `Repo::snapshot()`
  alongside header/files rather than a standalone method, matching G0/G1.
- **G4** done. `git::log::commits(repo, max)`: revwalk from HEAD,
  `Sort::TIME | Sort::TOPOLOGICAL` (TOPOLOGICAL breaks ties between commits
  made in the same second), bounded by `COMMITS_LIMIT`. Unborn branch (fresh
  repo) comes back empty, not an error.
- **G5** done. `git::stash::stashes()`: `stash_foreach`, which needs
  `&mut git2::Repository` — resolved by making `Repo::snapshot()` take
  `&mut self` (the plan's "decide at implementation" note), no `RefCell`
  needed.
- **G6** done. `blob_bytes(path, rev)` with `Rev::{Workdir, Head}`, unit
  tested in `tests/git_backend.rs`. `src/preview.rs` decodes the bytes;
  the right pane shows the image for an image selection in Files, on a
  half-block picker that upgrades to sixel / kitty when the terminal
  answers. `App::mock()` carries an embedded 8x8 PNG so the render tests
  exercise the path with no repo. Richer graphics polish stays phase 3.

`mock.rs` now only feeds `App::mock()` (the repo-free render-test path); every
pane reads from `git::Repo` when a real repo is open.

Phase 2 as scheduled now = **G0..G1 + G3..G6 done, G2 partial** (the `r`-key
refresh and `last_error` path are wired; the replay-harness script test
still waits on `PLAN_SELF_TESTING.md`).

## Definition of done (phase 2)

- `src/git/` has no `ratatui` dependency; `cargo tree -e no-dev` proves it.
- Status and Files panes show real data for the repo `ferrit` was pointed
  at (`-p/--path`, default `.`).
- A non-git directory produces a one-line message and a non-zero exit,
  terminal untouched.
- A `GitError` during `refresh()` shows in the Status pane; the app keeps
  running.
- `cargo clippy --all-targets` clean.
- `tests/git_backend.rs` and `test/scripts/20-status-files.script` pass.
- No panic on an empty repo, a bare repo, or a detached HEAD.
- Selecting an image file in Files shows it in the right pane (half-blocks
  at minimum); a non-decodable or missing blob shows a note, not a panic.

## After phase 2

Phase 3: real diffs in the right pane (syntax highlight via `syntect`,
hunk navigation, scrolling), and polish on the image preview that landed
in G6 (richer sixel / kitty output, size cues, Commits as a source).
