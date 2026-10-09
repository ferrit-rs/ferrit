# Plan: phase 22, break up `App`

**Status: in progress (`HelpState`, `ThemeEditor`, `git::Snapshot`, `RightPane`, `Modal`, `Workers`, `HitAreas` and the `dispatch` arms, `Nav`, `Authorship`, `Sheets`, `FullScreens` and `Prefs` done; the `ViewState` for drawing is open).** Third slice of the architecture clean-up. Needs phase 20 (typed
errors) and phase 21 (git port) first: sub-states are then testable against `FakeGit`.

What differs from the sketch below, and why:

- **No `RepoSnapshot` type.** `git::Snapshot` already has the seven fields (header, files,
  branches, remotes, commits, stashes, operation), so `App.snapshot: git::Snapshot` replaces
  them, and a refresh is `self.snapshot = snap`. The two drill-downs and `collapsed_dirs`
  are navigation state, not repository data; they stay on `App` for a later `Drill` step.
- **No `RgbChannel` / `PaletteIndex` newtypes yet.** `ThemeEditor` keeps `usize` and hides
  the arithmetic in `next_rgb_channel` and `move_palette`; a newtype would add noise now.
- `HelpState` exposes `view_parts()` so drawing can borrow the overlay mutably and the query
  immutably at once.
- **`Modal`, not one `Overlay` of five layers.** The characterization tests
  (`tests/app_overlay.rs`) showed the layers are not exclusive: a key-bar question sits over the
  welcome screen (`i` asks before `git init`) and over the git config editor, and `Esc` goes to a
  popup before the toast. What is exclusive is a popup against a question, so
  `app/modal.rs` has `enum Modal { None, Popup, Confirm }` and `App.modal` replaces `popup` and
  `pending_confirm`. The full-screen view and the sheet stay separate layers. Its methods live on
  `Modal` (not on `App`) so a caller can hold the popup mutably and read other fields of `App`.
- **`Workers` and `HitAreas`** took ten and eight fields (the event channel, the refresh, diff
  and image single-flight state, the one network operation; the pane rects, list offsets,
  click targets and keybar hits). `dispatch::run_action` was already a router but for five
  arms (`Back`, `Enter`, `Focus`, `NextPane`, `PrevPane`), which are now `go_back`,
  `enter_selected` and `focus_pane`.
- **Not done: step 6, a `ViewState` for drawing.** `screens::draw` still takes `&mut App`.
  `HitAreas` is what a draw function writes, so it can now be passed on its own, but the
  sheets and popups still read half of `App`. Left for a later phase.
- Count (fields of `pub struct App`, counted the same way before and after): 87 at the start of the phase, 26 now (target was about 15; the rest are the singles listed below). The "about 100" in the first sketch was an over-estimate.

## Goal

`App` stops being a bag of about 100 fields reached by 23 `impl App` blocks. It becomes a
small composition of named sub-states, each owning its fields and its methods, and the
modal UI (popup, confirm, sheet, full screen) becomes one enum so two overlays can never
be open at once by construction. No screen, key or behaviour changes.

```
Today                                    This phase
App {                                    App {
  help_scroll, help_rows, help_query,      help:     HelpState,
  help_mode,                               theme:    ThemeEditor,
  theme_config, theme_rgb_channel,         snapshot: RepoSnapshot,
  theme_mode, theme_palette_selected,      right:    RightPane,
  theme_picker_display,                    workers:  Workers,
  header, files, branches, commits,        layout:   Layout,     // rects, hits
  stashes, remotes, operation,             overlay:  Overlay,    // one at a time
  diff, preview, rendered_diff,            nav:      Nav,        // focus, selection
  right_key, right_scroll, ...             config, keymap, repo, toast, ...
  ~100 fields }                          }   // about 15 fields
```

## The gap this fixes

- `src/app/mod.rs:529-736`: `pub struct App` with about 100 fields, 3 `pub` (`focus`,
  `selection`, `show_help`) and the rest private. 23 files each carry an `impl App`
  (`grep -c "^impl App" src/app/*.rs`), and all of them touch any field, so the privacy
  is nominal. `mod.rs` is 2339 lines (about 92 KB).
- Modal state is spread over five unrelated fields that can contradict each other:
  `popup: Option<Popup>` (`mod.rs:409`), `pending_confirm: Option<ConfirmPrompt>`
  (`mod.rs:283`), `sheet: Sheet` plus `sheet_overlay`, `full_screen: FullScreen`
  (`mod.rs:398`), `welcome_dir`. Nothing stops `popup` and `pending_confirm` both being
  `Some`; code defends with ordering and early returns.
- `Mode` (`mod.rs:228`) is only `Nav | Diff`, a navigation mode, and is fine as is.
- `screens::draw` and 12 helpers take `&mut App` (`src/app/screens/mod.rs`), so
  rendering depends on the whole object.
- `dispatch::run_action` (`dispatch.rs:60`) is a single `match` over `Action` with the
  bodies of many arms inline.
- Primitives that mean something: `theme_rgb_channel: usize`, `theme_palette_selected:
  usize`, `selection: EnumMap<Pane, usize>`.

## Approach

Refactor under green tests, one extraction per commit, never mixed with behaviour.

1. **Characterization first (C0).** The existing 59 integration files and the replay
   flows are the safety net; add the few missing pins before touching a field (see
   Self-testing).
2. **Extract by cohesion, not by file.** Each sub-state is a struct in
   `src/app/<name>.rs` with private fields and the methods that used them, moved from
   the `impl App` blocks:

   | Sub-state | Takes over | Fields now on `App` |
   |---|---|---|
   | `HelpState` | `help_scroll`, `help_rows`, `help_query`, `help_mode`, `show_help` | 5 |
   | `ThemeEditor` | `theme_config`, `theme_rgb_channel`, `theme_mode`, `theme_palette_selected`, `theme_picker_display` | 5 |
   | `RepoSnapshot` | `header`, `files`, `collapsed_dirs`, `branches`, `remotes`, `commits`, `stashes`, `operation`, `branch_drill`, `commit_drill` | 10 |
   | `RightPane` | `diff`, `preview`, `rendered_diff`, `right_key`, `right_scroll`, `right_viewport`, `right_area`, `cursor` | 8 |
   | `Workers` | `event_sender`, `refresh_query`, `diff_query`, `image_query`, `remote_worker`, `remote_cancel`, `remote_busy`, `remote_busy_started` | 8 |
   | `HitAreas` | `left_areas`, `keybar_area`, `keybar_hits`, `author_click_area`, `dashboard_click_area`, `settings_hits`, `list_offset`, `view_detached_at` | 8 |

3. **One `Overlay` enum.**
   ```rust
   enum Overlay {
       None,
       Popup(Popup),
       Confirm(ConfirmPrompt),
       Sheet(Sheet),            // Settings | Dashboard, with its OverlayState
       FullScreen(FullScreen),  // GitConfig | Welcome
   }
   ```
   `App.overlay: Overlay` replaces `popup`, `pending_confirm`, `sheet`, `sheet_overlay`,
   `full_screen`. Key routing (`input.rs`) matches on it once instead of testing five
   options in a fixed order.
4. **Newtypes where an index has a meaning:** `RgbChannel(u8)` (0..3, `next()`/`prev()`
   wrap), `PaletteIndex(usize)`. Branch name and commit id newtypes (`BranchName`,
   `CommitId`) only where a function today takes two adjacent `&str`.
5. **Dispatch becomes a router.** `run_action` keeps the `match` but each arm is one call
   (`self.help.open()`, `self.theme.cycle_mode()`, `self.staging_action(a)`); bodies move
   into the owning module.
6. **A view for drawing.** `screens::draw(frame, &mut ViewState)` where `ViewState`
   borrows only what drawing needs (`&Nav`, `&RepoSnapshot`, `&mut HitAreas`, `&Palette`).
   This is the riskiest step and goes last; see Out of scope for the cut line.

## What it has to resolve

- **Borrow order.** `self.workers.spawn(&self.repo, ...)` borrows two fields of `self`
  at once; this compiles with disjoint fields and stops compiling if a method takes
  `&mut self` on `App`. Rule: a sub-state method takes only what it reads
  (`fn open(&mut self, rows: usize)`), never `&App`.
- **Hit areas are written during draw** (`&mut App` today). They move to `HitAreas` and
  `draw` takes `&mut HitAreas` explicitly, which is also what makes step 6 possible.
- **Re-find after refresh.** `select_when_listed` and `RightKey` survive a refresh by key;
  they stay together with the lists they index (`RepoSnapshot`/`Nav`), not with the
  worker plumbing.
- **Test access.** `src/app/tests.rs` uses `super::*` to read private fields. After the
  split it uses the sub-state accessors (`app.snapshot.files()`), the same ones the
  integration tests use.

```
key event ──► input.rs ──► match overlay { Popup | Confirm | Sheet | FullScreen | None }
                                 │                                          │
                                 ▼                                          ▼
                       overlay handler (owns its state)            Action ──► dispatch (router)
                                                                               │
                              HelpState │ ThemeEditor │ RepoSnapshot │ RightPane │ Workers
```

## State on `App`

After the phase (target, about 15 fields): `nav`, `help`, `theme`, `snapshot`, `right`,
`workers`, `hits`, `overlay`, `repo` (the port, phase 21), `config`, `keymap`, `profile`,
`toast`, `status_note`, `should_quit`. `app/mod.rs` target: under 600 lines, containing
`App`, `new`, `run` and the event match.

Test seams keep their names (`set_*`, `feed_key`, `on_*_done`); only where they live moves.

## Impl sketch

```rust
// src/app/help.rs
pub(super) struct HelpState { scroll: usize, rows: usize, query: TextInput, mode: HelpMode, open: bool }
impl HelpState {
    pub(super) fn open(&mut self, rows: usize) { self.open = true; self.rows = rows; self.scroll = 0; }
    pub(super) fn scroll_by(&mut self, delta: isize) { /* clamp to rows */ }
}

// src/app/dispatch.rs
Action::Help => self.help.open(self.hints.row_count()),   // one line per arm
```

## Out of scope

- **A full `ViewModel` for every screen.** Step 6 only covers the left panes and the key
  bar; the sheets and popups keep taking `&mut App` until a later phase, because their
  state is still entangled with the overlay enum.
- **Replacing `Pane`/`EnumMap` selection** with a per-pane struct. Works, not confusing.
- **Elm-style message/update split.** `Action` + `AppEvent` already express it; a rename
  would churn tests for no gain.
- **Behaviour changes of any kind.** If a bug is found, it gets its own commit after the
  extraction and a `CHANGELOG.md` line.

## Self-testing (see `PLAN_SELF_TESTING.md`)

- C0 pins (new, `tests/app_overlay.rs`): opening a popup while a confirm is up, opening
  the settings sheet from the dashboard sheet, `Esc` order with a popup over a sheet. These
  record today's behaviour so the `Overlay` enum can be checked against it.
- `tests/replay_harness.rs` and `test/flows/*.flow`: byte-identical frames before and after
  every extraction (`.dev-tools/tui-shot.sh` diff).
- Unit tests per sub-state (`HelpState::scroll_by` clamps, `RgbChannel` wraps) written
  before the move, in the new file, red then green.
- All prior phase tests stay green.

## Milestones

- **C0, pins.** `tests/app_overlay.rs`; no code change.
- **C1, `HelpState`.** Smallest and most isolated; sets the pattern.
- **C2, `ThemeEditor`** with `RgbChannel` and `PaletteIndex`.
- **C3, `RepoSnapshot`.** Largest field count; `apply_refresh_result` moves with it.
- **C4, `RightPane`** and **C5, `Workers`**, **C6, `HitAreas`.**
- **C7, `Overlay` enum.** Replaces the five modal fields; `input.rs` routes on it.
- **C8, dispatch as a router.** Arm bodies move to their owners.
- **C9, draw takes `HitAreas`** (and the left panes' read-only view).
- **C10, close.** `app/mod.rs` under 600 lines, `App` under 20 fields, clippy clean, layering
  held, all prior C green.

## Definition of done (phase 22)

- [ ] `wc -l src/app/mod.rs` is under 600; `App` has at most 20 fields.
- [ ] `popup`, `pending_confirm`, `sheet`, `full_screen` are gone from `App`; `overlay` exists.
- [ ] No `impl App` block outside `mod.rs`, `dispatch.rs` and `input.rs` reaches into a
      sub-state's private field.
- [ ] Replay flows and `tests/render.rs` frames unchanged.
- [ ] `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`,
      `cargo nextest run`, `cargo doc` with `-D warnings` pass.
- [ ] `PLAN_0_GENERAL.md` row 22 marked done; no `CHANGELOG.md` line unless a bug fix.

## After phase 22

Phase 23 (`PLAN_23_TEST_SUPPORT.md`) cleans the test suite and narrows the public API, now
that the types it exposes are small and named.

- **`Nav` and `Authorship`.** `App.nav` holds focus, the cursor of each pane, the two drills,
  the collapsed directories, the Branches tab, the rows to select once listed, the
  right-pane focus flag and the keyboard mode (nine fields; `focus` and `selection` stay
  public, now as `app.nav.focus`). `App.authorship` holds the git profile, ferrit's author
  pick and `user.name` (three fields) and the three methods that were scattered: the author
  line, `Ctrl-A` cycling and the `--author` value, which was built by the same closure in
  three places (`commit`, `rebase_actions`, `create_remote`) and is now `author_arg()`.
  `repo_name` stayed on `App`: it names the repository, not the author.

- **`Sheets` and `FullScreens`.** `App.sheets` (in `sheet.rs`) holds the drawer's animation,
  which sheet it holds, the settings sheet and the dashboard; the settings scroll moved into
  `SettingsSheet`, where it belongs. `App.full_screens` holds the active full-screen view,
  the git config editor's state and the welcome screen's folder and row. Nine fields became
  two; the 120-odd call sites were renamed by script and the compiler found no collision.

- **`Prefs`** (`prefs.rs`) holds the loaded `config.toml`, the keymap built from it, the file a
  save writes to, the terminal's colour depth and the palette: five fields that change together
  when the settings sheet saves.

## What is left on `App` (26 fields)

The create-remote flow (`create_remote`), what the user is told (`last_error`, `watch_error`,
`status_note`, `toast`, `commit_overlay`), the run loop's requests (`should_quit`,
`watch_request`, `terminal_request`), and a few singles (`repo`, `repo_name`, `mouse_pointer`,
`commit_draft`, `new_branch_title`). Grouping what the user is told into one `Feedback` value
is possible but would touch the toast and Status line code for little clarity; what remains is
`App` doing its job as the root of the state. The bigger open item is `screens::draw`, which
still takes `&mut App`.
