# Plan: phase 24, drawing reads the app, it does not change it

**Status: planned.** The step 6 that `PLAN_22_APP_SPLIT.md` left open ("a view for drawing").
Nothing a user sees changes; what changes is what the drawing code is allowed to touch.

## Goal

`screens::draw` takes `&App` for what it reads, and says out loud what it learned while drawing
(where each pane landed, how far a list is scrolled) instead of writing it into `App` from fifteen
places. The pieces ratatui needs `&mut` for (overlay animations, the image protocol, the diff
cache) live in one small `RenderState`, passed on its own. A reader of `screens/` can then trust
the signature: a function that takes `&App` cannot change the application.

```
Today                                    This phase
draw(frame, &mut App)                    draw(frame, &mut App)            same public entry
  panes ─► app.set_left_area(..)           │ take RenderState out of App
  list  ─► app.set_list_offset(..)         ▼
  right ─► app.set_right_viewport(..)    render(frame, &App, &mut RenderState) ─► Landed
  keybar─► app.set_keybar_hits(..)         │                                      (plain data)
  ...15 write-backs, 4 kinds of state      ▼
                                         app.land(landed); put RenderState back
```

## The gap this fixes

- Every function of `src/app/screens/` takes `app: &mut App` (`draw`, `draw_panes`,
  `draw_left_column`, `draw_right_pane`, `draw_command_log`, `draw_keybar`, `draw_welcome`,
  `draw_git_config`, `diff::draw_files_columns`, `diff::draw_single_file_diff`,
  `settings::draw`, `dashboard_sheet::draw`). The signature says nothing about what they change.
- They change a lot, in small pieces, from deep inside the layout code. Counted with
  `grep "app.set_" src/app/screens`: `set_left_area`, `set_list_offset`, `set_right_area`,
  `set_right_viewport` (four call sites), `set_author_click_area`, `set_dashboard_click_area`,
  `set_keybar_hits`, `set_git_config_offset`, `help.set_rows`, `clamp_dashboard_scroll`, plus
  direct writes: `app.hits.settings = ...` and `app.sheets.settings.scroll` / `.follow`
  (`settings.rs:241-248`, `:283`).
- A second kind of write is ratatui's own need for `&mut`: `Drawer::new(&mut app.sheets.overlay)`
  in `settings.rs:172` and `dashboard_sheet.rs:25`, `app.help.view_parts()` (the help overlay),
  `&mut app.toast`, `popup_view(&mut self)` and `commit_popup(&mut self)` (they hand out
  `&mut OverlayState`), `preview_mut()` (the image `StatefulProtocol` renders through `&mut`),
  and `rendered_diff(&mut self, ..)` (a cache).
- The mouse code depends on the first kind: a click is routed by the rects the last frame
  recorded (`HitAreas`, `PLAN_22`). That is a real feedback loop, and it deserves a name and one
  place, not fifteen.

## Approach

1. **`Landed`: what a frame learned, as plain data** (`src/app/screens/landed.rs`). One field per
   write-back above: `left: EnumMap<Pane, Rect>`, `list_offset: EnumMap<Pane, usize>`,
   `right_area`, `right_viewport`, `author`, `dashboard`, `keybar` and its hits, `settings_hits`,
   `settings_scroll`, `git_config_offset`, `help_rows`, `dashboard_max_scroll`. The draw functions
   take `&mut Landed` and fill it; `App::land(Landed)` applies it once, at the end of `draw`. The
   existing setters (`set_left_area`...) become the body of `land`, so tests that call them keep
   working.
2. **`RenderState`: what ratatui needs mutable** (`src/app/render_state.rs`). The animation of
   the help, the sheets and the commit popup, the `Toast` (it owns its animation), the image
   `Preview` and the rendered-diff cache. They are not state of the application; they are state of showing it. Today they sit in
   `HelpState.overlay`, `Sheets.overlay`, `App.commit_overlay`, `Toast`, `RightPane.preview` and
   `RightPane.rendered`; they move to `App.render`.
3. **The public entry keeps its shape.** `draw(frame, &mut App)` and `draw_painted` stay (33
   files, 42 call sites in tests and the run loop call them). Inside, it takes `RenderState` out
   with `std::mem::take`, calls `render(frame, &App, &mut RenderState) -> Landed`, puts the state
   back and lands the facts. Every reader of an animation inside the draw path reads it from the
   `RenderState` argument, never from `app.render`, which is empty for the duration.
4. **Order of work: facts first.** `Landed` is safe and valuable alone (C1 to C3): it removes the
   scattered writes and lets most draw functions take `&App`. `RenderState` (C4 to C6) is the
   riskier half; there is a decision point before it (see the milestones).

## What it has to resolve

- **The take-and-restore window.** While `render` runs, `app.render` is a default value. Any
  `&App` method called from the draw path that reads an overlay (`help_is_open`,
  `dashboard_is_open`, `sheet_is_open`, `Toast::is_animating`) would silently see "closed". The
  fix is mechanical, and the test below is what catches a miss: those reads move to take a
  `&RenderState`, or are computed once before the take and passed down.
- **Methods that are `&mut self` only for the overlay.** `popup_view` and `commit_popup` build a
  view that borrows an `OverlayState`. They become `popup_view(&self, render: &mut RenderState)`.
- **The image preview.** `Preview::Image` holds a `StatefulProtocol` that renders through
  `&mut`; `preview_mut()` is used in `draw_right_pane`. It moves to `RenderState` together with
  the decision of what to draw, which `update_preview` (in `image_query.rs`) sets.
  Work done off the UI thread stays as it is; this is only where the finished protocol lives.
- **Scroll that corrects itself.** `settings.rs` and `clamp_dashboard_scroll` change a scroll
  offset to keep the selected row in view. These are facts about the frame (`settings_scroll`,
  `dashboard_max_scroll`) and go through `Landed`, applied after the frame.
- **Tests that read after drawing.** `tests/mouse.rs`, `tests/scrollbar.rs` and the list tests
  draw, then click. They keep working because `draw` still lands the facts before it returns.

```
frame N                      frame N+1
App state ──► render ──► Landed ──► App.hits, scroll offsets
   ▲                                    │
   └──────── mouse click uses ◄─────────┘   (routing by what the user saw)
```

## State on `App`

| Field | Before | After |
|---|---|---|
| `hits` (`HitAreas`) | written by `set_*` and direct assignment | written only by `App::land` |
| `help.overlay`, `sheets.overlay`, `commit_overlay`, `toast` | spread over four places (the `Toast` component owns its own animation) | `render.help`, `render.sheet`, `render.commit`, `render.toast` (the whole `Toast`) |
| `right.preview` (image protocol), `right.rendered` (diff cache) | on `RightPane` | `render.image`, `render.diff_cache` |
| new: `render: RenderState` | none | one field, `Default` |

Test seams keep their names (`set_left_area`, `set_list_offset`, ...): they become the thin
pieces `land` is made of.

## Impl sketch

```rust
// src/app/screens/mod.rs
pub fn draw(frame: &mut Frame<'_>, app: &mut App) {
    let mut render = std::mem::take(&mut app.render);
    let landed = render_frame(frame, app, &mut render); // app is &App here
    app.render = render;
    app.land(landed);
}

fn render_frame(frame: &mut Frame<'_>, app: &App, render: &mut RenderState) -> Landed {
    let mut landed = Landed::default();
    let help_open = render.help.is_visible() || app.help.open;   // read once, passed down
    // ... draw_panes(frame, app, render, &mut landed, area) ...
    landed
}
```

## Out of scope

- **A `View<'a>` struct borrowing field by field.** It would let `draw` take `&App` and
  `&mut RenderState` without `mem::take`, but every `impl App` method the screens call
  (`status_lines`, `branch_lines`, `commit_lines`, `file_lines`, `diff_view`...) would have to move
  to the view. About twenty methods for a gain the `take` already gives. Revisit if the window
  above causes a bug.
- **Interior mutability (`Cell`, `RefCell`) for the facts.** It would give `&App` without a
  `Landed`, by hiding the writes. The point of this phase is to show them.
- **Changing what any screen looks like.** Frames must be byte-identical.
- **The `components/` widgets.** They already take `&mut` state in the ratatui style.

## Self-testing (see `PLAN_SELF_TESTING.md`)

New file `tests/draw_purity.rs`, using `App::with_git(FakeGit)` so no repository is needed:

- **Drawing twice gives the same frame and the same `Landed`**, in each screen: panes, help,
  commit popup, settings sheet, dashboard sheet, welcome, git config. A read of an overlay that
  went missing in the take-and-restore window shows up as a different second frame.
- **Draw changes nothing else**: before and after a draw, `status_lines()`, `row_count(pane)`,
  the selection of every pane, `commit_popup().is_some()`, `confirm_message()` and
  `sheet_is_open()` are equal.
- **Landed reaches the mouse**: draw, then click on the rect `Landed` reported for each pane and
  check the focus moved (`tests/mouse.rs` already does this for most panes).
- **Animations still advance**: open the help, advance the clock, draw, and the help is fully
  open (`tests/hints.rs`, `tests/scheme_paint.rs` cover the painted and toast cases).
- All frame tests (`tests/render.rs`, `scheme_paint.rs`, the replay scripts) stay green, byte for
  byte: they are the proof nothing visible moved.

## Milestones

- **C0, pin.** `tests/draw_purity.rs` against today's code (it must pass before anything moves).
  No code change.
- **C1, `Landed` exists.** The struct and `App::land`, with the setters as its pieces. `draw`
  builds a `Landed` and lands it; the draw functions still write directly. Suite green.
- **C2, facts out.** Move the write-backs of `mod.rs`, `diff.rs`, `settings.rs`,
  `dashboard_sheet.rs` into `Landed` one file per commit. After each, that file's functions take
  `&App` where they no longer write.
- **C3, facts done.** No `app.set_*` or direct `app.hits` / `app.sheets.settings` write in
  `screens/`. **Decision point:** if every draw function that reads only now takes `&App`, stop
  or go on to C4 depending on whether the remaining `&mut App` parameters are worth removing.
- **C4, `RenderState` exists.** The struct, `App.render`, the four overlays move in (one commit
  each), reading through the same accessors. No behaviour change.
- **C5, preview and cache.** `Preview::Image` and the rendered-diff cache move to `RenderState`;
  `preview_mut` and `rendered_diff` take it.
- **C6, signatures flip.** `render_frame(frame, &App, &mut RenderState) -> Landed`; `draw` does
  the take and restore. `popup_view` and `commit_popup` take `&RenderState`. Every function in
  `screens/` takes `&App`; none takes `&mut App`.
- **C7, close.** `cargo clippy --all-targets --all-features -- -D warnings` clean, `tests/layering.rs`
  and all earlier milestones green, `PLAN_22` and `docs/architecture.md` updated.

## Definition of done (phase 24)

- [ ] `grep -rn "&mut App" src/app/screens` returns nothing.
- [ ] `grep -rn "app\.set_\|app\.hits\.[a-z_]* =" src/app/screens` returns nothing.
- [ ] `App::land` is the only writer of `HitAreas` and of the scroll offsets the frame corrects.
- [ ] `tests/draw_purity.rs` passes for every screen listed above.
- [ ] Every existing frame test and replay script passes unchanged.
- [ ] `cargo fmt --check`, clippy with and without `--all-features`, `cargo doc` with
      `-D warnings` and `cargo nextest run` pass.
- [ ] `PLAN_0_GENERAL.md` row 24 marked done; no `CHANGELOG.md` line (nothing visible).

## After phase 24

`App` is the root of the state and `screens/` only reads it. What is left on `App` is the
create-remote flow, the feedback fields (`last_error`, `status_note`, `toast`) and the run loop's
requests; none of them is worth a plan. If the take-and-restore window ever causes a bug, the
`View<'a>` struct in "Out of scope" is the next step.
