# Plan: phase 3, diff view

## Goal

Right pane's diff/commit view, lazygit/lazygitrs pager look: syntax colour and
full-line add/delete background are mutually exclusive, never both on same
line, with full-width changed-line bars and cached rendering.

## Target rendering

Context lines keep per-token syntax colour on plain background. A `+`/`-` line
gets full-width background with flat/plain text, no per-token colour:

```
  444:450    self.left_areas[pane] = area;                 <- context: syntax colour, no bg
  445:451    }
 ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓
 ▓   :453    pub fn right_focused(&self) -> bool {         <- changed: full bg, flat text
 ▓   :456        self.right_focused
 ▓   :457    }
 ▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓▓
  449:461    pub fn list_offset(&self, pane: Pane) -> usize <- back to syntax colour
```

`old:new│` gutter, shortstat summary line, and boxed hunk/file focus
highlight (from earlier work) are unaffected by this change.

## In scope

- `src/theme.rs`, `render_diff`: a `+`/`-` line stops calling
  `HighlightLines` and instead gets flat `ADD`/`DEL` foreground plus the
  full-width `ADD_LINE_BG`/`DEL_LINE_BG` background. Only context lines stay
  `syntect`-tokenized. `is_code_line` still gates syntax colour (headers,
  hunk markers, binary/no-newline notices stay flat).
- Render performance: `render_diff` currently re-tokenizes every code line of
  the *entire* diff on every redraw (any keypress, tick, resize), not just
  the visible viewport, with no cache. Cache the built `Text` (in `App`,
  alongside `DiffView`), keyed on `right_key`, diff `text`, focus range, and
  pane width. Rebuild only when one changes; pure scroll must not rebuild.

## Out of scope

- Staging / unstaging (phase 6).
- External diff renderer config (`delta`, `difftastic`) as a user-facing
  option; the `DiffCmd` builder already leaves room for it.
- Side-by-side layout, combined merge-commit diff, horizontal scroll of
  un-wrapped lines.
- Branches' "Log" body, a stash entry's diff, Status's right side: still mock.

## Definition of done

- `+`/`-` lines: flat colour + full-line background, no per-token syntax
  colour. Context lines: syntax colour, no background.
- Scrolling a large diff does not visibly lag; `render_diff` only reruns on a
  diff-content or focus change, not on every redraw.
- `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test`,
  `cargo machete` clean.
- `CHANGELOG.md` updated.
