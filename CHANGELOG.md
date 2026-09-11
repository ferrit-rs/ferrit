# Changelog

All notable changes to ferrit are recorded here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and the project aims to follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

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
- Diff and commit view: lazygit-style word/char-level diff highlight. A `-`
  line immediately paired with its replacement `+` line (same position in a
  contiguous run of removals followed by additions) has its common
  prefix/suffix dimmed and only the actually-changed span drawn in full
  colour with a background tint (`Diff::word_diff_ranges`); an unpaired
  addition/deletion is unaffected. Supersedes an earlier `syntect`-based
  per-language syntax highlighting attempt, dropped in favour of this closer
  match to lazygit's own diff view (no dependency added).
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

[Unreleased]: https://github.com/ferrit-rs/ferrit/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/ferrit-rs/ferrit/releases/tag/v0.1.0
