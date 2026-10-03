# Plan: phase 18, painted themes (a real dark and a real light)

**Status: in progress (P0 to P3 done: the paint pass, the Theme row, the 256-colour fallback, and the check in a real terminal).** Written after phase 17 (the settings sheet) shipped a "Theme:
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
  themes can be added later; this phase ships two themes and no file format.
- **The terminal's window padding.** The margin some terminals keep around the cell
  grid is the terminal's and keeps its colour; ferrit has no cell there.

## Terminal is back (after P3)

The user asked for the Terminal theme back once it was clear what it is (the
terminal's own colours, as ferrit always did). The three choices below are the ones
that ship; everything that follows this section about "no Terminal" is the history of
P1b, kept for what it explains. What changes against the first version of this plan:

- `theme.scheme` is `Option<SchemeChoice>` (`terminal`, `dark`, `light`). **Absent, it
  is taken from `base`**, painted (`base = "light"` is Light, nothing is Dark), because
  the files written between P1b and now have only `base`. A default config writes no
  `scheme`; the sheet writes one as soon as a theme is chosen.
- `base` is the painted theme when there is no `scheme`, and nothing else: **there is
  no "Terminal is" row** (the user: "we do not need a terminal dark or light, only
  terminal, dark or light"). Under Terminal the palette is the dark one (the diff
  tints and the syntax theme); a light terminal picks Light. A painted scheme makes
  `base` irrelevant.
- **The default is still Dark, painted**, not Terminal: Terminal is a choice, not the
  fallback.
- The sheet: the Theme row is `Terminal / Dark / Light`, and that is all.
- `screens::draw_painted` paints only when the effective scheme is not Terminal.

## The two themes (as decided at P1b; Terminal came back after P3)

There are two themes and no "follow the terminal" option (dropped on the user's
decision after P1: a theme that paints nothing is not a theme). Every start is
painted: ferrit is dark unless the config says light.

| Theme | What it does | Config |
| --- | --- | --- |
| **Dark** (default) | ferrit paints a dark background and its own dark colours | `theme.base = "dark"` (or absent) |
| **Light** | ferrit paints a light background and its own light colours | `theme.base = "light"` |

The key is the existing `theme.base` (phase 12), so no config changes meaning:
`base = "light"` was already "I want the light colours" and is now also the light
background. Because of that, an existing user with a dark terminal and no `base`
sees ferrit's dark background instead of the terminal's: this is the one change in
look for existing users, and is the point of the phase. Any other value (the dropped
`"terminal"`, a typo) is reported at startup like a bad value, and dark is used.

The settings sheet keeps its `Theme` row, now painted for real:

```
┌ Settings ────────────────────────────────────────────┐
│ Appearance                                           │
│ ▸ Theme            (•) Dark  ( ) Light               │
│   Accent           ● Green  ○ Blue  ○ Purple  ○ Amber│
│                    (the colour picker, as today)     │
│ …                                                    │
└──────────────────────────────────────────────────────┘
```

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
it would be lost), and any colour a future widget uses. The pass is the *output*
stage: `screens::draw_painted` is `draw` plus the pass, and only the run loop calls
it; `screens::draw` keeps the colours the widgets chose (the palette's `Red`, `Blue`),
which is what most tests assert. A frame is about 6,000 cells, so the pass is not
measurable next to the draw itself (to be measured, see tests).

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

- Ferrit reads `COLORTERM` (`truecolor` or `24bit`), in the binary (`main`) only: the
  library never reads the environment, and `App::set_truecolor` hands it the answer.
- With 24-bit colour, the pass writes `Rgb`.
- Without it, the pass writes the nearest xterm-256 `Indexed` colour for every colour
  it paints, and for the `Rgb` ones it leaves (the nearest of the 256, computed once
  per frame cell value). The scheme still reads as dark or light.
- The settings sheet footer says when it fell back ("256 colours: approximated").

This is the part most likely to need a real terminal to judge; see Self-testing.

## What changes in what exists

- `theme_config.rs`: `ThemeConfig::scheme()` gives the painted scheme for `base`;
  `palette()` is unchanged (the `light` flag follows `base`, so the diff and syntax
  choices follow it).
- `components/ui/scheme.rs` (new): the two `const` schemes and the paint pass
  (`Scheme::paint`, later with the truecolor choice); no knowledge of ferrit's
  screens, like the other components.
- `screens/mod.rs`: `draw_painted` (draw, then the pass); the run loop draws with it.
  The toast and the drawer draw before the pass, so they are covered.
- `settings.rs` / `screens/settings.rs`: the Theme row is the existing one, saved to
  `theme.base` as before; nothing new in the sheet.
- `Backdrop` (drawer and popups dim the screen with `Black` and `DarkGray`): over a
  painted scheme the dimming must go toward the scheme's background, not toward black
  (a black wash over a white screen is a blackout). The pass handles it: it maps the
  backdrop's named colours like any other, and P2 checks the result by eye.

## Edge cases

| Case | Behaviour |
| --- | --- |
| Dark in a light terminal, or Light in a dark one | the whole screen is the chosen theme; the terminal's padding keeps its own colour |
| a config with no `base` | dark, painted |
| `base = "terminal"` or any unknown value | reported at startup like any bad value, dark used |
| `[theme.colors]` overrides (phase 12) | still apply, and win over the scheme's table for the slot they name |
| an image preview | drawn over the cell grid as today; transparent PNG shows the painted background |
| the accent is a named preset | the pass maps `Green` etc. like any named colour, so the accent follows the scheme; a picked `Rgb` is kept exactly |
| the colour picker's own swatches | `Rgb` swatches are untouched; named ones show the scheme's value, and the picked colour is saved as chosen |
| 256-colour terminal | indexed approximation, said in the footer |
| a very small terminal | the pass is a loop over the buffer's cells; nothing to overflow |

## Self-testing (see `PLAN_SELF_TESTING.md`)

- `tests/scheme_paint.rs`: after the pass on a rendered frame (the panes, the help, a
  popup, the drawer, the toast, the dashboard, the git config screen) in `Dark` and in
  `Light`, no cell has a `Reset` background or foreground and none has a named ANSI
  colour; with no config at all the frame is already dark and painted. A `Clear` in
  the middle does not leave a hole.
- A **contrast test**: for each scheme, every text colour on the background it is used
  on reaches a contrast ratio of 4.5 (3.0 for the dim grey and borders), computed from
  the table. The table is tuned until it passes; a new colour cannot make a theme
  unreadable without failing here.
- The truecolor fallback: with `COLORTERM` absent the pass writes `Indexed` only, and
  the nearest-colour function is checked on known values; with it present, `Rgb`.
  (The environment is read in the binary and passed in; the library never sets it,
  `unsafe_code` is forbidden.)
- `tests/config.rs`, `tests/scheme_paint.rs`: `base` parses, defaults to dark, an
  unknown value (`"terminal"` included) is reported, saving the theme keeps the other
  sections.
- `tests/app_settings.rs` and `tests/settings_screen.rs`: the Theme row has two
  choices and each is saved; the screen repaints at once; the footer note for 256
  colours (P2).
- A measurement, not an assertion: the cost of the pass on a 200x60 frame, in a debug
  build, written in this plan when done.
- Screenshots through the tmux harness (`__SOP/visual-verify.md`): the panes, a diff,
  a popup and the settings sheet in each scheme, in a truecolor terminal and in
  `Terminal.app`. The TestBackend proves the cells; only a real terminal shows how the
  colours look and whether the fallback is acceptable.
- `test/scripts/200-themes.script`: from the settings sheet, pick Light (arrow and
  click) and Dark; `reopen`; the sheet shows the last choice (the replay's frames are
  text, so the colours themselves are the buffer tests' job).

## Milestones

- **P0** ✅ the paint pass and its schemes, with buffer tests. (It first came with a
  `theme.scheme` key and a "Terminal" choice; both were dropped after P1.)
- **P1** ✅ the Theme row of the sheet, live and saved; sheet tests; the replay scripts.
- **P1b** ✅ "Terminal" removed: `theme.base` is the theme, `scheme` and "Terminal is"
  are gone, `draw_painted` is the output stage.
- **P2** ✅ the 24-bit fallback (`COLORTERM`, `Indexed`) with its tests and the footer
  note; the contrast test and the table tuned to pass; the backdrop checked.
- **P3** ✅ the screenshots in both terminals, the measured cost, anything the eye finds
  that no test saw, then README, CHANGELOG and plans.

## What P3 found (Terminal.app through tmux, 200x50, the `canonical` fixture)

Checked by eye on screenshots of the panes, the commit popup and the settings sheet,
in Dark and in Light, in a 256-colour terminal (Terminal.app sets no `COLORTERM`, so
this is the fallback path). Not checked: a 24-bit terminal (iTerm2 is installed, but
the screenshot harness drives Terminal.app only), so the `Rgb` path is proven by the
buffer tests and not by eye.

- Light on a dark Terminal.app is white everywhere ferrit draws: panes, diff with its
  pastel tints, popups, the drawer, and the dimmed screen behind a popup (a pale grey,
  not black). Dark is dark everywhere. Both read well after the 256-colour
  approximation.
- Found and fixed: text on a named fill (the selected row's blue bar) was dark on blue
  in Light, unreadable. The pass now keeps any text on a named fill at a contrast of 3
  or more, switching to white or the scheme's text colour when it would not be.
- Found and fixed: the accent presets that are not chosen were drawn dim, which on a
  white screen is nearly invisible; the chosen one is bold instead.
- Not changed: the settings footer shows the whole config path, which a long path cuts
  at the right edge; it is cosmetic and was already so.
- Cost of the pass on a 200x60 frame (the clone of the buffer taken off): 24-bit
  about 0.36 ms in a release build (2.5 ms debug); 256-colour about 0.46 ms release
  (9.8 ms debug, where the nearest-colour search is slow). A frame is not drawn more
  than a few times a second, so it does not matter in release; the debug figure is
  what a `cargo run` build pays.

## Definition of done (phase 18)

In the settings sheet choose Light: the whole of ferrit turns light (panes, diff,
popups, the sheet, the toast, the dashboard), on a dark terminal as on a light one;
choose Dark and it is dark. Close ferrit and open it again: the choice is kept. The
text is readable in both (the contrast test passes) and the 256-colour fallback still
reads as dark or light. `cargo clippy --all-targets --all-features -- -D warnings`
and `cargo test` are green, the replay passes, and the README table says so.

## Decisions taken

- "Terminal" (follow the terminal) is not offered. Decided by the user after P1.
- Dark is the default for a new install, and for an existing one with no `base`.
