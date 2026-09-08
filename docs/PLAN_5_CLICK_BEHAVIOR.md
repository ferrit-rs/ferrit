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

Two references, same gesture.

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

ferrit phase 5 keeps **steps 1 to 5 and 7**:

- step 1: `ViewIndexToModelIndex`, see "What a click has to resolve" below.
- step 2 + 3: focus the pane before touching the selection. A click anywhere
  in a pane's rect focuses it, even the border or the empty tail.
- step 4: click past the last row is focus-only, cursor unmoved. No panic,
  no clamp-to-last surprise.
- step 5: move `selection[pane]`.
- step 7: `update_right_pane()`, ferrit's `HandleFocus` equivalent.

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
   |  M src/app.rs   <----+-- click row     |   click here (col >= side):
   |  M src/ui.rs         |                 |     -> right pane. no-op for now
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
   | (y+1) > M src/app.rs               <- list_offset 0     inner_row  0
   | (y+2)   M src/ui.rs                                     inner_row  1
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
             loop panes top -> bottom
                              |
        hit-test ev inside left_areas[pane] ?
                     no  \         / yes
                    next pane      |
                                   v
                          focus = pane          (always, lazygit step 3)
                                   |
                 ev.row == pane_rect.y  (border / title row) ?
                        yes /                 \ no
                  keep cursor            index = map(ev.row)   (schema above)
                  update_right_pane            |
                        stop            index < row_count(pane) ?
                                          yes /            \ no  (empty tail,
                                             |              \     past last row)
                                   selection[pane] = index   keep cursor
                                             |                    |
                                        update_right_pane  <------+
                                             stop

        no pane hit  ->  click was on the right pane, the command log,
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
/// `ListState::offset` for each left pane, read back after render so a click
/// in a scrolled list maps to the right row. 0 before the first draw.
list_offset: EnumMap<Pane, usize>,
```

Test / render seams, same shape as `set_right_area` / `set_right_viewport`:

```rust
pub fn set_left_area(&mut self, pane: Pane, area: Rect) { self.left_areas[pane] = area; }
pub fn set_list_offset(&mut self, pane: Pane, off: usize) { self.list_offset[pane] = off; }
```

`draw_left_column` changes from `&App` to `&mut App` (the caller `ui::draw`
already holds `&mut App`). Per pane, per frame:

```rust
app.set_left_area(pane, rows[i]);

let mut state = ListState::default().with_offset(app.list_offset(pane));
if row_ct > 0 { state.select(Some(app.selected(pane).min(row_ct - 1))); }
frame.render_stateful_widget(list, rows[i], &mut state);

app.set_list_offset(pane, state.offset());   // ratatui adjusted it to keep
                                             // the selection on screen
```

`draw_right_pane` already writes `right_area`; no change there.

## Routing in `on_mouse`

```rust
fn on_mouse(&mut self, ev: MouseEvent) {
    // wheel: unchanged from phase 4
    match ev.kind {
        MouseEventKind::ScrollDown => return self.wheel(ev, 1),
        MouseEventKind::ScrollUp   => return self.wheel(ev, -1),
        MouseEventKind::Down(MouseButton::Left) => {}
        _ => return,   // right / middle click, drag, move: ignored for now
    }

    if self.show_help {
        self.show_help = false;   // any click dismisses the overlay
        return;
    }

    // left panes, top to bottom (lazygit HandleClick, steps 3 / 4 / 5 / 7)
    for pane in PANES {
        let a = self.left_areas[pane];
        if !hit(a, ev.column, ev.row) { continue; }
        self.focus = pane;                               // step 3: focus first
        if let Some(idx) = self.click_row(pane, ev.row) { // steps 1 + 4
            self.selection[pane] = idx;                  // step 5
        }
        self.update_right_pane();                        // step 7
        return;
    }

    // right pane: no-op until the right-pane-focus plan. wheel still works.
}

fn hit(a: Rect, col: u16, row: u16) -> bool {
    col >= a.x && col < a.x + a.width && row >= a.y && row < a.y + a.height
}

/// `ViewIndexToModelIndex` for a left pane. `None` = the border/title row or
/// a click past the last entry (focus only, keep the cursor).
fn click_row(&self, pane: Pane, screen_row: u16) -> Option<usize> {
    let a = self.left_areas[pane];
    let inner_row = (screen_row.checked_sub(a.y + 1)?) as usize;
    let idx = self.list_offset[pane] + inner_row;
    // phase 6: idx -= self.headers_above(pane, idx);
    (idx < self.row_count(pane)).then_some(idx)
}
```

The existing wheel body moves into `wheel(ev, step)` unchanged (column test
against `right_area`, `scroll_right` vs `select_down` / `select_up`).

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
- **List scroll state as a first-class thing.** `list_offset` is read back
  off ratatui, not driven by ferrit. A real per-pane `VerticalScroll` is a
  later cleanup (same note as phase 4's `right_scroll`).
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
- click on the pane's top border row (`y == rect.y`) -> focus changes, cursor
  unchanged.
- click past the last row (empty tail of a short list) -> focus changes,
  cursor unchanged, no out-of-range, no panic.
- click in the command-log rect / the gap below the panes -> nothing changes.
- `show_help = true`, any click -> `show_help == false` and nothing else moved.
- a `Down(MouseButton::Right)` and a `Drag` event -> no-op (kind filter).
- phase-4 wheel cases still green (wheel over `right_area` scrolls the diff,
  wheel over the left column nudges the selection).

`tests/render.rs`: one `TestBackend` frame, feed a left click on a `Commits`
row, redraw, assert the focused-border style moved from `Status` to
`Commits` and the highlighted row is the clicked one. Existing
`right_pane_follows_focus` stays green.

## Milestones

- **C0** `App::left_areas` + `list_offset` (both `EnumMap<Pane, _>`),
  `set_left_area` / `set_list_offset` seams. `draw_left_column` takes
  `&mut App`, writes the rect and reads back `ListState::offset()` each
  frame. No behaviour change yet; existing tests green.
- **C1** `on_mouse` handles `Down(MouseButton::Left)` in lazygit
  `HandleClick` order: hit-test the left panes, focus the pane, then
  `click_row()` (the `ViewIndexToModelIndex` helper) to select or, on the
  border / past-the-tail, not, then `update_right_pane`. Wheel body factored
  into `wheel()`, unchanged. Help overlay dismissed by a click.
  `tests/mouse.rs` select / border / tail / out-of-bounds / help cases.
- **C2** `tests/render.rs` click-moves-focus frame. `mock::HELP` gains a
  "click a row to select it" line; `mock::KEYBAR` unchanged (no new key).
- **C3** polish: `cargo clippy --all-targets` clean; right / middle click,
  drag and move are no-ops; clicks on every gap (log, keybar, inter-pane
  border, past the last row) never panic; `src/git/` still has no `ratatui`
  import; C0..C2 tests green.

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
leaves the hook: the `on_mouse` "right pane: no-op" branch becomes "focus the
right pane", once `App::focus` grows a `Right` arm.
