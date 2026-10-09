# Architecture

Ferrit is one crate with a thin binary (`src/main.rs`, a `clap` wrapper) over a library
(`src/lib.rs`). `src/` is organised by domain, one folder for each thing ferrit knows
about, plus `app`, which is what puts them together.

```
            main.rs  (clap, terminal setup)
               │
               ▼
┌──────────────────────────────────────────────────────────┐
│ app/   App, the run loop, events, popups, the panes      │
│        (orchestration: what reads several domains)       │
└───┬───────────┬──────────────┬──────────────┬────────────┘
    │ uses      │              │              │
    ▼           ▼              ▼              ▼
┌────────┐ ┌────────────┐ ┌──────────┐ ┌──────────────────┐
│ git/   │ │ keybindings│ │ config/  │ │ theme/           │
│ model, │ │ keymap,    │ │ config.  │ │ palette, scheme, │
│ port,  │ │ key bar,   │ │ toml,    │ │ [theme], picker, │
│ fake,  │ │ help       │ │ prefs,   │ │ editor state     │
│ repo/  │ └────────────┘ │ settings │ └──────────────────┘
│ (git2) │                └──────────┘
└────────┘      components/  reusable ratatui widgets (know nothing of git)
```

The domains do not know `app`. Inside `git`, the model and the `GitPort` traits are the
domain and `git/repo` is the adapter that implements them, so `app` never names `Repo`
except to build the app.

Rules that hold today, and that the tests and lints keep:

- Only `git/repo` names `git2`, and nothing in `git` imports `ratatui` or `crossterm`
  (`git/image` draws the preview and is the exception for `ratatui_image`). Rows handed to
  the UI are owned model types (`git/model.rs`). `tests/layering.rs` checks it on the
  sources, along with `app/` reaching `git/repo` only in `App::open` and `git init`, and
  `git` never reaching into `app`.
- `components/` knows nothing about git or `App`; `theme/` knows nothing of what is drawn
  with it.
- `app/` reaches git only through the `GitPort` traits (`git/port.rs`). The real adapter is
  `Repo`; tests can use `FakeGit` (`git/fake.rs`). `app/` names the concrete `Repo` in one
  place, `App::open`, and for `git init`, which runs before any repository exists. A
  contract suite (`tests/fake_git_contract.rs`) runs the same scenarios on both so the fake
  cannot drift.
- Errors keep their type up to the screen: `GitError`, `ConfigError`, `ImageError` and
  `AppError` (`thiserror`), with no `Result<_, String>` in `app/` or `git/`. The only
  `String`s are in view state that is already text (a diff note, a settings footer).
- Every `git` process is built in one function (`git/exec.rs`), so the command log sees
  every command (`tests/git_exec.rs`).
- No `unsafe`, no `unwrap`/`expect`/`panic` outside tests (`Cargo.toml` `[lints]`).

Known gaps, each with a plan:
- The code that only runs a subprocess (`exec`, `process`, `askpass`, `ssh_config`, and `gh`
  in `host`) is still beside the model in `git/`; it names no `git2`, but it is
  infrastructure, and `app/` calls some of it directly (`PLAN_21_GIT_PORT.md`, C4).
- `git/image` still imports `ratatui_image` for the preview protocol.
- `App` is smaller (87 fields to 26) but not small: the create-remote flow and what the user
  is told are still loose on it. Drawing reads `&App`: a frame returns what it learned as a
  `Landed` value (where each pane landed, for the mouse), and the animations, the toast, the
  image protocol and the diff cache live in a `RenderState` that `draw` takes out of `App` for
  the length of the frame (`PLAN_24_DRAW_VIEW.md`).
- The `interface` domain (`components`, the screens, the panes, the popups) and the
  `*_actions` of `app` are not moved yet; they still sit in `app/` and `components/`.
- The library still exposes more than a library would: the integration tests reach into
  most of `app` and `components`, and `App`'s public fields force their types to be
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

| Path | Holds |
| --- | --- |
| `src/app/mod.rs` | `App`, which owns the parts below and orchestrates what reads several of them (the run loop, a refresh, the event match); a behaviour that touches one part lives in that part |
| `src/app/{pane,views,popup,confirm,drill,diff_cursor,selection,refresh,pane_rows}.rs` | the types `App` is made of, one file each: the panes and what each shows (`PaneRows`, a read-only view over `Nav` and the snapshot), the right column's diff views, popups and questions, the drill-downs, the line cursor, the selection keys, the result of a refresh |
| `src/app/{nav,authorship,full_screens,help,right_pane,modal,workers,hit_areas,render_state}.rs` and `sheet.rs` | the parts of `App` with a name: where the user is in the panes, who commits are by, the configuration and what it makes, the full-screen views, the side sheets, the help screen, the theme being edited, the right column, a popup or a question, background work in flight, where the last frame put the clickable things |
| `src/keybindings/` | the remappable bindings (`keymap`: `Action`, `Context`, `Keymap`) and the key bar and help screen built from them (`hints`) |
| `src/app/dispatch.rs`, `input.rs` | routing a key or a click to the part of the app that owns it, and running an `Action` |
| `src/app/*_actions.rs`, `staging.rs`, `commit.rs`, `remote.rs` | one feature each |
| `src/app/screens/` | drawing only |
| `src/config/` | `config.toml` (`mod.rs`, `error.rs`), `prefs` (what is loaded and what it makes: keymap, palette, diff options) and `settings` (the rows of the settings sheet and what changing one does); the sheet itself is `app/settings.rs` |
| `src/theme/` | how ferrit looks, and nothing else: `palette`, the terminal `scheme` (colour depth), the `[theme]` config, the colour picker and the theme editor state |
| `src/app/row_lines.rs` | the styled lines of git rows (a file, a commit, a branch), which move to the panes with the `interface` domain |
| `src/git/` | the git types and pure logic (model, diff parsing, statistics, config, hosting rules); `port.rs` (the traits) and `fake.rs` (the in-memory git); `profile/` (commit identities) and `image/` (format detection, preview) |
| `src/git/repo/` | the `git2` and subprocess adapter: `Repo`, and for each `git/<x>.rs` the code that reads with `git2` or runs `git` |
| `src/components/ui/` | widgets (donut, heat map, palette, toast, drawer, ...) |
| `src/components/tui_overlay/` | vendored overlay code, with its upstream licence |
| `src/replay/` | the scripted test harness (see ADR 2) |
| `tests/` | integration tests, `app_*` drive `App`, `git_*` drive a real repository; `tests/common` holds the shared helpers, and the big ones are test crates in a folder (`main.rs`, `support.rs`, one module per behaviour) |
| `test/scripts/` | replay scripts |

## How it is tested

Four levels, from cheap to broad: unit tests beside the code, property tests for the
parsers of text read from git (`tests/proptest_diff.rs`), integration tests in `tests/`
(headless `TestBackend` frames and `App` seams such as `feed_key`), and replay
scripts that press keys on a deterministic fixture repository and assert on the screen.
See [ADR 2](adr/0002-replay-scripts-as-integration-tests.md) and `PLAN_SELF_TESTING.md`.
