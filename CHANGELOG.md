# Changelog

All notable changes to ferrit are recorded here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and the project aims to follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

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

[Unreleased]: https://github.com/ferrit-rs/ferrit/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/ferrit-rs/ferrit/releases/tag/v0.1.0
