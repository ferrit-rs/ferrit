# Bug: image preview rendered blank in VS Code's integrated terminal

## Symptom

Selecting an image in the Files pane showed an empty right pane. Worked in a
real Terminal.app window, not inside VS Code's built-in terminal panel.

## Root cause

`ratatui-image`'s `Picker::from_query_stdio()` asks the terminal which
graphics protocol it supports by writing an escape sequence and reading the
reply. VS Code's integrated terminal answers the iTerm2 capability query, so
`from_query_stdio()` returns `Iterm2` and reports success, but VS Code never
actually draws iTerm2 inline images. The picker believed it had a working
graphics backend; the terminal silently dropped every frame.

Real terminals (Terminal.app, iTerm2, Kitty, WezTerm) answer the same query
honestly, so this only reproduces under `TERM_PROGRAM=vscode`.

## Fix

Skip the query entirely under VS Code and keep `Picker::halfblocks()`, the
always-works fallback. Half-blocks render as unicode blocks instead of a
native image, lower resolution but visible everywhere.

Code: `src/image/detect.rs`, `detect_picker()` returns `None` (caller keeps
its half-block picker) when `TERM_PROGRAM == "vscode"`, otherwise forwards to
`Picker::from_query_stdio()`.

## Related, not fixed

Zellij (terminal multiplexer) also swallows graphics escape codes in its own
terminal emulation layer. `ratatui-image` has explicit tmux passthrough
support (`is_tmux`, DCS wrapping) but no zellij equivalent, so the same blank
pane happens there. No code fix yet. Same shape of fix would apply: detect
`ZELLIJ` env var, force half-blocks. Deferred.

## Two unrelated fixes bundled in the same pass

While chasing the blank pane, two regressions from the `StatefulProtocol`
rewrite got fixed alongside it:

- **`&mut App` render signature**: switching the preview from a `RefCell`-
  wrapped protocol to a live `StatefulProtocol` (matching the ratatui-image
  examples) means `StatefulImage` mutates the protocol at render time, so
  `ui::draw` now takes `&mut App` instead of `&App`. `tests/render.rs` and
  `examples/dump_frame.rs` still called it with `&app`; updated both.
- **Preview pane title**: the image branch in `ui.rs` was using
  `right_title` (the pane-focus title, e.g. "Unstaged changes") instead of
  the literal `" Preview "` title. Reverted to the hardcoded title so the
  pane header stays "Preview" while an image is shown, independent of which
  pane last had focus.
