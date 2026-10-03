# Plan: phase 19, the dashboard as a sheet

**Status: done (S0 to S3).** Written after the settings sheet (phase 17): the dashboard
(phase 13) is a full-screen view, and the user wants to keep the panes in sight
while it is up. lazygit has no dashboard, so this is not compared with it; it is
checked with frame tests, the replay harness and screenshots
(`PLAN_SELF_TESTING.md`).

## Goal

`D` opens the dashboard as a drawer that slides in from the right, over a dimmed
copy of the panes, exactly like the settings sheet, instead of replacing the whole
screen. The content (totals, activity chart, kinds, heat map, contributors, hot
files, branches) and the way it is computed (off the UI thread, quick pass then full
pass) do not change.

```
Today (D, full screen)                  This phase (D, a sheet)
┌──────────────────────────────┐        ┌───────────┬──────────────────────┐
│ Dashboard                    │        │ panes     │ Dashboard            │
│ the panes are not visible    │        │ (dimmed,  │ totals · activity    │
│                              │        │  behind)  │ contributors · …     │
│ Back: esc  Window: t  …      │        │           │                      │
└──────────────────────────────┘        ├───────────┴──────────────────────┤
                                        │ Back: esc  Window: t  Counts: n  │
                                        └──────────────────────────────────┘
```

## What stays, what changes

- **Stays**: `dashboard::draw(frame, area, &view)` already draws into any rectangle,
  so the page itself is untouched; `Dashboard` (state, cache, worker, generations),
  the keys `t` `T` `r` `n`, `j` `k` `PgUp` `PgDn` `Home` `End`, `?`, and `D` to close.
- **Changes**: it stops being a `FullScreen` state and becomes a drawer, like the
  settings sheet: the same `Drawer` shell (slide-in, dimmed backdrop, `Esc` or a click
  outside closes), and the key bar below the drawer shows the dashboard's own keys
  (`Bar::Dashboard`, as in full screen today) instead of the panes'.
- **One sheet at a time.** The settings sheet and the dashboard share one drawer
  state with a kind (`Sheet::Settings`, `Sheet::Dashboard`), so they cannot both be up
  and share one animation. The sheet owns the keyboard while it is up, so `D` cannot
  open the dashboard over the settings sheet, nor a click on the author's name open
  the settings over the dashboard (a click outside the dashboard closes it, the
  second click opens the settings).
- `FullScreen` keeps `GitConfig` and `Welcome` (the git config editor has a long
  filtered list and the welcome screen has no panes behind it: neither is converted).

## Width

> **Superseded in S3** (see "S3, what the screenshots found"): the drawer is as wide as
> the page (112 cells), at most 95 % of the terminal, not 90 %. The analysis below is
> what the plan assumed before the screenshots.

The dashboard has two layouts, chosen from the width it is given: **wide** (two
columns, from 110 cells, `WIDE`) and **stacked** (one column, more scrolling); below
`NARROW` it is compact. A 75 % drawer (the settings sheet's width) gives two columns
only on a terminal of 147 columns or more; 90 % does it from 123.

```
terminal 120 cols        75 % = 90 cells  → stacked
                         90 % = 108 cells → stacked (just under 110)
terminal 140 cols        75 % = 105       → stacked
                         90 % = 126       → two columns
terminal 200 cols        both             → two columns
```

The drawer is 90 % wide, which keeps the two columns on the usual wide terminals while
still showing a strip of the panes. On a narrower terminal it is stacked, which is
the layout that exists today for narrow windows. (The settings sheet stays at 75 %.)

## Behaviour

| Case | Behaviour |
| --- | --- |
| `D` (or the bound key) with no sheet up | the dashboard slides in; stats are asked for as today (`ensure_stats`) |
| `D`, `Esc` or `q` while it is up | it slides out; a running computation is cancelled as today |
| a click outside the drawer | it closes (the panes behind are dimmed and not clickable) |
| the wheel over the drawer | scrolls the page (three rows a notch, as today) |
| `?` while it is up | the help screen over it, as today |
| a popup or a question is up | `D` does nothing, as today |
| the repository changes under it (a refresh) | the cache is dropped by the refs fingerprint, as today |
| the terminal is resized | the page is composed again for the new width |
| mouse capture off (`ui.mouse = false`) | keyboard only, as today |
| a result arrives after it closed | dropped when stale, kept when good, as today (`DashboardDone`) |
| `[theme] base` Dark or Light | the charts follow the painted theme; checked on screenshots (see Self-testing) |

## Backend

- `App`: the settings sheet's `author_overlay` and its click target become the sheet's:
  `sheet_overlay` plus `sheet: Option<Sheet>`; `open_dashboard` opens it with
  `Sheet::Dashboard`, `close_dashboard` closes it and cancels the worker. The places
  that tested `full_screen == FullScreen::Dashboard` (`input.rs`, `screens/mod.rs`
  twice, `dashboard.rs` for the error path) test the sheet kind instead.
- The run loop's animation tick and `advance_clock` / `finish_animations` already tick
  the settings drawer: they tick the shared one.
- `screens/mod.rs`: `draw_dashboard` becomes `dashboard_sheet::draw`, drawing the
  `View` into the drawer's inner area (90 % wide, above the key bar row, so the key bar
  can be the dashboard's); `draw_keybar` picks `Bar::Dashboard` while the dashboard
  sheet is up. `Drawer` gets the width as a parameter it already has; it is drawn into
  the area above the key bar, so the backdrop leaves the key bar alone.

## Self-testing (see `PLAN_SELF_TESTING.md`)

- `tests/dashboard_screen.rs` (44 frame tests) and `tests/app_dashboard.rs` (11): they
  open the dashboard with `D` and read the frame. They need two changes: let the slide
  finish (`advance_clock`) before reading, and a terminal wide enough for what they
  assert (two columns need 123 cells now, not 110). Their assertions on the content do
  not change; a test that only held in full screen is rewritten, not deleted.
- New tests: the panes are still on screen (dimmed) while the dashboard is up; `Esc`,
  `q`, `D` and a click outside close it; the wheel scrolls; the settings sheet and the
  dashboard exclude each other; opening and closing does not leave a worker running;
  a very small terminal does not panic; the key bar shows the dashboard's keys while
  it is up and the panes' after.
- `test/scripts/150-dashboard.script`: adapted the same way (the replay settles the
  slide on its own, `finish_animations`).
- Screenshots (the tmux harness, `__SOP/visual-verify.md`) of the dashboard in Dark and
  in Light, in a wide terminal and in a 120-column one. **Phase 18 did not look at the
  dashboard on a real terminal** (only the panes, a popup and the settings sheet): the
  charts' colours on the painted themes are checked here for the first time.

## Milestones

- **S0** ✅ the shared sheet state (`Sheet`, one overlay), the settings sheet moved onto
  it with no change of behaviour; its tests stay green.
- **S1** ✅ the dashboard drawn in the drawer, opened and closed by `D`, `Esc`, `q` and
  a click outside; the key bar; the worker's lifecycle.
- **S2** ✅ the tests and the replay script adapted and extended.
- **S3** ✅ the screenshots in both themes and both widths, anything the eye finds, then
  README, CHANGELOG, `PLAN_13_DASHBOARD.md` (a line saying it is a sheet now) and
  `PLAN_0_GENERAL.md`.

## Definition of done (phase 19)

`D` opens the dashboard as a drawer over the dimmed panes; `D`, `Esc`, `q` or a click
outside closes it; the statistics, the window key `t`, the counts `n`, the refresh `r`,
the scroll and the help work as before; the settings sheet and the dashboard never
show together. On a 140-column terminal the dashboard has its two columns. The charts
read in Dark and in Light (looked at, not only tested). `cargo clippy --all-targets
--all-features -- -D warnings` and `cargo test` are green, the replay passes, and the
README says it is a sheet.

## Decisions taken (confirmed by the user before S0)

- The drawer is wide, not 75 % like the settings sheet, which would stack the page on
  most terminals. (Planned as 90 % of the terminal; changed in S3, see below.)
- One shared drawer state with a kind (`app::sheet::Sheet`), not a second overlay state
  for the dashboard.
- No full-screen mode kept beside the sheet.
- `C` (git config) and the welcome screen stay full screen.

## S3, what the screenshots found (Terminal.app through tmux, the ferrit repository)

Dark and Light at 200 columns, and Dark at 120, read by eye.

- **The 90 % drawer was wrong on a wide terminal.** The page stops composing at 110
  cells (`MAX_WIDTH`), so at 200 columns a 90 % drawer (about 170 cells) held a 110-cell
  page floating in the middle with wide empty margins, and left only about 20
  columns of panes in sight, which defeats the point of a sheet. The drawer is now as
  wide as the page plus its two border columns (112 cells) and at most 95 % of the
  terminal: at 200 columns 88 columns of panes stay visible (the Files, Branches and
  Commits panes and the start of the diff), and at 120 columns the page keeps its two
  columns (112 of 120) with a strip of the panes still showing. Below about 118 columns
  it is 95 % wide and the page stacks, as it always did on a narrow terminal.
- Both themes read well on the page: the line chart, the ring, the heat map (the dark
  and the light greens), the contributor bars, the hot files and the branch table. The
  panes behind are visibly dimmed in both.
- **The double frame, then fixed** (asked for by the user after S3): the page drew its
  own rounded border inside the drawer's, and its header repeated the drawer's title.
  `View` gets `framed`: the sheet passes `false`, and the page then draws no border, no
  "Dashboard" in its header, and starts on its header row (the border's two rows come
  off its height, `rim`). The full-screen `draw` of the page keeps `framed: true` for
  the frame tests that build a `View` themselves. Screenshot after: one frame, the
  drawer's, with its title, and the page's header (`ferrit · main`, `window: 90 days
  (t)`) right under it.
- A screenshot taken after the window had been idle for a few seconds came out blank
  twice (the app was fine: `tmux capture-pane` showed the dashboard); taking it again
  gave the right frame. A property of the capture, noted in `__SOP/visual-verify.md`.

## S2, as built

`tests/dashboard_sheet.rs` (8 tests): the panes stay on screen behind the sheet and are
dimmed with the theme's own layer (Dark and Light); a click inside leaves it up and a
click over the panes slides it out, reaching no pane; the settings sheet and the
dashboard exclude each other (`D` inside the settings does nothing, a click on the
author's name over the dashboard only closes it); closing cancels the worker and
leaves nothing running; the key bar is the dashboard's while it is up and the panes'
after; a key while it slides out neither reopens it nor reaches a pane; tiny terminals
do not panic. Script 150 gains the click on the dimmed panes.

## S1, as built

`Sheet::Dashboard`; `FullScreen::Dashboard` is gone. `open_dashboard` is `open_sheet`,
`close_dashboard` closes the drawer and cancels the worker. `input.rs` routes keys and
the mouse by the sheet kind (`dashboard_key`, `dashboard_mouse`: the wheel scrolls, a
click outside the drawer closes). `screens/dashboard_sheet.rs` draws the `View` into
the drawer's inner area, 90 % wide and above the key bar row, so the key bar is
`Bar::Dashboard`; `OverlayState::is_closing` tells a drawer that is leaving from one
that is up (`App::dashboard_is_open`). The page stops composing at 110 cells
(`MAX_WIDTH`), so the 90 % drawer is as wide as the page ever gets from a terminal of
about 124 columns.

Existing tests adapted, not deleted: they let the slide finish before reading a frame,
read `dashboard_is_open` instead of `full_screen`, and ask for 130 columns where they
assert the two-column layout; "replaces the panes" became "a sheet over the dimmed
panes", and the replay script 150 expects the panes in sight.

## S0, as built

`src/app/sheet.rs`: `Sheet` (for now `Settings` alone; `Dashboard` comes with S1, so no
variant sits unused), `App::open_sheet`, `close_sheet`, `sheet_is_open`. `author_overlay`
is `sheet_overlay`, and `App::sheet` says which sheet it holds. The settings sheet
opens through `open_sheet(Sheet::Settings)` and `screens::draw` picks the sheet to draw
from `app.sheet`. No change of behaviour: the settings tests pass as they were.
