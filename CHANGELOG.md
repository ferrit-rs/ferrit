# Changelog

All notable changes to ferrit are recorded here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and the project aims to follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

- App errors now retain typed Git/app categories and appear in Status plus a persistent bottom-right toast, dismissible with its `x` button.

- Bottom panel now has an `Infos` heading above its frame; configured Git
  author name appears inside the frame, right-aligned beside the first command.
- Clicking the configured author opens an animated right-side Git identity drawer.
- Hovering the configured author shows a native pointing-hand cursor in terminals supporting OSC 22.

## [0.4.0] - 2026-09-20

### Added

- Commits pane: `Enter` on a commit row drills into that commit's changed
  files as a lazygit-style tree, replacing the commit list in place (the
  pane title becomes `[4] Diff files (<hash> <summary>)`); a directory row
  toggles collapsed with `Enter`, and the Patch pane shows the commit's full
  diff. `Esc`/`h` backs out to the commit list.
- The staged index can now be committed: `c` opens a message popup and
  creates a normal commit (disabled with nothing staged); `A` amends `HEAD`
  with the pre-filled message plus whatever is staged; `w` rewords `HEAD`'s
  message only, leaving the index untouched. `Ctrl-S` commits, `Ctrl-O` /
  `Ctrl-N` toggle sign-off / no-verify (no-verify shown in red, never a
  silent skip), `Esc` cancels but keeps the draft for the next `c`. Every
  commit hook, GPG/SSH signing, and `commit.*` config setting applies,
  because it's a real `git commit` subprocess; a rejecting hook or any
  other failure shows its full output in a dismissible note instead of
  silently doing nothing.
- Files pane: changes can now be staged, unstaged, and discarded, not just
  viewed. `<space>` on a file row stages or unstages it (direction inferred
  from which side has a change); `a` does the same for every changed file at
  once. `Enter` or `l` on a file row focuses the diff itself — `j`/`k` move a
  cursor over its `+`/`-` lines (context is skipped), `]`/`[` jump hunks, `V`
  starts a line selection, and `<space>` there stages/unstages the hunk under
  the cursor or the selected lines, with `--recount` handling the rewritten
  hunk header. `h`/`Esc` returns to the file list. `d` discards a worktree
  change at the same three granularities (file, hunk, or selected lines),
  always after a one-line confirm in the keybar — nothing destructive
  happens without asking first. The panes refresh immediately after any of
  this, and the diff cursor keeps its place across that refresh — even one
  triggered by a change staged from another shell.
- Branches pane: each row now shows the tip commit's age (`5h`, `1d`, `3d`,
  ...) in its own colour, lazygit-style. Just selecting a branch (no key press)
  previews its own commit log in the right pane as spaced-out `git log`-style
  blocks (hash, author, date, summary), not a cramped one-liner — scrollable
  with J/K, PageUp/Down and the mouse wheel, with its own scrollbar when it
  overflows; pressing Enter drills that same pane into that log (title
  becomes `Commits (<branch>)`) instead of the generic HEAD-based one, and
  selecting a commit there shows its diff the same way the Commits pane
  always has. `Esc` backs out to the branch list.
- Status pane: the right side now shows a lazygit-style welcome screen (a
  `ferrit` wordmark, tagline, version, licence, and a keybindings pointer)
  instead of sitting blank on a real repo. Like lazygit, the wordmark grows
  with the terminal instead of staying one fixed size: three tiers, biggest
  that fits the right pane's width and height, falling back to a plain
  `ferrit` label rather than wrapping a wordmark into noise below all three.
  Identical in `App::mock()` and against a real repo — it's app chrome, not
  repo data.
- Files pane: changed paths below the repo root now group into a lazygit-
  style directory tree (a root `/` row, one header per directory, files
  shown by their own name once nested) instead of a flat list of full
  paths. Directories toggle collapsed/expanded with Enter or a left click
  on the row (lazygit's own click-to-toggle, not just a keybinding). Stays
  exactly the previous flat list — no root row, no headers — when every
  changed file is directly at the repo root, which is most working trees
  most of the time.
- Files pane: selecting a changed file now shows both sides at once,
  lazygit-style — an "Unstaged Changes" column beside a "Staged Changes"
  one — instead of a single diff that guessed which side to show. The left
  column narrows while this split is up so both stay readable. A file with
  changes on only one side just shows an empty diff on the other.
- Branches pane: `HEAD` can now move. `<space>` checks out the selected
  branch; `n` opens a popup for a new branch's name, always branched from
  the current `HEAD`; `d` deletes the selected branch after a one-line
  confirm, asking a second time (to force it) if it turns out to be
  unmerged, and refuses outright (no confirm) on the branch that is
  currently checked out; `u` fast-forwards the selected branch to its
  upstream whether or not it's the one checked out; `M` merges the
  selected branch into the current one, landing a merge commit, a
  fast-forward, or a conflicted state git itself would also leave — visible
  in the Files pane and a dismissible note, not silently pretended away.
  Every action shells out to real `git`, so hooks and git's own safety
  messaging (a dirty worktree a checkout would clobber, an unmerged
  delete's refusal) apply exactly as they would from a shell.
- ferrit can now talk to a remote: `f` fetches, `p` pulls (honouring
  whatever `pull.rebase`/`pull.ff` config is already set), and `P` pushes —
  offering to set an upstream via a small remote picker when the current
  branch has none. All three run on a background thread, so a slow or
  stalled network never freezes the keyboard; a "Fetching…"-style label
  shows while one is in flight, a short confirmation line once it's done,
  a failure with git's own message otherwise. The Branches pane gains a
  real Remotes tab (`Ctrl-Right`/`Ctrl-Left` to switch to it) listing every
  configured remote's fetch and push URLs.

### Changed

- Manual and automatic repository refreshes now read snapshots on a worker
  while the TUI stays responsive. Bursts coalesce into one follow-up refresh.
- Selected file diffs, commit diffs and branch logs load off-thread. Fast
  navigation keeps only one active read and latest pending selection; stale
  results cannot replace the current preview.
- Image blob reads and decoding, plus refreshes of drilled branch/commit data,
  now run off-thread too. Old image results cannot replace a newer selection.
- Commit and branch popups now share reusable `TextInput` and `Dialog`
  components; remote, note and help overlays reuse the same dialog shell.
- Panes, remote selection and key-hint rows now use shared `Panel`,
  `SelectList` and `KeyBar` components with existing styling preserved.
- Pane-list state/scroll rendering and preview scrollbars now use shared
  `PaneList` and `ScrollBar` components.
- Popup rendering is isolated in `src/ui/popups.rs`; shared dialogs support
  content-sized layouts and compose existing `TextInput`, `SelectList`, and
  `KeyBar` components.
- The event loop now handles bounded batches of queued input and worker events,
  avoiding a repaint for every key-repeat while preserving event order.
- Refresh restores selection by stable file, directory, branch, commit, or
  stash identity; stash rows now carry their object id for reliable matching.
- Background refresh, diff, image, and remote workers report panic failures
  through their normal completion events, releasing their in-flight state.
- Remote Git commands now have a five-minute deadline and stop their process
  group when Ferrit shuts down, keeping captured diagnostics on timeout or
  cancellation; typed worker kinds replace string labels.
- A failed filesystem watcher no longer prevents startup; Ferrit keeps polling
  and shows the watcher failure in the Status pane.
- Two-sided Files diff rendering now lives in `src/ui/diff.rs`, separate from
  the screen layout and shared overlays.
- Component and Git APIs now use explicit module paths; `clippy::pub_use` is
  denied crate-wide to prevent re-export shortcuts from returning.
- Every bordered box (left panes, right pane, command log, help overlay,
  image preview) now uses rounded corners (`╭╮╰╯`), matching lazygit's own
  look, instead of ratatui's square-corner default (`┌┐└┘`).

### Fixed

- Terminal initialization failures now unwind raw mode/alternate-screen changes;
  restoration always attempts to disable raw mode even if screen cleanup fails.
- Left column accordion: the focused pane now claims a weighted majority of
  the space (4 shares vs. 1 for each other pane) instead of a fixed floor
  each with 100% of the leftover to focus. The old scheme fell back to a
  perfectly even split — no accordion at all — whenever the terminal was too
  short for every pane's floor, which is exactly when a clear size
  difference matters most; the new one degrades gracefully at any height.
  Status is also sized to its actual line count (3 normally, 4 with a
  conflict to report) instead of a flat 4, freeing a row for the others.
- Branch recency and the branch-log preview's `Date:` line always floored to
  whole days, so anything committed earlier the same day showed a misleading
  `0d ago` instead of a real age. Both now step down to hours, minutes, or
  seconds once the elapsed time is under a day (`4h`, `12m`, `9s`).

## [0.2.0] - 2026-09-11

### Added

- Diff renderer now matches target lazygit/lazygitrs pager treatment: context
  lines keep syntax colours on plain background; `+`/`-` lines use flat
  add/delete colours with a full-width background; when available, delta now
  supplies the Patch layout, line-number gutter, word highlights, file
  markers, separators, and stat block.
- Styled diff `Text` now caches in `App`; scroll-only redraws reuse rendered
  spans. Cache invalidates on diff content, selection, focus anchor, or pane
  width changes.
- `docs/TESTS_STRATEGY.md`: maps lazygit's 551-file integration test suite
  onto our phase table as a `.script` behavior backlog, companion to
  `PLAN_SELF_TESTING.md`.
- Every left-column pane (Status, Files, Branches, Commits, Stash) now draws a
  vertical scrollbar when its list overflows the pane, matching the right
  pane's diff scrollbar; the thumb is green while the pane is focused and grey
  otherwise, following the pane's own border colour.
- Left column accordion: the focused pane among Files/Branches/Commits/Stash
  now grows to claim the leftover vertical space, the other three collapse to
  a 3-row floor (border + one row), lazygit-style. Status stays fixed height
  regardless of focus. Computed by hand rather than via `ratatui::Layout`,
  which mixes `Min`/`Fill` in an order-sensitive way at small heights.
- Diff and commit view: a lazygit-style `old new│` line-number gutter in front
  of every line, derived from the hunk header counters already parsed
  (`Diff::line_numbers`). Blank on headers, one-sided on an addition or
  deletion, both columns on context.
- Diff and commit view: a `git --shortstat` style summary line (`N file(s)
  changed, X insertion(s)(+), Y deletion(s)(-)`) above the scrollable diff,
  derived from `Diff::stat`. Rendered as its own row so it does not shift the
  line-index alignment scroll and hunk/file focus rely on.
- Diff and commit view: a `]` / `[` jump now boxes the whole hunk (or file, in
  a commit) in a dim background tint, not just its header line, so the
  boundary a jump landed on stays visible even after scrolling the header out
  of view.
- Diff and commit view: per-language syntax highlighting on every code line
  (`syntect`, syntax picked from the file's extension via `Diff::line_extensions`),
  plus a full-line pastel green/red background tint (`ADD_LINE_BG`/
  `DEL_LINE_BG`) on `+`/`-` lines so an addition or deletion still reads at a
  glance under the syntax colours. Metadata lines (headers, hunk markers,
  binary/no-newline notices) keep the flat `diff_line_style` colouring.
- Left click on the right pane focuses it (border lights up like a left
  pane's); `Esc` returns focus to the left column. Click still routes
  scrolling exactly as before.
- Left click on a left-pane row focuses that pane and moves its selection
  cursor to the clicked row (lazygit style), rebuilding the right pane the
  same way a `j` / `k` move would. Clicking a pane's border or title focuses
  it without moving the cursor; a click past the last row, on the command
  log, or in a gap does nothing; any click dismisses the help overlay.
  Right click, middle click, drag and mouse move stay inert for now.
- Strict Rust tooling, ported from the RUSTIFY reference setup. `rust-toolchain.toml`
  pins the compiler (1.97.1) so CI and every contributor lint identically.
  `rustfmt.toml` and `clippy.toml` fix formatting and the MSRV clippy target.
  `Cargo.toml` grows `[lints.clippy]`, `[lints.rust]` and `[lints.rustdoc]` tables:
  the clippy `all` / `pedantic` / `nursery` / `cargo` groups run at warn, a curated
  deny list breaks the build on `unwrap`, `panic`, `todo`, `indexing_slicing`,
  `print_stdout` / `print_stderr`, `exit`, undocumented `unsafe` and more, and
  `unsafe_code` is `forbid`. `deny.toml` adds a `cargo deny check` gate over
  advisories, licences, banned and duplicate deps, and the source allowlist.
- `.github/workflows/ci.yml`: runs `cargo fmt --check`, `cargo clippy --all-targets
  --all-features -- -D warnings`, `RUSTDOCFLAGS=-D warnings cargo doc`,
  `cargo machete`, `cargo deny check` and `cargo nextest run` on every push to
  `main` and every pull request.

### Fixed

- Right-pane diff scrollbar: the thumb now reaches the track's bottom at max
  scroll instead of stopping one cell short. `ScrollbarState`'s
  `content_length` is the count of distinct scroll positions
  (`total - viewport + 1`), not the raw line count, so a thumb sized against
  the raw total never spanned the full track.
- Both scrollbars drop their begin/end arrow glyphs (`.begin_symbol(None)`,
  `.end_symbol(None)`): a plain track + thumb, lazygit style, instead of
  arrows eating a row at each end.

### Changed

- `Repo::file_diff()` and `Repo::commit_diff()` take `DiffOpts` by value instead
  of by reference. `DiffOpts` is a 12 byte `Copy` struct, so the reference was
  pure overhead (`clippy::trivially_copy_pass_by_ref`).
- Lint fallout across `src/`: `map_or_else` instead of `map(..).unwrap_or_else(..)`,
  checked `usize` / `isize` arithmetic instead of `as` casts in the scroll paths,
  slice `.get()` instead of indexing, and small scoped `#[expect(..)]` where a
  lint flags a provably unreachable arm or a disproportionate dependency.

### Removed

- Unused direct dependency `crossterm`. Only the `ratatui::crossterm` re-export
  was ever used.

## [0.1.0] - 2026-09-08

### Added

- Right-pane scroll, lazygit style. The diff view now scrolls without leaving
  the focused left pane: `J` / `K` by a line, `PageUp` / `PageDown` by a page,
  `Ctrl-u` / `Ctrl-d` by a half page, `<` / `>` to the ends, `]` / `[` between
  hunks (or files, in a commit). Step sizes follow the real pane height. A
  vertical scrollbar shows on the right pane whenever the diff overflows, its
  thumb tracking the scroll position. The mouse wheel scrolls whichever pane
  the pointer is over (mouse capture is now enabled). The scroll keys and the
  wheel are inert over an image, a "no changes" note, and the mock bodies. New
  keys are listed in the keybar and the `?` help overlay.
- Real diff view in the right pane, lazygit style. Selecting a Files row runs
  `git diff` (or `git diff --cached` for a fully staged file, `git diff
  --no-index` for an untracked one); selecting a Commits row runs `git show`.
  Output is coloured from a byte-range parser (`src/git/diff/`): hunk headers
  cyan, additions green, deletions red, file and commit metadata bold. Vertical
  scroll with `Ctrl-d` / `Ctrl-u`, jump between hunks (or files, for a commit)
  with `]` / `[`. The viewport is kept across a background refresh of an
  unchanged selection and reset to the top when the selection moves. An image
  selection still owns the pane. `git config` (`diff.algorithm`, rename
  detection, and so on) is honoured because the diff is a subprocess.
- `Repo::file_diff()` and `Repo::commit_diff()` on the read-only git backend,
  plus `git::parse_diff()` for plain-text diff parsing without a subprocess.
- Live auto refresh, lazygit style. A background event multiplexer
  (`src/events.rs`) feeds the render loop from three sources: terminal input, a
  recursive filesystem watch on the worktree, and a 10 second poll fallback.
  Staging or committing from another shell, or an editor saving a file, now
  updates the panes on its own with no keypress. Filesystem bursts are
  debounced (150 ms) and `*.lock` plus `.git/objects/` churn is filtered so one
  stage triggers exactly one refresh. Manual `r` still works.
- `Repo::workdir()` accessor, exposing the worktree root that the watcher
  recurses from.

### Fixed

- Selection bar, lazygit style: the solid blue row highlight now shows only in
  the focused left pane. Unfocused panes keep their cursor position but draw no
  bar, so the three panes no longer look selected at once.
- Right pane with a real repo and nothing selected (fresh repo, no commits, no
  changes) showed `App::mock()`'s hardcoded sample diff instead of staying
  blank. `App::is_mock()` now gates that fallback to the repo-free path only.

[Unreleased]: https://github.com/ferrit-rs/ferrit/compare/v0.4.0...HEAD
[0.4.0]: https://github.com/ferrit-rs/ferrit/releases/tag/v0.4.0
[0.2.0]: https://github.com/ferrit-rs/ferrit/releases/tag/v0.2.0
[0.1.0]: https://github.com/ferrit-rs/ferrit/releases/tag/v0.1.0
