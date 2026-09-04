# Plan: phase 1, layout only

## Goal

Reproduce the lazygit screen in a terminal, with **no git and no features**.
Static mock data, keyboard navigation between panes, nothing else. This phase
exists to nail the layout, the focus model, and the render loop before any git
code lands.

## Target screen

```
┌─ 1 Status ────────────┐┌─ Diff / main panel ──────────────────────────────┐
│ ferrit → main ↑2 ↓0   ││                                                  │
│ ✓ no merge conflicts  ││  diff --git a/src/main.rs b/src/main.rs           │
└───────────────────────┘│  @@ -1,3 +1,7 @@                                  │
┌─ 2 Files ─────────────┐│  -fn main() {                                     │
│  M src/main.rs        ││  +fn main() -> Result<()> {                       │
│  ?? docs/PLAN.md      ││  +    let repo = git::open(".")?;                 │
│  A  Cargo.lock        ││       println!("ferrit");                         │
│                       ││  +    Ok(())                                     │
│                       ││   }                                              │
└───────────────────────┘│                                                  │
┌─ 3 Local Branches ────┐│  (contenu = ce que la pane FOCUS de gauche       │
│ * main                ││   veut montrer: diff, log du commit, contenu     │
│   feat/tui-skeleton   ││   du stash, detail de branche...)                │
│   fix/parse-args      ││                                                  │
└───────────────────────┘│                                                  │
┌─ 4 Commits ───────────┐│                                                  │
│ 5e04050 docs: expand… ││                                                  │
│ 23023d9 docs: add ins…││                                                  │
│ 2f9bd4f docs: drop ar…││                                                  │
│ d5bc03c chore: initial││                                                  │
└───────────────────────┘│                                                  │
┌─ 5 Stash ─────────────┐│                                                  │
│ (vide)                ││                                                  │
└───────────────────────┘└──────────────────────────────────────────────────┘
┌─ command log ────────────────────────────────────────────────────────────┐
│ $ git status --porcelain                                                  │
│ $ git diff src/main.rs                                                     │
└──────────────────────────────────────────────────────────────────────────┘
 <space> stage  <c> commit  <P> push  <p> pull  <?> keybinds  <q> quit
```

In phase 1 the command log lines and the diff text are **hardcoded strings**.
Nothing is computed.

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
- Config file, themes, colours beyond a basic focused/unfocused distinction.
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
    ├── Length(28)    left column    ── fixed width, like lazygit's default
    └── Min(0)        right pane     ── takes the rest

left column
└── Layout::vertical
    ├── Length(4)     1 Status       ── header, always small
    ├── Min(3)        2 Files
    ├── Min(3)        3 Local Branches
    ├── Min(3)        4 Commits
    └── Length(4)     5 Stash
```

Later enhancement (not phase 1): accordion behaviour, where the focused left
pane grows and the others shrink to their title line, like lazygit.

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
(`<space> stage`, `<c> commit`, ...) as **inert labels**, so the screen looks
right; those keys do nothing yet.

## Right pane content by focus (all mock)

| Focus | Right pane shows |
| --- | --- |
| Status | short repo summary block (branch, ahead/behind, clean) |
| Files | a sample `git diff` snippet |
| Branches | a sample `git log --oneline --graph` snippet |
| Commits | a sample commit: header + diff |
| Stash | `(no stash entries)` |

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
