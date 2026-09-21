# Plan: phase 5, click behaviour

## Goal

Make a left mouse click do the obvious thing: click a row in a left pane and
that pane takes focus with its selection cursor on the clicked row. Same
gesture lazygit and gitu use. Mouse capture and the wheel already landed in
phase 4 (`PLAN_4_SCROLL_BEHAVIOR.md`); this plan is the click half.

Still read only. Still one focused left pane, the right pane a follower. This
plan does **not** turn the right pane into its own focus context (that is the
follow-up, see "Out of scope").

## The gap this fixes

Phase 4 turned mouse capture on and wired `App::on_mouse`, but only for the
wheel:

```rust
fn on_mouse(&mut self, ev: MouseEvent) {
    let step = match ev.kind {
        MouseEventKind::ScrollDown => 1,
        MouseEventKind::ScrollUp => -1,
        _ => return,            // <- every click and drag falls out here
    };
    ...
}
```

So today a click does nothing at all. Capture is on (native text selection is
already gone), the user pays that cost, and gets no click in return. lazygit
and gitu both let you click a list row to select it.

## Approach: lazygit's `HandleClick`, trimmed to what phase 5 needs

Four references, same gesture: two in Go (lazygit, its Rust port), two in
ratatui 0.30 (gitpane, drydock). The ratatui pair matters most, they hit the
exact API ferrit has.

`../ferrit-references/tui/gitu/src/app.rs` `handle_mouse_input`:

```
MouseEventKind::Down(MouseButton::Left):
    click_y = mouse.row
    if that screen row is a valid list line:
        move the selection to it
        if it was already the selected row: activate it (Show / ToggleSection)
MouseEventKind::Down(MouseButton::Right):
    move the selection there, then Show
ScrollUp / ScrollDown: move the view (phase 4 already does this)
```

`../ferrit-references/tui/lazygit/pkg/gui/controllers/list_controller.go`
`HandleClick` (lines 243-268) is the same idea with a cleaner order. That
order is the spec for `on_mouse`'s left-click arm:

```
1. newIdx   = ViewIndexToModelIndex(click.Y)   // screen row -> model row
2. wasFocused = this pane is the focused one    // capture BEFORE step 3
3. if !wasFocused: push this pane as the focus  // focus first, always
4. if newIdx > Len()-1: return                  // click past last row:
                                                //   focus only, no select
5. SetSelection(newIdx)
6. if isDoubleClick && wasFocused && onDoubleClick != nil: onDoubleClick()
7. HandleFocus{}                                // rebuild main / right view
8. onClick hook                                 // per-view extra (files:
                                                //   toggle a section header)
```

lazygit also runs `switch_to_focused_main_view_controller.go` on a click in
the `main` / `secondary` view: the click focuses that view. That is the
right-pane-as-focus-context work, deferred (see "Out of scope").

The same mapping in ratatui, for the record:

`../ferrit-references/tui/gitpane/src/components/repo_list/render.rs`
`handle_mouse_event`, `Down(Left)` arm:

```
content_y  = render_area.y + 1               // strip the border row
visual_row = mouse.row - content_y
idx        = visual_row + self.state.offset() // ListState::offset, read live
if idx < display_rows.len() { self.state.select(Some(idx)) }
```

`../ferrit-references/tui/drydock/crates/drydock/src/tui/mod.rs`
`select_at_row(row, first_row) -> bool`:

```
if row < first_row { return false }          // border / header row
idx = self.scroll + (row - first_row)
if idx >= self.visible.len() { return false } // past the tail
self.selected = idx; true
```

Both are `click_row` from "Coordinate mapping" below, already shipping.
drydock's `-> bool` return is the shape ferrit copies (see `click_pane`).

ferrit phase 5 keeps **steps 1 to 5 and 7**:

- step 1: `ViewIndexToModelIndex`, see "What a click has to resolve" below.
- step 2 + 3: focus the pane before touching the selection. A click anywhere
  in a pane's rect focuses it, even the border or the empty tail.
- step 4: click past the last row is focus-only, cursor unmoved. No panic,
  no clamp-to-last surprise.
- step 5: move `selection[pane]`.
- step 7: `update_right_pane()`, ferrit's `HandleFocus` equivalent.

Steps 3 to 5 fold into one `App::click_pane(pane, row) -> bool` (mirrors the
existing `select(pane, index)`), returning whether the cursor moved so the
test and the border / tail case read off one value, drydock's
`select_at_row`. Routing (`pane_at`) and step 7 stay in `on_mouse`.

Dropped for phase 5:

- step 6, the double-click branch. crossterm has no double-click and gitu's
  "click the already-selected row" gesture covers the same need with no
  timer. Folds into phase 6 / 7 (nothing to activate read-only).
- step 8, the `onClick` hook. First user is Files' Staged / Unstaged section
  headers in phase 6; no headers exist yet.
- right-click (step order N/A): waits for the `x` context menu (phase 12).

## What a click has to resolve

```
                 screen
   +----------------------------------------+
   | [1] Status            |  Unstaged      |   click here (col < side):
   +----------------------+                 |     -> which left pane row?
   | [2] Files            |   <diff text>   |
   |  M src/domain/app/mod.rs   <----+-- click row     |   click here (col >= side):
   |  M src/components/screens/mod.rs         |                 |     -> right pane. no-op for now
   +----------------------+                 |         (right-pane-focus plan)
   | [3] Local branches   |                 |
   |  * main              |                 |
   +----------------------+                 |
   | [4] Commits          |                 |
   |  a1b2c3d  msg        |                 |
   +----------------------+-----------------+
   | command log                            |   click here: no-op
   +----------------------------------------+
   | q quit  ? help  ...                    |   click here: no-op (for now)
   +----------------------------------------+
```

Click -> row index inside a pane, accounting for:

1. **which pane rect** the click is in. `draw_left_column` builds the 5 rects
   locally every frame and never stores them. It must write them back to
   `App`, the way `draw_right_pane` already writes `right_area`.
2. **the top border**: row 0 of the rect is the border/title, not a list row.
3. **the list scroll offset**. `ListState::offset` is recomputed from zero
   every frame today (nothing persists it), so ratatui auto-scrolls to keep
   the selection visible but ferrit does not know the offset. To map a click
   in a scrolled `Files` / `Commits` list, that offset has to be read back off
   the `ListState` after render and stored.
4. **non-model rows inside the list** (later). lazygit's
   `viewIndexToModelIndex` (`context/list_renderer.go:98`) subtracts every
   section header that sits above the clicked line, because a header is a
   drawn row with no model entry. ferrit's lists are flat today, so the
   subtraction is 0, but Files grows Staged / Unstaged headers in phase 6.
   Isolate the mapping in one function now so phase 6 adds the header count
   in one place.

### Coordinate mapping: screen row -> model index

lazygit's `ViewIndexToModelIndex`, ferrit terms. gocui folds the scroll
origin into the view Y for lazygit; ratatui does not, so ferrit adds
`list_offset` itself.

```
   ev.row  (absolute screen row, from crossterm)
     |
     |  - pane_rect.y            strip the pane's screen position
     v
   row within pane rect
     |
     |  - 1                      strip the top border / title row
     v
   inner_row   (0 = first visible list line)
     |
     |  + list_offset[pane]      add rows scrolled off the top
     v
   flat list index
     |
     |  - headers_above(idx)     phase 6+: Staged / Unstaged header rows
     v
   model index  ->  selection[pane]


   pane_rect
   +-- (y)   -------------------------  <- border/title      inner_row -1
   | (y+1) > M src/domain/app/mod.rs               <- list_offset 0     inner_row  0
   | (y+2)   M src/components/screens/mod.rs                                     inner_row  1
   | (y+3)   ? notes.md          <-- click here, ev.row = y+3
   +-- (y+h-1) ----------------------
                                 inner_row = (y+3) - y - 1 = 2
                                 index    = 2 + list_offset[Files]
```

### Click decision

```
                 left click at (ev.column, ev.row)
                              |
              show_help ?  ---yes--> clear overlay, stop
                              | no
              pane_at(ev.column, ev.row) ?    (Rect::contains, top -> bottom)
                   None  \         / Some(pane)
                    no-op          |
                                   v
                        click_pane(pane, ev.row):
                          focus = pane                (always, lazygit step 3)
                          idx = click_row(pane, ev.row)
                             |                    |
                          Some(idx)             None  (border / title row,
                             |                        or past the last row)
                     selection[pane] = idx      keep cursor
                          -> true               -> false
                             |                        |
                             +--------> update_right_pane (step 7), stop

        pane_at == None  ->  click was on the right pane, the command log,
                             the keybar, or an inter-pane gap: no-op.
                             (right pane: hook for the right-pane-focus plan.)
```

Clicking the title bar or the box border of a pane focuses it without moving
the cursor, matching lazygit (click a collapsed panel header to focus it).

## State on `App`

Mirror the phase-4 right-pane tracking, keyed by `Pane`:

```rust
use enum_map::EnumMap;

/// Each left pane's bordered rect from the last frame, for routing a click
/// to the pane it landed in. `Rect::ZERO` before the first draw.
left_areas: EnumMap<Pane, Rect>,
/// `ListState::offset` for each left pane. Ratatui recomputes it every frame
/// from the selection; `draw_left_column` copies it back here **after**
/// `render_stateful_widget` so a click in a scrolled list maps to the right
/// row. Only valid post-render; 0 before the first draw. Not ferrit-driven,
/// see "Out of scope".
list_offset: EnumMap<Pane, usize>,
```

Test / render seams, same shape as `set_right_area` / `set_right_viewport`:

```rust
pub fn set_left_area(&mut self, pane: Pane, area: Rect) { self.left_areas[pane] = area; }
pub fn set_list_offset(&mut self, pane: Pane, off: usize) { self.list_offset[pane] = off; }
```

`draw_left_column` changes from `&App` to `&mut App` (the caller `ui::draw`
already holds `&mut App`). Write the rect **before** reading `pane_lines`, so
the `&mut` borrow for `set_left_area` and the `&` borrow for the list body do
not overlap (gitpane sets `render_area` as the first line of `draw`). Per
pane, per frame:

```rust
app.set_left_area(pane, rows[i]);            // &mut, first

let lines = pane_lines(app, pane);           // &, after
let list = List::new(lines).block(block).highlight_style(...);

let mut state = ListState::default().with_offset(app.list_offset(pane));
if row_ct > 0 { state.select(Some(app.selected(pane).min(row_ct - 1))); }
frame.render_stateful_widget(list, rows[i], &mut state);

app.set_list_offset(pane, state.offset());   // ratatui moved it to keep the
                                             // selection on screen; copy back
```

`draw_right_pane` already writes `right_area`; no change there.

## Routing in `on_mouse`

```rust
use ratatui::layout::Position;

fn on_mouse(&mut self, ev: MouseEvent) {
    match ev.kind {
        MouseEventKind::ScrollDown => return self.wheel(ev, 1),
        MouseEventKind::ScrollUp   => return self.wheel(ev, -1),
        MouseEventKind::Down(MouseButton::Left) => {}
        MouseEventKind::Down(MouseButton::Right) => return, // phase 12: x menu
        _ => return,   // middle click, drag, move: ignored
    }

    if self.show_help {
        self.show_help = false;   // any click dismisses the overlay
        return;
    }

    // lazygit HandleClick, steps 3 / 4 / 5 / 7, folded into click_pane.
    if let Some(pane) = self.pane_at(ev.column, ev.row) {
        self.click_pane(pane, ev.row);
        self.update_right_pane();                        // step 7
    }
    // else: right pane / command log / keybar / gap. no-op.
    // (right pane: hook for the right-pane-focus plan.)
}

/// Which left pane a screen cell is in, `None` for the right pane, the log,
/// the keybar or an inter-pane gap. Shared by the click and the wheel.
fn pane_at(&self, col: u16, row: u16) -> Option<Pane> {
    let p = Position { x: col, y: row };
    PANES.into_iter().find(|&pane| self.left_areas[pane].contains(p))
}

/// Focus `pane`, then move its cursor to the clicked row if that row maps to
/// a real entry. Returns whether the cursor moved: `false` for the border /
/// title row and for a click past the last entry (drydock's `select_at_row`).
/// Mirrors the existing `select(pane, index)` helper.
fn click_pane(&mut self, pane: Pane, screen_row: u16) -> bool {
    self.focus = pane;                                   // step 3: focus first
    let Some(idx) = self.click_row(pane, screen_row) else { return false };
    self.selection[pane] = idx;                          // step 5
    true
}

/// `ViewIndexToModelIndex` for a left pane. `None` = the border/title row or
/// a click past the last entry.
fn click_row(&self, pane: Pane, screen_row: u16) -> Option<usize> {
    let a = self.left_areas[pane];
    let inner_row = screen_row.checked_sub(a.y + 1)? as usize;
    let idx = self.list_offset[pane] + inner_row;
    // phase 6: idx -= self.headers_above(pane, idx);
    (idx < self.row_count(pane)).then_some(idx)
}
```

The phase-4 wheel body moves into `wheel(ev, step)` **unchanged**: column
test against `right_area`, then `scroll_right` over the diff or
`select_down` / `select_up` on the focused pane over the left column. Pure
extraction so `on_mouse` reads as one match. Routing the wheel to the
*hovered* left pane via `pane_at` is a real improvement but a behaviour
change to phase 4, so it is a named follow-up (see "Out of scope"), not part
of this plan.

## Out of scope

- **Right pane as its own focus context.** Click the diff to focus it so
  `j` / `k` / `Ctrl-d` scroll it and `Esc` returns. Needs `App::focus` to
  grow from `Pane` into `Focus { Left(Pane), Right }` plus an escape stack.
  Its own plan, the natural next step after this one and phase 4.
- **Activate on a second click** (gitu's "click the selected row again ->
  Show / stage"). Nothing to activate read-only. Folds into phase 6 (stage a
  file / hunk) and phase 7 (open a commit).
- **Click a diff line** to drop a line cursor there. No line cursor exists
  (phase 4 note); the right pane also soft-wraps, so a screen row is not a
  diff line. Comes with phase 6 line staging.
- **Right-click context menu.** Pairs with the `x` menu, phase 12.
- **Drag the scrollbar thumb**, drag to select, kinetic scroll. lazygit has
  thumb drag; defer with the rest of the mouse polish to phase 12.
- **Clickable keybar / command-log entries** (lazygit makes footer hints
  clickable). Phase 12.
- **Double-click timing.** crossterm has no double-click; it would need a
  last-click timestamp + position. The gitu "click the selected row" gesture
  covers the same need without a timer, so skip it.
- **Wheel to the hovered left pane.** Today (and after this plan) a wheel
  over the left column nudges the *focused* pane. `pane_at` now makes
  "scroll the pane under the pointer" a three-line change in `wheel()`, the
  gitu / gitpane gesture. Deferred only because it changes phase-4
  behaviour; do it with the phase-12 mouse polish or as its own small step.
- **List scroll state as a first-class thing.** `list_offset` is copied back
  off ratatui after render, not driven by ferrit, so it is only valid
  post-render. The clean form is `selection` + `list_offset` collapsing into
  one `EnumMap<Pane, ListState>` on `App`, rendered `&mut` each frame the way
  gitpane keeps its `state: ListState` (offset then just persists, no
  copy-back, no ordering hazard). That refactor touches `selected`,
  `counter`, `right_key_for`, `refresh` and every test, so it is its own
  cleanup (same note as phase 4's `right_scroll`).
- **A `mouse: false` config key.** lazygit's `Gui.MouseEvents` is a master
  toggle (its own comment: captured mouse makes terminal text selection
  harder). Wheel step is `Gui.ScrollHeight: 2` there, hardcoded `3` in
  ferrit (phase 4, matching gitu). Both become config lines in phase 12,
  not here.

## Self-testing (see `PLAN_SELF_TESTING.md`)

`tests/mouse.rs` (new), driving `App::mock()` through `feed_mouse` with the
`set_left_area` / `set_list_offset` seams:

- click at `(col, y)` inside the `Files` rect, row 2 -> `focus == Files`,
  `selected(Files) == list_offset + (y - rect.y - 1)`; the right pane rebuilt.
- click inside `Commits` rect -> focus moves to `Commits`, cursor to that row.
- with `set_list_offset(Files, 5)`, a click on the first list row selects
  index 5, not 0.
- click on the pane's top border row (`y == rect.y`) -> `click_pane` returns
  `false`, focus changed, cursor unchanged.
- click past the last row (empty tail of a short list) -> `click_pane`
  returns `false`, focus changed, cursor unchanged, no out-of-range, no panic.
- click in the command-log rect / the gap below the panes -> `pane_at` is
  `None`, nothing changes.
- `show_help = true`, any click -> `show_help == false` and nothing else moved.
- a `Down(MouseButton::Right)`, a `Down(MouseButton::Middle)` and a `Drag`
  event -> no-op (kind filter).
- phase-4 wheel cases still green (wheel over `right_area` scrolls the diff,
  wheel over the left column nudges the focused pane's selection).

`tests/render.rs`: one `TestBackend` frame, feed a left click on a `Commits`
row, redraw, assert the focused-border style moved from `Status` to
`Commits` and the highlighted row is the clicked one. Existing
`right_pane_follows_focus` stays green.

## Milestones

- **C0** done. `App::left_areas` + `list_offset` (both `EnumMap<Pane, _>`),
  `set_left_area` / `set_list_offset` seams. `draw_left_column` takes
  `&mut App`, writes the rect **before** reading `pane_lines`, and copies
  `ListState::offset()` back after `render_stateful_widget` each frame. No
  behaviour change yet; existing tests green.
- **C1** done. `on_mouse` handles `Down(MouseButton::Left)`: `pane_at()`
  (`Rect::contains`) to route, then `click_pane()` which focuses the pane
  (step 3) and calls `click_row()` (the `ViewIndexToModelIndex` helper) to
  select or, on the border / past-the-tail, returns `false` with the cursor
  put back, then `update_right_pane` (step 7). Wheel body factored into
  `wheel()`, unchanged. `Down(Right)` is an explicit `// phase 12` no-op.
  Help overlay dismissed by a click. `tests/mouse.rs` select / border /
  tail / gap / help / non-left-button cases.
- **C2** done. `tests/render.rs` click-moves-focus frame
  (`click_moves_focus_and_selection_on_screen`, plus `focused_border_rows`
  alongside the existing `selection_bar_rows`). `mock::HELP` gains a
  "click a row" line; `mock::KEYBAR` unchanged (no new key).
- **C3** done. `cargo clippy --all-targets` clean; right / middle click,
  drag and move are no-ops (`only_a_left_click_routes_to_a_pane`); a
  `u16::MAX` click and a `usize::MAX` `list_offset` never panic
  (`extreme_coordinates_and_offsets_never_panic`); a real-frame click on
  the command log or the keybar is a no-op
  (`click_on_the_command_log_or_keybar_is_a_no_op`); `src/domain/git/` still has
  no `ratatui` import; C0..C2 tests green.

## Definition of done (phase 5)

- A left click on a left-pane row focuses that pane and moves its selection
  cursor to the clicked row; the right pane rebuilds exactly as it would for
  a `j` / `k` move.
- A click on a pane's border or title focuses the pane without moving the
  cursor. A click past the last row, on the command log, or in a gap does
  nothing. A click anywhere dismisses the help overlay.
- A click in a scrolled `Files` / `Commits` list lands on the right row.
- Right-click, middle-click, drag and mouse-move are inert.
- No click path panics on an empty pane, a short list, or `App::mock()`.
- `cargo clippy --all-targets` clean; `tests/mouse.rs` and `tests/render.rs`
  pass.

## After phase 5

The right-pane-as-focus-context plan (Enter or a click to focus the diff,
`Esc` to return, `j` / `k` scroll it while focused) is the next step and the
last piece that makes ferrit's navigation match lazygit's. This plan already
leaves the hook: the `pane_at(...) == None` branch in `on_mouse` (currently a
bare no-op) tests `right_area` and focuses the right pane, once `App::focus`
grows a `Right` arm.
