# 3. Ferrit paints its whole screen

Status: accepted (`docs/PLAN_18_THEMES.md`)

## Context

A "Dark" or "Light" theme that only changes foreground colours is not a theme: on a
terminal with the other background nothing looks right. Widgets also draw with ANSI
names and with no colour at all (`Reset`), about 180 colour sites in 16 files.

## Decision

One paint pass at the end of `screens::draw_painted`, over the frame buffer: `Reset`
background becomes the scheme background, `Reset` foreground the scheme text, named
ANSI colours the scheme's RGB; `Rgb` and `Indexed` colours are left alone. A scheme is
one `const` struct. `Terminal` skips the pass and keeps the terminal's colours.

Fonts and the terminal's own background (OSC 11) are out of scope.

## Consequences

- A new widget is themed without touching it, and `Clear` (popups, drawer, toast) is
  covered because the pass runs after it.
- Tests assert on `draw`, which keeps the colours widgets chose; only the run loop
  calls `draw_painted`.
- A colour-depth fallback (256 colours) is a second mapping stage, with a contrast
  test over both schemes.

## Alternatives considered

- **Edit every colour site to read the palette.** Slow, fragile, and the next widget
  breaks it.
- **Ask the terminal to change its background (OSC 11).** Not honoured everywhere and
  can leave the terminal changed after a crash.
