# Plan: phase 18, painted themes (a real dark and a real light)

**Status: planned.** Written after phase 17 (the settings sheet) shipped a "Theme:
Dark / Light" row that does not paint anything. lazygit has no such setting (it only
sets foreground colours and leaves the terminal's background alone), so this phase is
not compared with it; it is checked with buffer tests and screenshots
(`PLAN_SELF_TESTING.md`).

## Goal

Choosing **Dark** makes ferrit's whole screen dark, and choosing **Light** makes it
light, whatever the terminal's own background is. Today `theme.base` only retunes a
few colours for a terminal that is already dark or light; it leaves the background
to the terminal. That is not what "dark theme / light theme" means to a user.

```
Today, Light on a dark terminal         This phase, Light
┌ terminal, dark ──────────────┐        ┌ terminal, dark ──────────────┐
│ black background (terminal)  │        │ WHITE background (ferrit)    │
│ grey / white text (ANSI)     │        │ dark text, dark-on-light     │
│ pastel diff tints on black   │        │ pastel diff tints on white   │
└──────────────────────────────┘        └──────────────────────────────┘
          (nothing looks light)                 (the whole screen is light)
```

A theme here is a named combination of colours: a background, a text colour, the
sixteen colours ferrit's palette uses (green, red, grey…) and the diff tints. The
accent colour stays the user's own pick on top of any of them.

## What is out of scope, and why

- **Fonts.** Ferrit runs inside a terminal; the terminal chooses the font family and
  size, and a program drawing text cells cannot change them. The only text styles
  ferrit controls are bold, italic, underline and dim. A theme is therefore colours
  only. The font stays a setting of the user's terminal.
- **Asking the terminal to change its own background** (the OSC 11 escape). Not all
  terminals honour it, and a crash would leave the terminal's background changed.
  Ferrit paints its own cells instead, which works everywhere and cleans up by itself.
- **Themes the user writes** (a theme file format). The data is shaped so that more
  themes can be added later; this phase ships three choices and no file format.
- **The terminal's window padding.** The margin some terminals keep around the cell
  grid is the terminal's and keeps its colour; ferrit has no cell there.

## The three choices

| Choice | What it does | Config |
| --- | --- | --- |
| **Terminal** (default, today's behaviour) | ferrit paints no background and uses the terminal's own colours; `base` tells it whether the terminal is dark or light, as now | `theme.scheme = "terminal"` (or absent) |
| **Dark** | ferrit paints a dark background and its own dark colours | `theme.scheme = "dark"` |
| **Light** | ferrit paints a light background and its own light colours | `theme.scheme = "light"` |

No existing config changes meaning: a file without `scheme` is `terminal`, and
`base = "light"` keeps tuning the terminal case exactly as before. When `scheme` is
`dark` or `light`, `base` is ignored (the scheme says it).

The settings sheet:

```
┌ Settings ────────────────────────────────────────────┐
│ Appearance                                           │
│ ▸ Theme            ( ) Terminal  (•) Dark  ( ) Light │
│   Terminal is      (•) Dark  ( ) Light     ← only    │
│                                              shown   │
│   Accent           ● Green  ○ Blue  ○ Purple  ○ Amber│    when Theme
│                    (the colour picker, as today)     │    is Terminal
│ …                                                    │
└──────────────────────────────────────────────────────┘
```

"Terminal is" is the old `base` row, kept for the Terminal choice only (it decides the
diff tints and the syntax theme when ferrit does not paint). It disappears when the
theme is Dark or Light, since the scheme already says it.

## How it is painted

Ferrit draws with about 180 colour sites across 16 files (`Color::Green`,
`Color::Gray`, and many `Style::new()` with no colour at all, which means "the
terminal's default"). Editing every site is the slow, fragile way and would break the
next widget someone adds. Instead:

**One paint pass, at the end of `screens::draw`, over the frame's buffer.** For a
painted scheme, for every cell:

- a background of `Reset` (nothing set) becomes the scheme's background;
- a foreground of `Reset` becomes the scheme's text colour;
- a named ANSI colour (`Black` … `White`, `Gray`, `DarkGray`, the `Light*` ones)
  becomes the scheme's RGB value for that name;
- an `Rgb` or `Indexed` colour is left alone (the diff tints, the syntax colours,
  the accent, the colour picker's swatches).

```
widgets draw as today ─▶ buffer ─▶ paint pass (scheme) ─▶ terminal
 (ANSI names, Reset)                 Reset  → bg / text
                                      Green  → scheme.green
                                      Rgb    → untouched
```

Why a pass and not the palette: it also catches what `Clear` resets (popups, the
drawer, the toast: `Clear` wipes a cell back to `Reset`, and a background set before
it would be lost), and any colour a future widget uses. In the `terminal` choice the
pass does nothing, so today's rendering is untouched. A frame is about 6,000 cells, so
the pass is not measurable next to the draw itself (to be measured, see tests).

### The schemes

Starting values, tuned in P2 against the contrast test below. Each scheme is one
`const` struct, so a fourth is one more struct.

| Slot | Dark | Light |
| --- | --- | --- |
| background | `#101216` | `#ffffff` |
| text | `#d7dae0` | `#1f2328` |
| green / red / yellow | `#3fb950` / `#f85149` / `#d29922` | `#1a7f37` / `#cf222e` / `#9a6700` |
| blue / magenta / cyan | `#58a6ff` / `#bc8cff` / `#39c5cf` | `#0969da` / `#8250df` / `#1b7c83` |
| gray (secondary text) | `#8b949e` | `#656d76` |
| dark gray (boxes, borders) | `#484f58` | `#8c959f` |
| selection bar | `#1f4e8c` with white text | `#0969da` with white text |
| diff add / delete tint | `rgb(20,45,20)` / `rgb(55,20,20)` (as now) | `rgb(214,245,214)` / `rgb(250,214,214)` (as now) |

The syntax colours in the diff already have a dark and a light syntect theme
(`base16-ocean.dark`, `InspiredGitHub`); a painted scheme picks the one that matches
its brightness.

### Terminals without 24-bit colour

The scheme is RGB. A terminal that does not do 24-bit colour shows it wrong (macOS
`Terminal.app` is 256 colours only; tmux needs `Tc`). Ferrit already uses a few `Rgb`
colours, but a painted background makes a wrong colour impossible to miss.

- Ferrit reads `COLORTERM` (`truecolor` or `24bit`).
- With 24-bit colour, the pass writes `Rgb`.
- Without it, the pass writes the nearest xterm-256 `Indexed` colour for every colour
  it paints, and for the `Rgb` ones it leaves (the nearest of the 256, computed once
  per frame cell value). The scheme still reads as dark or light.
- The settings sheet footer says when it fell back ("256 colours: approximated").

This is the part most likely to need a real terminal to judge; see Self-testing.

## What changes in what exists

- `theme_config.rs`: `Scheme { Terminal, Dark, Light }`, `theme.scheme` (serde,
  default `Terminal`); `ThemeConfig::palette()` for a painted scheme returns the dark
  or light `Palette` (the `light` flag follows the scheme, so the diff and syntax
  choices follow it).
- `components/ui/scheme.rs` (new): the two `const` schemes and the paint pass
  (`paint(buffer, &Scheme, truecolor)`); no knowledge of ferrit's screens, like the
  other components.
- `screens/mod.rs`: one call to the pass after everything is drawn, before the frame
  ends. The toast and the drawer draw before it, so they are covered.
- `settings.rs` / `screens/settings.rs`: the Theme row becomes three choices; the
  "Terminal is" row appears under it only for `Terminal`; both are saved to
  `theme.scheme` / `theme.base` through `save_sections` as today.
- `Backdrop` (drawer and popups dim the screen with `Black` and `DarkGray`): over a
  painted scheme the dimming must go toward the scheme's background, not toward black
  (a black wash over a white screen is a blackout). The pass handles it: it maps the
  backdrop's named colours like any other, and P2 checks the result by eye.

## Edge cases

| Case | Behaviour |
| --- | --- |
| `scheme = "dark"` in a light terminal | the whole screen is dark; the terminal's padding stays light |
| `scheme = "light"` and `base = "dark"` | `base` is ignored |
| a config with no `scheme` | `terminal`: nothing changes |
| `scheme = "sepia"` (unknown) | reported at startup like any bad value, `terminal` used |
| `[theme.colors]` overrides (phase 12) | still apply, and win over the scheme's table for the slot they name |
| an image preview | drawn over the cell grid as today; transparent PNG shows the painted background |
| the accent is a named preset | the pass maps `Green` etc. like any named colour, so the accent follows the scheme; a picked `Rgb` is kept exactly |
| the colour picker's own swatches | `Rgb` swatches are untouched; named ones show the scheme's value, and the picked colour is saved as chosen |
| 256-colour terminal | indexed approximation, said in the footer |
| a very small terminal | the pass is a loop over the buffer's cells; nothing to overflow |

## Self-testing (see `PLAN_SELF_TESTING.md`)

- `tests/scheme_paint.rs`: after the pass on a rendered frame (the panes, a popup, the
  drawer, the toast, the dashboard, the welcome screen) in `Dark` and in `Light`, no
  cell has a `Reset` background or foreground and none has a named ANSI colour; in
  `Terminal` the buffer is byte-identical to before the pass (nothing changes for
  today's users). A `Clear` in the middle does not leave a hole.
- A **contrast test**: for each scheme, every text colour on the background it is used
  on reaches a contrast ratio of 4.5 (3.0 for the dim grey and borders), computed from
  the table. The table is tuned until it passes; a new colour cannot make a theme
  unreadable without failing here.
- The truecolor fallback: with `COLORTERM` absent the pass writes `Indexed` only, and
  the nearest-colour function is checked on known values; with it present, `Rgb`.
  (The environment is read in the binary and passed in; the library never sets it,
  `unsafe_code` is forbidden.)
- `tests/config.rs`: `scheme` parses, defaults, an unknown value is reported, a file
  without it loads as `terminal`, saving the theme keeps the other sections.
- `tests/app_settings.rs` and `tests/settings_screen.rs`: the Theme row has three
  choices and each is saved; "Terminal is" shows only for `Terminal`; the footer note
  for 256 colours; the palette follows the choice at once.
- A measurement, not an assertion: the cost of the pass on a 200x60 frame, in a debug
  build, written in this plan when done.
- Screenshots through the tmux harness (`__SOP/visual-verify.md`): the panes, a diff,
  a popup and the settings sheet in each scheme, in a truecolor terminal and in
  `Terminal.app`. The TestBackend proves the cells; only a real terminal shows how the
  colours look and whether the fallback is acceptable.
- `test/scripts/200-themes.script`: from the settings sheet, pick Dark, then Light,
  then Terminal; `reopen`; the sheet shows the last choice (the replay's frames are
  text, so the colours themselves are the buffer tests' job).

## Milestones

- **P0** `Scheme`, `theme.scheme` config and its tests; the two scheme structs and
  the paint pass with its buffer tests; wired into `screens::draw`. No UI yet: the
  config key alone switches the theme.
- **P1** the settings sheet: the three-choice Theme row, the "Terminal is" row, live
  and saved; sheet tests; the replay script.
- **P2** the 24-bit fallback (`COLORTERM`, `Indexed`) with its tests and the footer
  note; the contrast test and the table tuned to pass; the backdrop checked.
- **P3** the screenshots in both terminals, the measured cost, anything the eye finds
  that no test saw, then README, CHANGELOG and plans.

## Definition of done (phase 18)

In the settings sheet choose Light: the whole of ferrit turns light (panes, diff,
popups, the sheet, the toast, the dashboard), on a dark terminal as on a light one;
choose Dark and it is dark; choose Terminal and ferrit looks as it did before this
phase. Close ferrit and open it again: the choice is kept. The text is readable in
every scheme (the contrast test passes) and the 256-colour fallback still reads as
dark or light. `cargo clippy --all-targets --all-features -- -D warnings` and
`cargo test` are green, the replay passes, and the README table says so.

## Questions for the user before P0

- Is "Terminal" worth keeping as a third choice? Recommended yes: it costs nothing
  (the pass does nothing for it) and nobody with a well-tuned terminal theme is
  forced to change.
- Should **Dark** or **Light** be the default for a new install, or stay `Terminal`?
  Recommended `Terminal`: a first start must not change what an existing user sees.
