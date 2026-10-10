# Architecture

Ferrit is one crate with a thin binary (`src/main.rs`, a `clap` wrapper) over a library
(`src/lib.rs`). `src/` has a backend, the interface, and two small leaves:

```
            main.rs  (clap, terminal setup)
               │
               ▼
        ┌──────────────────────────────────────────────┐
        │ tui/   the interface: App, and one file per  │
        │        piece of it in components/            │
        └───────┬─────────────────┬────────────────────┘
                │ uses            │ uses
                ▼                 ▼
        ┌──────────────┐   ┌──────────────┐   ┌──────────────┐
        │ git/         │   │ config/      │   │ theme/       │
        │ model, port, │   │ config.toml, │   │ palette,     │
        │ fake, repo/  │   │ settings     │   │ scheme,      │
        │ (git2)       │   │ rows         │   │ [theme]      │
        └──────────────┘   └──────────────┘   └──────────────┘
```

- `git/` is the backend and knows nothing of the rest. Inside it, the model and the
  `GitPort` traits are the domain and `git/repo` is the adapter, so `tui` never names
  `Repo` except to build the app.
- `tui/` is everything the user sees and touches. `tui/mod.rs` holds `App` and the run
  loop; `tui/components/` has one file per piece (a pane, a popup, a sheet, a screen): its
  state, its keys and how it is drawn, so a feature is in one place. `tui/widgets/` are
  reusable pieces that know nothing of git or `App`.
- `config/` is `config.toml`; `theme/` is the leaf both `config` and `tui` use (colours).

Rules that hold today, and that the tests and lints keep:

- Only `git/repo` names `git2`, and nothing in `git` imports `ratatui` or `crossterm`.
  `tui/image` draws the preview. Rows handed to
  the UI are owned model types (`git/model.rs`). `tests/layering.rs` checks it on the
  sources, along with `tui/` reaching `git/repo` only in `App::open` and `git init`, and
  `git` never reaching into `app`.
- `tui/widgets/` knows nothing about git or `App`; `theme/` knows nothing of what is drawn
  with it.
- `tui/` reaches git only through the `GitPort` traits (`git/port.rs`). The real adapter is
  `Repo`; tests can use `FakeGit` (`git/fake.rs`). `tui/` names the concrete `Repo` in one
  place, `App::open`, and for `git init`, which runs before any repository exists. A
  contract suite (`tests/fake_git_contract.rs`) runs the same scenarios on both so the fake
  cannot drift.
- Errors keep their type up to the screen: `GitError`, `ConfigError`, `ImageError` and
  `AppError` (`thiserror`), with no `Result<_, String>` in `tui/` or `git/`. The only
  `String`s are in view state that is already text (a diff note, a settings footer).
- Every `git` process is built in one function (`git/repo/exec.rs`), so the command log sees
  every command (`tests/git_exec.rs`).
- No `unsafe`, no `unwrap`/`expect`/`panic` outside tests (`Cargo.toml` `[lints]`).

Known gaps, each with a plan:
- The code that only runs a subprocess (`exec`, `process`, `askpass`, `ssh_config`, and `gh`
  in `host`) is still beside the model in `git/`; it names no `git2`, but it is
  infrastructure, and `tui/` calls some of it directly (`PLAN_21_GIT_PORT.md`, C4).
- `tui/image` owns terminal image protocol detection and decoding; `git` only supplies image bytes.
- `App` is smaller (87 fields to 26) but not small: the create-remote flow and what the user
  is told are still loose on it. Drawing reads `&App`: a frame returns what it learned as a
  `Landed` value (where each pane landed, for the mouse), and the animations, the toast, the
  image protocol and the diff cache live in a `RenderState` that `draw` takes out of `App` for
  the length of the frame (`PLAN_24_DRAW_VIEW.md`).
- A component decides from an `Env` (a read-only view of the model) and returns `Event`s;
  `tui/reducer.rs` is the one place that changes the state. A component that
  owns a lot of its own state (the settings sheet, the git config screen, the dashboard) is
  a struct over the parts of the app it changes, borrowed for the call. No component writes
  an `impl App` (`tests/layering.rs`). The flows that cross the popups, the workers and
  the repository are files of `tui/` itself: `publish.rs` (creating the GitHub repository)
  and `loading.rs` (reading the diff and the image preview off the UI thread).
- The library still exposes more than a library would: the integration tests reach into
  most of `tui`, and `App`'s public fields force their types to be
  nameable. The test seams that can be separated (`replay`, `FakeGit`) are behind the
  `test-util` feature; `git` is documented and checked by `missing_docs`. `app::mock` is
  neither gated nor private, because the repo-free path of the production code reads its
  sample text (`PLAN_23_TEST_SUPPORT.md`).

## One key press

```
terminal ─► Events (one mpsc channel)  ◄── file watcher, poll timer, worker threads
                 │ AppEvent::Input(key)
                 ▼
          App::on_key ─► input.rs picks the owner (popup, sheet, pane)
                 │ Action
                 ▼
          dispatch::run_action ─► the feature module (staging, branch_actions, ...)
                 │ git call            │ slow call (diff, refresh, fetch)
                 ▼                     ▼
          git::repo::Repo      worker thread ─► AppEvent::*Done ─► App::on_*_done
                 │
                 ▼
          App state changes ─► screens::draw(frame, &mut App) ─► paint pass ─► terminal
```

- **One channel.** Terminal input, filesystem changes, a slow poll and every background
  result arrive as an `AppEvent` (`app/events.rs`), so the loop blocks on a single `recv`.
- **Slow work leaves the UI thread.** Refresh, diff, image decode, statistics and
  fetch/pull/push run in workers. Each carries a generation number; a stale result is
  dropped.
- **Drawing is a function of state.** Hit areas are recorded during draw so a click can be
  mapped back to a row.

## Where things live

The rule: a folder is a flat list of files named after what they do, with a subfolder
only for a feature that has several files, and nothing deeper than two folders
(`tests/layering.rs`). A feature keeps its name across the roles it plays: `git/<x>.rs`
the types and rules, `git/repo/` how `Repo` does it (one file per trait of the port, not
per feature), `tui/components/<x>.rs` everything the interface does with it.

| Path | Holds |
| --- | --- |
| `src/tui/mod.rs` | `App` and the composition root (`api.rs` holds what tests ask of it); `impl App` blocks live only under `tui/` (`tests/layering.rs`) |
| `src/tui/event.rs` | `Event` (what a component asks) and `Env` (what it may read) |
| `src/tui/reducer.rs` | `App` state mutation for component events and runtime outcomes |
| `src/tui/components/` | one file per piece: `files/` (file actions, tree model, file projection and file navigation), `panes/` (shared navigation, selection identities, rows, drills, hit areas and column drawing), `branches`, `commits`, `stash` (what a key does in each), `diff/` (the right column: `right_pane` with its line cursor, `views`, `queries`, `draw`), `commit_editor`, `create_remote`, `menu`, `popups` (popup, question, note), `help`, `command_log`, `keybar`, `dashboard` (+ `dashboard/`), `settings`, `git_config/` (`catalog`, `edit`, `keys`, `screen`, `draw`), `welcome`, `remote` |
| `src/tui/` (the rest) | `input` (routing a key or a click), `controllers/actions` (resolved action mutations and navigation), `scene` (what drawing may read of `App`: references to its state; drawing takes a `Scene`, never `App`), `view` (the read-only questions screens and tests ask), `publish` (creating the GitHub repository), `loading` (the diff and the image preview, off the UI thread), `keymap/{action,binding,context,defaults}` (explicit keymap domain), `events`, `runtime` (terminal loop and background-event routing), `workers` (background work and refresh), `draw` (the top-level layout, `Landed`, `RenderState`), `prefs`, `row_lines::{rows,diff,status}` (explicit renderer ownership), `terminal`, `error`, `mock` |
| `src/tui/widgets/` | reusable widgets: `chrome` (panel, separator, scroll bar, drawer, dialog, lists, key bar), `donut`, `heatmap`, `share_bar`, `chart_palette`, `text_input`, `toast`, and `tui_overlay/` (vendored overlay code, with its upstream licence) |
| `src/config/` | `config.toml` (`mod.rs`, `error.rs`) and `settings` (the rows of the settings sheet) |
| `src/theme/` | how ferrit looks, and nothing else: `palette` (with its style helpers), the terminal `scheme` (colour depth), the `[theme]` config (`theme_config`) and the colour picker |
| `src/git/` | the git types and pure logic (model, diff parsing, statistics, config, hosting rules); `port.rs` (the traits) and `fake.rs` (the in-memory git); `identity` (commit identities: the profile, the settings, the pick) |
| `src/git/repo/` | the `git2` and subprocess adapter: `Repo`, in capability files that follow `git/port.rs` (`status`, `log`, `blob`, `diff`, `index`, `history`, `branches`, `stashes`, `remotes`, `gitconfig`, `statistics`); `read` holds only shared worktree and command-output helpers |
| `src/replay/` | the scripted test harness (see ADR 2) |
| `tests/` | integration tests, `app_*` drive `App`, `git_*` drive a real repository; `tests/common` holds the shared helpers, and the big ones are test crates in a folder (`main.rs`, `support.rs`, one module per behaviour) |
| `test/scripts/` | replay scripts |

## How it is tested

Four levels, from cheap to broad: unit tests beside the code, property tests for the
parsers of text read from git (`tests/proptest_diff.rs`), integration tests in `tests/`
(headless `TestBackend` frames and `App` seams such as `feed_key`), and replay
scripts that press keys on a deterministic fixture repository and assert on the screen.
See [ADR 2](adr/0002-replay-scripts-as-integration-tests.md) and `PLAN_SELF_TESTING.md`.
