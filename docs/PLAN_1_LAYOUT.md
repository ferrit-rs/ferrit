# Plan: phase 1, layout only

## Goal

Reproduce the lazygit screen in a terminal, with **no git and no features**.
Static mock data, keyboard navigation between panes, nothing else. This phase
exists to nail the layout, the focus model, and the render loop before any git
code lands.

## Target screen

Match lazygit's default look: panel titles carry the number in `[N]` and list
their inert sibling tabs; each list panel shows a bottom-right `N of M` counter;
Status is one line `repo → branch ↑ahead`; the focused panel border and title
are green; the selected row is a solid blue bar; the right panel title is
contextual (`Unstaged changes` for Files, `Log`, `Commit`, `Stash`); the keybar
reads `Label: key` with the keys in yellow.

```
┌ [1] Status ───────────┐┌ Unstaged changes ────────────────────────────────┐
│ ferrit → main ↑2      ││  diff --git a/src/main.rs b/src/main.rs           │
└───────────────────────┘│  @@ -1,3 +1,7 @@                                  │
┌ [2] Files - Worktrees ┐│  -fn main() {                                     │
│  M src/main.rs         ││  +fn main() -> Result<()> {                       │
│ ?? docs/notes.md       ││  +    let repo = git::open(".")?;                 │
│ A  Cargo.lock          ││       println!("ferrit");                         │
│                        ││  +    Ok(())                                     │
│                1 of 3 ─┘│   }                                              │
┌ [3] Local branches -  ┐│                                                  │
│ * main ↑2              ││  right panel = what the focused left panel wants │
│   feat/tui-skeleton    ││  to show: file diff, commit patch, stash diff,  │
│                1 of 3 ─┘│  branch log. Hardcoded strings in phase 1.       │
┌ [4] Commits - Reflog  ┐│                                                  │
│ 5e04050 MW o docs: ex… ││                                                  │
│ 23023d9 MW o docs: ad… ││                                                  │
│ 2f9bd4f MW o docs: dr… ││                                                  │
│ d5bc03c MW o chore: i… ││                                                  │
│                1 of 4 ─┘│                                                  │
┌ [5] Stash ────────────┐│                                                  │
│ (no stash entries)     ││                                                  │
└───────────────────────┘└──────────────────────────────────────────────────┘
┌ command log ─────────────────────────────────────────────────────────────┐
│ $ git status --porcelain                                                  │
│ $ git diff src/main.rs                                                     │
└──────────────────────────────────────────────────────────────────────────┘
 Stage: <space> | Commit: c | Push: P | Pull: p | Keybindings: ? | Quit: q
```

In phase 1 the command log lines and the diff text are **hardcoded strings**.
Nothing is computed.

## Palette

One flat palette, tuned to lazygit's defaults. Lives in `theme.rs`, no config
yet (that is phase 12).

| Role | Colour | lazygit name |
| --- | --- | --- |
| Focused panel border + title | green, bold | `activeBorderColor` |
| Unfocused panel border | gray | `inactiveBorderColor` |
| Selected row | white on blue, bold | `selectedLineBgColor` |
| Commit hash + graph node | green | |
| Author initials | magenta | |
| Added line / checked-out branch | green | |
| Removed line / deleted path | red | |
| Hunk header `@@` | cyan | |
| Ahead/behind counts, `N of M` counter | gray/yellow | |
| Keybar key names | yellow | |

## Commit row + graph column

Each Commits row is `<hash8> <initials> <graph-node> <subject>`:

```
5e04050 MW o docs: expand the layout plan
│       │  │ └ commit subject, plain
│       │  └ graph-column glyph
│       └ author initials, magenta (up to 2, uppercased)
└ abbreviated hash, green
```

Graph glyphs: `o` a commit on the current line, `│` a passing lane, `├` `─` a
merge join. Phase 1 draws a static `o` for every row (linear history). The real
graph, computed by walking the commit DAG, lands in phase 2 milestone G4;
`theme::commit_line` already takes the glyph as a parameter so wiring it later
touches only the backend.

## In scope

- ratatui + crossterm app that boots, draws the screen above, restores the
  terminal cleanly on exit.
- Left column: 5 bordered panes, numbered `1`..`5`.
- Right column: one large pane whose content changes with the focused left pane.
- Bottom: command-log box + one-line keybind bar.
- Focus model: exactly one left pane focused, its border highlighted.
- Keyboard: `1`-`5` and `Tab` / `Shift-Tab` switch focus, `j`/`k` (and arrows)
  move a selection cursor inside the focused list, `q` / `Ctrl-c` quits,
  `?` toggles a help overlay.
- Lists scroll when longer than their pane height.
- Resizes without panicking, no horizontal overflow.

## Out of scope (later phases)

- Any real git: `git2` / `gitoxide`, status, diff, staging, commit, push, pull,
  branch checkout, stash, rebase.
- Config file, theme switching. The fixed lazygit-style palette above is in
  scope; user-overridable colours are not.
- Mouse support.
- Input popups (commit message, confirm dialogs).
- Async / background workers.
- Real command-log capture.
- Telemetry: a TUI has no PostHog. If we ever want anonymous opt-in usage
  counts, discuss separately before adding anything.

## Dependencies

```toml
[dependencies]
ratatui = "0.30"                                   # check crates.io for latest 0.30.x
crossterm = "0.29"                                 # or use ratatui::crossterm re-export
clap = { version = "4", features = ["derive"] }
color-eyre = "0.6"                                 # pretty panics + error report
```

Notes:
- ratatui 0.30+ is a modular workspace. `crossterm` is re-exported as
  `ratatui::crossterm`; a separate `crossterm` dep is optional.
- `color-eyre` is what the ratatui templates use. `anyhow` is a fine swap.

## Module layout

```
src/
├── main.rs          entry point
│                      - parse args with clap: --version, -p/--path <dir>
│                      - install color-eyre
│                      - tui::init()  ->  run(app)  ->  tui::restore()
├── tui.rs           terminal plumbing
│                      - init(): enable raw mode, enter alt screen, hide cursor
│                      - restore(): exact reverse, also run from a panic hook
│                      - type alias Tui = Terminal<CrosstermBackend<Stdout>>
├── app.rs           application state + logic
│                      - struct App { focus, selection, show_help, should_quit }
│                      - App::new(), App::update(&mut self, event)
│                      - focus_next / focus_prev / select_up / select_down
├── event.rs         (optional) crossterm event  ->  small AppEvent enum
├── mock.rs          hardcoded sample data
│                      - status header lines, files, branches, commits, stash
│                      - command_log lines, sample diff / log text
└── ui/
    ├── mod.rs       draw(frame, &app): builds the top-level Layout
    ├── panes.rs     left column: the 5 bordered List widgets
    ├── main_view.rs right pane: match app.focus  ->  render a mock Paragraph
    ├── command_log.rs   bottom log box
    └── status_bar.rs    bottom one-line keybind bar
```

For a first cut this can collapse to `main.rs`, `tui.rs`, `app.rs`, `ui.rs`,
`mock.rs`. Split into the tree above once `ui.rs` gets awkward.

## Layout math (ratatui)

```
frame area
└── Layout::vertical
    ├── Min(0)        content        ── the two columns
    ├── Length(4)     command log    ── bordered box, 2 lines of text
    └── Length(1)     keybind bar    ── plain line, no border

content
└── Layout::horizontal
    ├── Length(w/3)   left column    ── a third of the width, min 24,
    │                                  matches lazygit sidePanelWidth 0.3333
    └── Min(0)        right pane     ── takes the rest

left column
└── Layout::vertical
    ├── Length(status_lines().len() + 2)   [1] Status ── 3 normally, 4 with
    │                                        a conflict line; never accordions
    └── Min(0)        accordion_area     ── [2]..[5], split by hand below

accordion_area (lazygit-style `expandFocusedSidePanel`, hand-computed, not a
Layout constraint solve: mixing Min/Fill in one `Layout::vertical` call is
order-sensitive and can starve the boosted pane below its neighbours at small
heights)
    FOCUS_WEIGHT = 4 shares to the focused pane, 1 share to each other pane
    each pane's height = accordion_area.height * its_weight / total_weight,
      floored at MIN_HEIGHT = 2 (a collapsed but still-bordered box: no room
      for a content row, but still recognisable, unlike a 1-row sliver)
    if focus is Status (outside [2]..[5]): every pane gets 1 share (an even
      split, not left blank)
    a round-robin correction pass afterwards fixes the rounding so the four
      heights always sum to exactly accordion_area.height

    Revision: an earlier version gave unfocused panes a fixed FLOOR (3) and
    100% of the leftover to focus, which fell back to "split area.height
    evenly over the 4" whenever there wasn't room for every pane's floor —
    erasing the accordion in exactly the short-terminal case where showing
    one pane clearly matters most (confirmed against a real lazygit
    screenshot at a comparable terminal height: lazygit's focused pane still
    dominated, ferrit's four panes came out nearly equal). The weighted
    scheme degrades gracefully at any height instead of falling off a cliff.
```

Example at a typical 80x24 terminal, Commits focused:

```
┌ [1] Status ───────────┐  3  (dynamic: no conflict line)
├ [2] Files ─────────────┤  3  (1 share)
├ [3] Local branches ────┤  2  (1 share, MIN_HEIGHT floor)
├ [4] Commits ───────────┤  9  (4 shares)
└ [5] Stash ─────────────┘  2  (1 share, MIN_HEIGHT floor)
```

Implemented (not deferred): `draw_left_column` in `src/ui.rs`.

## State model

```rust
#[derive(Clone, Copy, PartialEq, Eq)]
enum Pane { Status, Files, Branches, Commits, Stash }

const PANES: [Pane; 5] =
    [Pane::Status, Pane::Files, Pane::Branches, Pane::Commits, Pane::Stash];

struct App {
    focus: Pane,
    selection: [usize; 5],   // one cursor per left pane, indexed by pane order
    show_help: bool,
    should_quit: bool,
}
```

`mock.rs` owns the data. `App` only owns navigation state.

## Data flow

```
   ┌───────────────────────── loop ─────────────────────────┐
   │                                                        │
crossterm event ─▶ App::update(event) ─▶ mutate App ─▶ ui::draw(frame, &app)
   ▲                     │                                  │
   │                     └─ sets should_quit on q / Ctrl-c  │
   │                                                        ▼
   └──────────────────────── terminal ◀─────────────────────┘

loop: draw, then block on `event::read()`, then update, until should_quit.
No tick, no polling, no async in phase 1.
```

## Keybindings (phase 1 only)

| Key | Action |
| --- | --- |
| `1` `2` `3` `4` `5` | focus that left pane |
| `Tab` / `Shift-Tab` | focus next / previous left pane |
| `j` / `Down` | move selection down in focused pane (clamped) |
| `k` / `Up` | move selection up in focused pane (clamped) |
| `?` | toggle help overlay |
| `q` / `Ctrl-c` | quit |

Every other key is ignored. The keybind bar shows the lazygit set
(`Stage: <space>`, `Commit: c`, ...) as **inert labels**, so the screen looks
right; those keys do nothing yet.

## Right pane content by focus (all mock)

| Focus | Panel title | Right pane shows |
| --- | --- | --- |
| Status | `Status` | short repo summary block (branch, ahead/behind, clean) |
| Files | `Unstaged changes` | a sample `git diff` snippet, or the image preview when the selected path is an image |
| Branches | `Log` | a sample `git log --oneline --graph` snippet |
| Commits | `Commit` | a sample commit: header + diff |
| Stash | `Stash` | `(no stash entries)` |

## Milestones

- **M0** deps added, `tui::init` / `restore`, blank alt-screen, `q` quits,
  panic hook restores the terminal.
- **M1** static layout renders: 5 left boxes + right box + command log +
  keybar, proportions match the target, clean on resize.
- **M2** focus switching (`1`-`5`, `Tab`), focused border highlighted.
- **M3** mock lists render with a highlighted row, `j`/`k` moves it, clamped
  at both ends, list scrolls when taller than its pane.
- **M4** right pane swaps mock content based on `app.focus`.
- **M5** polish: status title bar text, `?` help overlay, no horizontal
  overflow at narrow widths, `cargo clippy` clean.
- **M6** lazygit skin: the palette table above in `theme.rs`, `[N] Tab - Tab`
  panel titles, bottom-right `N of M` counters, Status collapsed to one line,
  blue selection bar, `Label: key` keybar, `<hash> <initials> o <subject>`
  commit rows. Typed `CommitEntry` / `BranchEntry` / `StashEntry` in
  `src/git/model.rs` so G3..G5 swap the source without touching the UI.

## Definition of done (phase 1)

- `cargo run` from any directory shows the target screen.
- Keyboard navigation works as in the table above.
- Terminal is always restored on exit, including on panic.
- `cargo clippy --all-targets` is clean.
- Resizing the terminal never panics and never overflows horizontally.
- No `git` anywhere in the code or `Cargo.toml`.

## After phase 1

Phase 2 replaces `mock.rs` one pane at a time with a real backend, starting
with Status + Files via `git2` (read-only). See `docs/INSPIRATION.md` for the
backend options and the "clean split: git backend crate <-> TUI crate" goal.
