# Bug: image preview blank or low-resolution, terminal-dependent

## Symptom

Selecting an image in the Files pane either showed an empty right pane or a
coarse, blocky picture, and which one depended entirely on the terminal:

- VS Code integrated terminal: blank pane.
- iTerm2: blank pane.
- zellij: visible but chunky (half-blocks).
- Terminal.app / kitty / WezTerm: fine.

## Root cause

`ratatui-image`'s `Picker::from_query_stdio()` asks the terminal which
graphics protocol it supports by writing an escape sequence and reading the
reply. Several hosts answer with a protocol they do not actually draw:

- **VS Code integrated terminal** answers the iTerm2 capability query but
  never draws iTerm2 inline images. Confirmed still true on 1.135. The picker
  believes it has a working backend; every frame is silently dropped.
- **iTerm2 (>= 3.5)** answers the kitty graphics query, so
  `from_query_stdio()` returns `Kitty`. iTerm2 only implements part of the
  kitty protocol, and `ratatui-image`'s kitty encoder relies on the unicode
  placeholder part that iTerm2 does not render, so the pane comes out blank.
  iTerm2's own inline-image protocol works fine.
- **zellij** swallows graphics escape codes in its terminal-emulation layer
  and has no passthrough (unlike tmux, which `from_query_stdio` handles via
  `is_tmux` DCS wrapping), so no native protocol reaches the outer terminal.

A native protocol also matters for sharpness, not just for showing anything:
half-blocks render at the character-grid resolution, so a small preview pane
turns blocky. kitty / iTerm2 / sixel draw at the cell's pixel size and stay
crisp even in a small pane.

## Fix

`src/image/detect.rs`, `detect_picker()`:

1. `FERRIT_NO_GRAPHICS` set: return `None`, caller keeps `Picker::halfblocks()`.
2. `pinned_protocol()`: on a host where the query is misleading, skip it (also
   avoids a 2s stdio timeout), take a plain `Picker::halfblocks()`, and pin
   the protocol the host really draws:
   - `ZELLIJ` set: `Sixel`. zellij (>= 0.40) composites sixel itself but has
     no passthrough for kitty / iTerm2.
   - `TERM_PROGRAM == "vscode"`: `Sixel`. The integrated terminal draws sixel
     once `terminal.integrated.enableImages` is on (shipped in
     `.vscode/settings.json`), but answers the query as iTerm2 and then drops
     every frame.
   `FERRIT_FORCE_GRAPHICS` disables this skip and runs the query anyway.
3. Otherwise `Picker::from_query_stdio()`, then:
   - iTerm2 (`TERM_PROGRAM == "iTerm.app"` or `LC_TERMINAL == "iTerm2"`, the
     latter surviving ssh / tmux) that came back as `Kitty` is forced to
     `Iterm2`. iTerm2 >= 3.5 answers the kitty query but only half-implements
     it and `ratatui-image`'s kitty encoder needs the placeholder part.
4. `FERRIT_GRAPHICS=<halfblocks|sixel|kitty|iterm2>` forces that protocol
   last, overriding every rule above. Use it when a picked protocol still
   renders blank (old zellij with no sixel, VS Code with the setting off).

`FERRIT_DEBUG` set: `detect::debug_line()` returns a one-liner
(`graphics: Sixel  font 7x15  ...`) that `App::detect_graphics` stashes in
`last_error` so the chosen protocol and detected font size show in the
Status pane.

## Result

| Terminal                     | Protocol       | How            |
|------------------------------|----------------|----------------|
| iTerm2                       | Iterm2         | query + fixup  |
| kitty / Ghostty / WezTerm    | Kitty          | query          |
| Terminal.app                 | Sixel          | query          |
| VS Code integrated           | Sixel          | pinned         |
| zellij                       | Sixel          | pinned         |

`cargo run` is crisp in all five with no env vars, given a recent zellij and
`terminal.integrated.enableImages: true` under VS Code (the tracked
`.vscode/settings.json` sets it).

## Still not fixed

- zellij older than 0.40 (no sixel) renders blank; use
  `FERRIT_GRAPHICS=halfblocks` there.
- VS Code with `terminal.integrated.enableImages` turned off renders blank;
  same fallback, or turn the setting back on.

## Two unrelated fixes bundled in the original pass

While first chasing the blank pane, two regressions from the
`StatefulProtocol` rewrite were fixed alongside it:

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
