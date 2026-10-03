# Plan: phase 17, the settings sheet

**Status: in progress (U0 to U3 done: `Config::save_sections`, the rows, live changes and autosave, and the sheet itself, drawn by `screens/settings.rs`; the author line in the commit popup is next).** lazygit has no settings screen
(its config is a YAML file), so this phase is not compared with it; it is checked
with the replay harness and screenshots (`PLAN_SELF_TESTING.md`).

## Goal

The sheet that opens when the user clicks the author's name (`Infos` box) today
is called "Profile" and mixes three things: the git identities (git's config), a
contributors-and-activity block (the dashboard's job) and a theme editor (ferrit's
own). This phase makes it **the sheet of ferrit's own settings and nothing else**:
a theme (dark or light, an accent colour with the existing colour picker), the
interface, the diff, the commit editor and the command log. Every change is saved
at once to `config.toml`, so the next start finds the sheet as it was left and
nobody sets things again.

```
 click on the author's name (the only way in; no key)
        │
        ▼
┌ Settings ─────────────────────────────────────────────┐
│ Appearance                                            │
│ ▸ Theme            (•) Dark   ( ) Light               │
│   Accent           ● Green  ○ Blue  ○ Purple  ○ Amber │
│                    ▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒▒    │
│                    (the colour picker, as today)      │
│                    RGB  R 000  G 255  B 000           │
│ Interface                                             │
│   Mouse            [x]                                │
│   Wheel step       3                                  │
│   Refresh every    10 s                               │
│ Diff                                                  │
│   Context lines    3                                  │
│   Ignore whitespace [ ]                               │
│ Commit                                                │
│   Sign-off by default [ ]                             │
│ Command log                                           │
│   Show read commands [ ]                              │
│                                                       │
│ Saved as you change it · …/ferrit/config.toml         │
└ ↑↓ row · ←→ or Space change · Enter picker · Esc ─────┘
```

Out, on purpose (see "Out of scope"): key remapping (it stays in the file),
anything that edits git's own configuration (that is `C`, phase 14), and the
contributors and activity charts (that is `D`, phase 13).

## What changes in what exists

- **Removed from the sheet**: the "Global Git users" block (the identity cards, the
  `1` to `9` and `0` keys, the confirmation) and the whole activity block
  (`domain/profile/activity.rs`, `Repo::activity`, the heat map and the
  contributors). The README row "Recent activity across local and fetched remote
  branches" goes with it: the dashboard covers activity.
- **Kept**: the colour picker (the palette grid, the spectrum view, the RGB
  editor), the theme presets, the drawer shell and its open / close animation, the
  click on the author's name as the way in.
- **The author of Ferrit's commits** is a ferrit behaviour (it passes `--author`),
  so it stays, but it leaves the sheet: it becomes a line in the commit popup
  (`author: Name <email>`, `Ctrl-A` cycles the identities git knows, then "git's
  own"). It is still for this run only and never writes git's config.
- **The author label** in the `Infos` box still shows the effective identity and is
  still the click target that opens the sheet.

## Settings

Every row is a key of `config.toml` that ferrit already reads; nothing new is
invented except where a row needs it.

| Section | Row | Key | Values | Takes effect |
| --- | --- | --- | --- | --- |
| Appearance | Theme | `theme.base` | Dark, Light | at once (the palette is rebuilt) |
| Appearance | Accent | `theme.preset`, `theme.accent` | four presets, or any colour from the picker | at once |
| Interface | Mouse | `ui.mouse` | on, off | at once: mouse capture is switched on or off in the terminal |
| Interface | Wheel step | `ui.wheel_step` | 1 to 50 | at once |
| Interface | Refresh every | `ui.poll_secs` | 1 to 3600 s | at once: the poll thread reads a shared interval |
| Diff | Context lines | `diff.context` | 0 to 200 | at once: the diff in view is read again |
| Diff | Ignore whitespace | `diff.ignore_whitespace` | on, off | at once, same |
| Commit | Sign-off by default | `commit.sign_off` | on, off | the next commit popup |
| Command log | Show read commands | `log.show_reads` | on, off | at once |

`diff.rename_threshold` is left to the file: nobody changes it twice.

## Saving

**Automatic, per change, no Save button.** After each change ferrit writes the
sections it touched with `Config::save_sections`, which merges into the file the
way `save_theme` does (the file is read, only those sections are replaced, unknown
sections are kept, the write goes through a staging file and a rename). One write
per change, never a half-written file.

- No config file path (tests, a system with no config directory): nothing is written
  and that is not an error, as today.
- A file that is not valid TOML is **not overwritten**: the footer of the sheet says
  so, and the setting still applies for the run.
- A file that cannot be written (read-only, full disk): the footer says why; the
  setting still applies for the run, and the next change tries again.
- Comments in the file are not preserved (as before: the `toml` crate cannot);
  the file header says so.

The footer shows the path of the file and, after a write, `saved`; after a failure,
the reason.

## Keys and mouse

The sheet owns the keyboard while it is up (after the popups and questions, like the
other overlays). No key opens it: the click on the author's name does.

| Key | Action |
| --- | --- |
| `↑` `↓` (or `k` `j`) | move between rows |
| `←` `→` (or `h` `l`) | change the value: flip a toggle or radio, step a number |
| `Space` | flip a toggle, or the next value |
| `Enter` | on Accent: open the colour picker; on a number: nothing |
| `PgUp` `PgDn` `Home` `End` | scroll when the sheet is taller than the screen |
| `Esc` | close the colour picker if it is open, else close the sheet |

In the colour picker (as today): arrows move over the swatches, `v` switches palette
and spectrum, `Enter` applies, `e` edits the RGB channels (`Tab` changes channel,
arrows change the value), `Esc` goes back. Each applied colour is saved at once.

The mouse: a click on a value sets it (a radio, a toggle, a `‹` or `›` around a
number, a swatch of the picker); the wheel scrolls; a click outside the sheet closes
it. The sheet needs no mouse to be used.

## Backend

- `Config::save_sections(path, &[(&str, toml::Value)])` in `app/config/mod.rs`;
  `save_theme` becomes a call to it. No change to what is read.
- `src/app/settings.rs` (new, state and keys; drawing is `screens/settings.rs`): the
  rows, the selected row, the colour picker's own state (moved from the `theme_*`
  fields of `App`), `change`, `save`. The live `Config` is `App::config`, so every
  other part of ferrit already reads the new value (`diff_opts()`, the wheel, the
  commit popup, the log panel).
- Live effects that need the run loop: switching mouse capture on or off, and the new
  refresh interval. Both go through a small request the loop applies
  (`terminal_request`), like the watch request of phase 16.
- `Events` gets a shared poll interval (`Arc<AtomicU64>`) so `ui.poll_secs` changes
  without a restart.

## Rendering: `src/app/screens/settings.rs`

Replaces `screens/profile.rs`, `profile/settings.rs` and `profile/activity.rs`. The
drawer shell is the existing `Drawer` (75% width, slide-in). Rows are
`  Label        value` with the selected row marked `▸` and highlighted like the other
menus. A radio is `(•) a  ( ) b`, a toggle `[x]` or `[ ]`, a number `‹ 3 ›`. Section
titles are the existing `Separator`. The colour picker block keeps its current lines.
A sheet taller than the screen scrolls, with the existing scroll bar.

## Edge cases

| Case | Behaviour |
| --- | --- |
| no `config.toml` yet | the first change creates it, with the header and only the sections touched |
| the file has unknown sections or keys | kept untouched on every save |
| the file has `[keys]` entries | kept untouched; the sheet says remapping is done in the file |
| a value the user typed in the file is out of range (`wheel_step = 200`) | the load already reports and clamps it; the sheet shows the clamped value and a change saves a valid one |
| `ui.mouse` turned off from the sheet | the click that did it was the last click: from then on the sheet is keyboard-only, and the footer says so |
| a very small terminal | the sheet scrolls; nothing draws outside the drawer |
| the theme changes while a popup is up | cannot happen: the sheet owns the keyboard |
| two ferrit windows open | the last write wins on the file; each keeps its own live values until restarted |
| the repository has no identity configured | the commit popup's author line says "git's own" and `Ctrl-A` has nothing to cycle |

## Self-testing (see `PLAN_SELF_TESTING.md`)

- `tests/config.rs`: `save_sections` keeps unknown sections and comments' absence is
  documented, writes through a staging file, refuses an invalid file without touching
  it, and writes several sections in one write.
- `tests/app_settings.rs`: every row changes its value with `←` `→` and `Space`, clamps
  at its ends, and each change is on disk (read back with `Config::load`); a change
  with no config file path does not fail; an unwritable file reports and still applies.
  The colour picker keeps working (palette, spectrum, RGB) and its result is saved.
- `tests/settings_screen.rs`: the sheet at 120, 80 and 50 columns and a short height,
  the selected row, the footer states (saved, invalid file, not writable), no trace of
  identities or activity.
- `tests/app_commit.rs`: the author line in the commit popup and `Ctrl-A`.
- `test/scripts/190-settings.script`: click the author's name, change the theme, a
  toggle and a number, close, and check the file with `git`-free checks (`config`
  directive reopen) that the values stay.

## Milestones

- **U0** ✅ `Config::save_sections` and `Section`, tests (`tests/config.rs`).
- **U1** ✅ `settings.rs` state, rows, autosave and every live effect, mouse switch and
  refresh interval included (they go through `terminal_request` and `poll_request`,
  which the run loop applies), tests through the `App` (`tests/app_settings.rs`). The
  keys are routed in U2.
- **U2** ✅ `screens/settings.rs` and the routing: the click on the author's name opens
  it, the old drawer's identity and activity blocks and keys go, `domain/profile/activity.rs`
  and `Repo::activity` go with them (the refresh no longer walks two years of commits for it).
- **U3** folded into U1 (the live mouse switch and refresh interval).
- **U4** the author line and `Ctrl-A` in the commit popup.
- **U5** the replay script and a last pass over README, CHANGELOG and plans.

## Definition of done (phase 17)

Click the author's name: a sheet with ferrit's settings only opens. Change the theme
to light, turn the mouse off, set the context lines to 5, close ferrit, open it
again: everything is as left, and `config.toml` holds exactly those changes plus what
was there. The colour picker works as before and its colour is kept too. No identity,
no contributor and no activity appears in the sheet; the commit popup lets the author
be chosen. `cargo clippy --all-targets --all-features -- -D warnings` and `cargo test`
are green, the replay script passes, and the README table is up to date.

## Out of scope

- remapping keys from the sheet (the `[keys]` tables stay in the file)
- editing git's configuration (`C`, phase 14) and the repository statistics (`D`,
  phase 13)
- per-repository settings: the file is the user's, for every repository
- importing or exporting settings, profiles of settings
- a language or locale setting
- persisting the chosen commit author across runs
