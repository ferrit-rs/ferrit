# Changelog

All notable changes to ferrit are recorded here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and the project aims to follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Live auto refresh, lazygit style. A background event multiplexer
  (`src/events.rs`) feeds the render loop from three sources: terminal input, a
  recursive filesystem watch on the worktree, and a 10 second poll fallback.
  Staging or committing from another shell, or an editor saving a file, now
  updates the panes on its own with no keypress. Filesystem bursts are
  debounced (150 ms) and `*.lock` plus `.git/objects/` churn is filtered so one
  stage triggers exactly one refresh. Manual `r` still works.
- `Repo::workdir()` accessor, exposing the worktree root that the watcher
  recurses from.
- Official ratatui logo SVGs vendored under `assets/logos/` (`ratatui-logo.svg`
  and the monochrome `ratatui-logo-simple.svg`), pulled from the upstream
  `ratatui/ratatui` repo.

### Fixed

- Selection bar, lazygit style: the solid blue row highlight now shows only in
  the focused left pane. Unfocused panes keep their cursor position but draw no
  bar, so the three panes no longer look selected at once.
