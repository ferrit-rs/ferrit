# Plan: phase 4, right-pane scroll behaviour

## Goal

Make the right pane (the diff / `git show` view from phase 3) scrollable the
way lazygit's main view is: dedicated scroll keys that do not fight the
left-pane selection, mouse wheel, a visible scrollbar, and a clamp that stops
at the real bottom instead of scrolling the content off screen.

Still read only. Still one focused left pane; the right pane stays a
"follower" view, not its own focusable context (that is "Out of scope").

## The bug this fixes

Today `j` / `k` / `Down` / `Up` always move the selection cursor of the
focused **left** pane. That is correct, lazygit does the same. But there is
no obvious key that scrolls the **right** pane, so pressing `j` in front of a
long diff scrolls the commit list instead of the diff. `Ctrl-d` / `Ctrl-u`
(phase 3) do scroll the right pane, but nothing advertises them and there is
no scrollbar to show there is more content.

## Approach: do what lazygit does

lazygit keeps the side panel focused and gives the main view its own scroll
keybindings (`../ferrit-references/tui/lazygit/pkg/config/user_config.go`
defaults):

```
scrollUpMain    : <pgup>  K  <ctrl+u>
scrollDownMain  : <pgdown> J  <ctrl+d>
gotoTop / gotoBottom (main): < / >
mouse wheel over the main view
```

Lowercase `j` / `k` stay selection. Uppercase `J` / `K` and the page keys
scroll the main view **without leaving the side panel**. A scrollbar is drawn
on the main view whenever its content overflows.

`../ferrit-references/tui/lazyjj/src/ui/panel/details_panel.rs` is the
ratatui version of the same thing and the model to copy:

- the panel remembers its last rendered `Rect` (`panel_rect`) so mouse events
  and the clamp know the viewport height;
- it renders `Scrollbar::new(ScrollbarOrientation::VerticalRight)` with a
  `ScrollbarState::new(total_lines).position(scroll)` over the border, but
  **only when `total_lines > inner.height`**;
- scroll ops are an enum (`ScrollDown`, `ScrollUp`, `ScrollDownHalfPage`,
  `ScrollDownPage`, ...) applied to one `scroll: u16` field, clamped to
  `total_lines - viewport`.

`../ferrit-references/tui/gitu/src/app.rs` `handle_mouse_input` shows the
crossterm side: match `MouseEventKind::ScrollUp` / `ScrollDown`, move the
view by a small fixed number of lines.

## Key routing

The right-pane scroll keys are live **only** while the right pane shows a
real diff (`DiffView::Files` / `DiffView::Commit` with at least one file),
exactly the phase-3 rule. Inert over an image, a `Note`, or the mock bodies.

| Key | Action | Status |
| --- | --- | --- |
| `J` / `K` | right pane down / up one line | new |
| `<pgdn>` / `<pgup>` | right pane down / up one page (`viewport - 1`) | new |
| `Ctrl-d` / `Ctrl-u` | right pane down / up half a page | phase 3, keep |
| `<` / `>` | right pane to top / bottom | new |
| `]` / `[` | next / prev hunk (Files) or file (Commit) | phase 3, keep |
| `j` / `k` / arrows | left-pane selection | unchanged |

`on_key`: a `match` on the scroll keys runs **before** the selection `match`
and `return`s, so a scroll keystroke never falls through to
`update_right_pane()` and never re-runs `git`. Half-page / page steps are
derived from `App::right_viewport` (see below), not a fixed constant, now
that the pane height is tracked; `Ctrl-d`'s phase-3 `RIGHT_HALF_PAGE`
constant goes away.

## Viewport tracking

`draw_right_pane` already takes `&mut App`. It writes the diff block's inner
height back every frame, lazyjj-style:

```rust
// in draw_right_pane, for the DiffView::Files | Commit arm
let inner = block.inner(area);
app.set_right_viewport(inner.height as usize); // stores usize, 0 before first draw
```

`App` gains:

```rust
right_viewport: usize,   // inner height of the diff pane, last frame
right_area: Rect,        // whole right-pane rect, last frame (mouse routing)
```

Clamp becomes viewport-aware:

```rust
fn max_right_scroll(&self) -> usize {
    self.diff_line_count().saturating_sub(self.right_viewport.max(1))
}
```

So the last line lands at the bottom of the pane and cannot scroll past it.
Before the first draw `right_viewport == 0`, the clamp is permissive by one
screen; the next frame corrects it. Matches lazyjj (it just tracks the last
rect, no special first-frame case).

`]` / `[` still jump `right_scroll` to an anchor line, then the same clamp
applies, so jumping to the last hunk of a short tail does not leave a blank
screen below it.

## Scrollbar

```rust
use ratatui::widgets::{Scrollbar, ScrollbarOrientation, ScrollbarState};

// after render_widget(paragraph, area), same arm:
let viewport = inner.height as usize;
if total > viewport {
    // `ScrollbarState::content_length` is the count of distinct scroll
    // positions (`total - viewport + 1`), not the raw line count: sizing
    // the thumb against the raw total leaves it one cell short of the
    // track's end at max scroll. `viewport_content_length` must also be
    // set, or the thumb is sized against `area.height` instead.
    let max_scroll = total - viewport;
    let mut sb = ScrollbarState::new(max_scroll + 1)
        .position(scroll.min(max_scroll))
        .viewport_content_length(viewport);
    frame.render_stateful_widget(
        Scrollbar::new(ScrollbarOrientation::VerticalRight)
            .begin_symbol(None)
            .end_symbol(None),
        area.inner(Margin { vertical: 1, horizontal: 0 }),
        &mut sb,
    );
}
```

`area.inner(Margin { vertical: 1, .. })` keeps the track between the border
corners. Drawn only on overflow, like lazyjj and lazygit. `begin_symbol(None)`
/ `end_symbol(None)` drop the arrow glyphs (and the track space they'd
reserve) for a plain track + thumb, lazygit style. The `Note` / mock bodies
do not get one (they do not overflow in practice; revisit if that changes).
The same `content_length = max_scroll + 1` shape and arrow-less symbols are
reused for the left-column list scrollbars (Status, Files, Branches,
Commits, Stash), coloured like the pane's own border (green when focused,
grey otherwise) — a phase-4 follow-up beyond this plan's original left-pane
scope, but the same overflow-only scrollbar mechanism. No theming knob
beyond that border colour; a fuller palette entry is phase 12.

## Mouse wheel

`tui::init` gains `EnableMouseCapture`, `tui::restore` gains
`DisableMouseCapture` (both from `ratatui::crossterm::event`). The panic hook
already routes through `restore`, so a panic still releases the mouse.

`App::run` currently drops every non-key `Event`. Add:

```rust
AppEvent::Input(Event::Mouse(m)) => self.on_mouse(m),
```

`on_mouse`:

```
MouseEventKind::ScrollDown | ScrollUp:
    if column is inside self.right_area and the right pane is a real diff:
        scroll_right(±WHEEL_LINES)          // 3, matching gitu's default
    else:
        move the focused left-pane selection by ±1
other kinds: ignored (no click-to-select yet)
```

Column test: `self.right_area.x <= m.column < self.right_area.x + self.right_area.width`.
Wheel over the left column moving the selection is a small bonus lazygit also
does; drop it if it complicates the diff test.

Enabling mouse capture also means the terminal no longer does native
text selection with the mouse. lazygit accepts this; ferrit does too (a
future `mouse: false` config key is a phase-12 line, not phase 4).

## Keybar and help

`mock::KEYBAR` and `mock::HELP` gain the right-pane keys:

```
KEYBAR: ... | Scroll diff: J/K | Hunk: ]/[ | ...
HELP:  adds
  J / K            scroll the diff pane
  PgUp / PgDn      scroll the diff pane a page
  Ctrl-u / Ctrl-d  scroll the diff pane half a page
  < / >            diff pane to top / bottom
  ] / [            next / previous hunk (or file, in a commit)
```

Both are still static text (dynamic, context-aware keybar is a later phase).

## Out of scope

- **The right pane as its own focus context.** lazygit lets you focus the
  main view (Enter on Files/Commits, or a click) so `j` / `k` / `Ctrl-d`
  scroll it and `Esc` returns. That needs `App::focus` to grow from `Pane`
  into something like `Focus { Left(Pane), Right }`, plus an escape stack.
  Worth doing, its own plan. Phase 4 keeps the left pane always focused and
  the right pane a follower.
- **Line-level selection cursor** in the diff (needed for phase 6 staging)
  and keeping that cursor on screen while scrolling (gitui `VerticalScroll`).
  Phase 4's `right_scroll: usize` has no cursor.
- **Horizontal scroll** of un-wrapped lines. Phase 3 soft-wraps; unchanged.
- **Search in the diff** (`/`), fold / unfold hunks, `space` to stage.
- **Scroll state for the mock / `Note` bodies**, and a scrollbar on them.
- **Click to select** a left-pane row or a diff line (`gitu`
  `MoveToScreenLine`). Only the wheel is wired in phase 4.
- **Kinetic / smooth scroll, configurable wheel speed.** Fixed 3 lines.

## Self-testing (see `PLAN_SELF_TESTING.md`)

- `tests/diff_app.rs` (extend): open a fixture repo with a tall diff, then
  - `J` * n moves `right_scroll` by n, clamped to `line_count - viewport`;
  - `>` goes to `max_right_scroll()`, `<` back to 0;
  - `<pgdn>` moves by `viewport - 1`;
  - a `MouseEventKind::ScrollDown` at a column inside `right_area` scrolls the
    diff; the same event at a left-column column moves the selection;
  - a 1-line diff and an empty diff: every scroll key is a no-op, no panic,
    no underflow.
  `App` needs a test seam for the viewport (`set_right_viewport`) and for
  feeding a synthetic `MouseEvent` (`on_mouse` is already `pub(crate)` via
  the test module, like `on_key`).
- `tests/scrollbar.rs` (own file, not `diff_app.rs`): one `TestBackend`
  frame with a diff taller than the pane asserts a solid thumb glyph column
  at the right edge of `right_area` and that it reaches the track's exact
  top/bottom at min/max scroll; one frame with a short diff asserts none.
  Same coverage for each left-column pane, plus a green-vs-grey thumb
  colour check for focused vs. unfocused.
- Existing `tests/render.rs` cases (`right_pane_follows_focus`,
  `image_selection_takes_over_the_right_pane`) must stay green: the mock
  bodies and the image path draw no scrollbar and ignore the scroll keys.

## Milestones

- **S0** `App::right_viewport` + `set_right_viewport`, written from
  `draw_right_pane`. Viewport-aware clamp (`max_right_scroll`). `J` / `K`,
  `<pgup>` / `<pgdn>`, `<` / `>`; `Ctrl-d` / `Ctrl-u` re-expressed in terms
  of the viewport; `RIGHT_HALF_PAGE` deleted. `tests/diff_app.rs` key cases.
- **S1** `Scrollbar` on the `DiffView::Files` / `::Commit` arm, overflow only,
  arrow-less (`begin_symbol(None)` / `end_symbol(None)`), thumb reaching the
  track's exact ends via `content_length = max_scroll + 1`. Same scrollbar on
  every left-column pane, coloured by the pane's border style. `mock::KEYBAR`
  / `mock::HELP` updated. `tests/scrollbar.rs` assertions (present on
  overflow, absent when it fits, thumb touches both track ends, green when
  focused).
- **S2** `EnableMouseCapture` / `DisableMouseCapture` in `tui`. `App::run`
  handles `Event::Mouse`; `App::on_mouse` routes wheel by column
  (`App::right_area`). `tests/diff_app.rs` wheel cases.
- **S3** polish: `cargo clippy --all-targets` clean, no warnings; scroll /
  wheel on empty, one-line, and huge diffs never panic; `src/domain/git/` still has
  no `ratatui` import; all S0..S2 tests green.

## Definition of done (phase 4)

- With a long diff in the right pane, `J` / `K`, `PgUp` / `PgDn`, `Ctrl-d` /
  `Ctrl-u`, `<` / `>` and the mouse wheel scroll it; `j` / `k` still move the
  left-pane selection.
- Scrolling stops with the last line at the bottom of the pane, never past
  it; an empty or one-line diff is a no-op, never a panic.
- A scrollbar shows on the right pane exactly when the diff overflows, its
  thumb tracks `right_scroll` and touches both track ends at min/max scroll,
  and it draws no arrow glyphs. Every left-column pane gets the same
  treatment when its list overflows, thumb green while focused.
- The scroll keys and the wheel are inert over an image, a `Note`, and the
  mock bodies.
- `mock::KEYBAR` / `mock::HELP` list the new keys.
- `cargo clippy --all-targets` clean; `tests/diff_app.rs`, `tests/scrollbar.rs`
  and `tests/render.rs` pass.

## After phase 4

The right-pane-as-focus-context work (Enter to focus the diff, `Esc` to
return, `j` / `k` scroll it while focused) is the natural next step and what
makes ferrit's navigation match lazygit's exactly. It pairs well with phase
5: once the diff has a line cursor for staging, focusing the pane and moving
that cursor is the same gesture lazygit uses. Keep `right_scroll` a plain
`usize` until then, then fold it into a gitui-style `VerticalScroll` that
also keeps the cursor line on screen.
