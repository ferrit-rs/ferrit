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
| Status | `Status` | short repo summary block (branch, ahead/behind, clean) — superseded by the welcome screen below, never built as written here |
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

## Welcome screen (Status, real repo) — done

lazygit's right pane, when Status is focused, is not a status summary at
all: it's a static welcome screen — a big ASCII wordmark, the tagline,
version, licence, and a keybindings pointer. ferrit never built this. The
"Right pane content by focus" table above planned a real `git status`-style
summary for that slot instead, which was never implemented either — on a
real repo (`!app.is_mock()`), `draw_right_pane` (`src/ui.rs`) falls straight
through to a blank `Paragraph::new("")` the moment Status is focused,
because `right_key_for` has no arm for `Pane::Status` (there is nothing to
diff). That blank pane is the actual gap this closes; the mock-only status
summary the table describes was never more than a placeholder for it.

Chosen direction (screenshots compared against lazygit, see the session
this plan section came out of): a full ASCII wordmark, closer to lazygit's
own welcome screen than a plain text summary. Generated with `figlet`,
not hand-drawn, so the glyphs are guaranteed to line up.

lazygit doesn't show one fixed-size banner either — resize its terminal
wider and its own logo visibly grows to fill the extra room. A single
fixed wordmark can't do that: sized for a roomy terminal (`toilet -f
mono12 ferrit`, 58 trimmed / 60 padded columns) it lost to its own
narrow-terminal fallback far more often than lazygit's does at the same
size; sized to always fit comfortably (`toilet -f smmono12 ferrit`, 30
columns) it stayed small even when the terminal had plenty of room to
spare, unlike lazygit. Landed on three tiers instead, biggest-that-fits:

| Tier | Font | Canvas | Needs (right-pane `width, height`) |
| --- | --- | --- | --- |
| Small | `smmono12` | 30x7 | 40, 16 |
| Medium | `mono12` | 60x7 | 70, 16 |
| Large | `bigmono12` | 60x13 | 70, 24 |
| *(none)* | plain `ferrit` label | — | below Small's |

Medium and Large share a canvas width (60) — what Large actually needs
more of is *height* (13 rows against Medium's 7), so it only kicks in once
the pane is both wide **and** tall enough, closer to how lazygit's own
banner reads as "bigger" (bolder, denser) rather than just "wider" once
the terminal has real room. Large, at a big terminal:

```
┌ Status ──────────────────────────────────────────────────────┐
│                                              ██              │
│     ▒████                                    ██              │
│     █████                                    ██       ██     │
│     ██                                                ██     │
│   ███████    ░████▒    ██░████   ██░████   ████     ███████  │
│   ███████   ░██████▒   ███████   ███████   ████     ███████  │
│     ██      ██▒  ▒██   ███░      ███░        ██       ██     │
│     ██      ████████   ██        ██          ██       ██     │
│     ██      ████████   ██        ██          ██       ██     │
│     ██      ██         ██        ██          ██       ██     │
│     ██      ███░  ▒█   ██        ██          ██       ██░    │
│     ██      ░███████   ██        ██       ████████    █████  │
│     ██       ░█████▒   ██        ██       ████████    ░████  │
│                                                              │
│     A lazygit-style terminal UI for git, written in Rust     │
│                                                              │
│              v0.2.0 · MIT · The ferrit Authors               │
│             https://github.com/ferrit-rs/ferrit              │
│                                                              │
│                   Press ? for keybindings                    │
└──────────────────────────────────────────────────────────────┘
```

A spaced-letter `F E R R I T` banner (no real glyphs, fits any width, but
reads as a placeholder rather than a logo) was also sketched and set
aside — kept in mind for the below-Small fallback, not as an alternative
tier.

- **Content**: the wordmark; `env!("CARGO_PKG_DESCRIPTION")` as the tagline
  (already "A lazygit-style terminal UI for git" in `Cargo.toml`);
  `env!("CARGO_PKG_VERSION")`, `env!("CARGO_PKG_LICENSE")`, and
  `env!("CARGO_PKG_REPOSITORY")` for the credit line — compile-time
  constants, not hand-typed strings that go stale. All literal, no git read;
  this is chrome, not data.
- **Every wordmark row needs the same width.** `toilet` right-pads every
  row of a figlet-style font to the widest row, so all rows span the same
  columns and the letterforms line up down the block; each `Wordmark`
  constant in `src/ui.rs` stores its rows trimmed of that trailing padding
  instead (no trailing whitespace sitting in the source), so
  `welcome_lines` re-pads every row to that tier's own `width` field before
  centering it. Skipping that re-pad was a real bug during implementation:
  `Line::centered()` centers each row by its own (then-different) trimmed
  length, which shifted rows against each other and broke the letterforms
  — centering only reads as "the same logo, centered" when every row is
  still the same width first.
- **Tier selection**: `welcome_lines(width, height)` (`src/ui.rs`) takes
  the right pane's own `(area.width, area.height)`, tries `WORDMARK_LARGE`,
  `WORDMARK_MEDIUM`, `WORDMARK_SMALL` in that order against each one's
  `min_area`, and falls back to a plain bold `ferrit` label below all
  three rather than wrapping or truncating a wordmark into noise.
- **Where it hooks**: `draw_right_pane` in `src/ui.rs`, a dedicated
  `app.focus == Pane::Status` branch placed before the mock/real-repo split
  (image preview and `Preview::Note` still win first, same precedence as
  every other pane, since they're handled even earlier in the function).
  Static content, no new `App` state.
- **Mock vs real repo**: shown for both, confirmed by
  `status_shows_the_welcome_screen` in `tests/render.rs` — one case per
  tier plus the plain-label fallback, each sized so exactly one tier's bar
  is cleared, checked against a marker glyph unique to that tier's source
  art (`▐` only in Small, a 4-wide `▄` run only in Medium, `▒` only in
  Large). All four run against `App::mock()`, which needed no change.
- **Superseded**: the "Right pane content by focus" table's original Status
  entry (a `git status`-style summary) never got built; this replaces it,
  not layers alongside it — `mock::RIGHT_STATUS` is gone, `mock.rs`'s
  `Pane::Status` mock-body case merged into `Pane::Branches`'s existing `""`
  (both routes return earlier now, before that match is ever reached for
  either pane).

## Files pane: directory tree — done

lazygit's Files pane groups changed paths under their directories (an
always-present root `/`, one header row per directory, files nested and
shown by their own name rather than the full path), not a flat list of
full paths. ferrit's had a flat list since phase 1 (`theme::file_line`,
one `Line` per `FileEntry`, unchanged since the M6 lazygit re-skin).

- **Flat when there's nothing to nest.** The common case — every changed
  file directly at the repo root — stays exactly the flat list it always
  was: no root row, no directory headers, `row_count(Pane::Files) ==
  self.files.len()`. The tree only appears once at least one changed file
  has a parent directory (`App::files_tree_rows`, `src/app.rs`) — matches
  lazygit's own behaviour and keeps the previous, simpler rendering (and
  every test built around it) valid for the case most working trees are in
  most of the time.
- **Building the tree.** `build_file_tree` (`src/app.rs`) groups
  `self.files` (already a flat, path-sorted `Vec<FileEntry>` from
  `git::status::files`) into nested `BTreeMap<String, TreeNode>` levels —
  `BTreeMap` for free alphabetical iteration per level, directories and
  files interleaved by name rather than directories-first, matching
  lazygit. `flatten_file_tree` walks it depth-first into `Vec<FileRow>`
  (`Dir { path, name, depth, expanded }` or `File { index, depth }`),
  skipping the children of any directory in `self.collapsed_dirs`.
- **New `App` state**: `collapsed_dirs: HashSet<PathBuf>` — empty means
  "everything expanded" (lazygit's own default), so no pre-population
  needed. Persists across `refresh()`; changed by `toggle_files_dir`, wired
  to both `Enter` on a directory row and a left click landing on one
  (`on_mouse`, same as lazygit — the click still moves the selection there
  too, exactly like clicking any other row already did). Either on a file
  row is a no-op, reserved for staging (`docs/PLAN_6_STAGING.md`), not this.
- **Every read path re-derives the tree.** `row_count`, `file_lines`,
  `file_display`, `right_key_for`'s `Pane::Files` arm, and `build_preview`
  all call `files_tree_rows()` fresh rather than caching it — cheap at
  working-tree sizes, the same choice `branch_lines`/`commit_lines` already
  make. A selected row resolves to a `FileEntry` (for a diff or an image
  preview) only when it's a `File` row; a `Dir` row (or the root) means no
  diff, same "nothing selected" precedent every other pane already has.
- **Rendering**: `theme::file_line` gained a `depth: usize` parameter —
  two spaces of indent per level, and past depth 0 it shows just the file's
  own name (`Path::file_name`) instead of the full path, since the
  directory rows above it already say where it lives. `theme::dir_line` is
  new: the same indent, then `▼`/`▶` (lazygit's own glyphs) and the
  directory's bold name, no status code (a directory's would mean
  aggregating several files', and lazygit doesn't bother either).
- **Tests**: `tests/diff_app.rs` — `files_pane_groups_nested_files_into_a_tree`
  (root + directory header + nested file, and the nested file's own diff
  still resolves correctly by row), `files_pane_stays_flat_with_no_nesting`
  (the common case is unaffected), `enter_on_a_files_directory_row_toggles_it`
  (`row_count` shrinks and grows back around one `Enter`). `tests/mouse.rs`
  — `click_on_a_files_directory_row_toggles_it`, the same shrink/grow check
  but through a real `feed_mouse` click. Existing tests keyed off
  `mock::mock_files()` (which spans several directories, so it does
  trigger the tree) — `src/app.rs`'s own unit tests and
  `tests/render.rs`'s image-preview test — moved off hardcoded flat indices
  onto `row_count`/`file_display`-based lookups, the same pattern
  `tests/diff_app.rs`'s `files_row` helper already used. Tests built on a
  single flat temp repo (`tests/{diff_app,mouse,scrollbar}.rs`'s other
  fixtures) needed no changes at all — confirms the "stays flat" case
  really did stay compatible.
