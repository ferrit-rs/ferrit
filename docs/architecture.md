# Architecture

Ferrit is a Cargo workspace. The binary is a thin `clap` composition root. Application
state, Git domain, concrete Git adapter, configuration, theme data, and terminal
presentation have separate package ownership:

```
            crates/ferrit/src/main.rs  (clap, terminal setup)
               │
               ▼
        ┌──────────────────────┐
        │ crates/ferrit-app/   │  application state, controllers, runtime
        └──────┬───────────────┘
               │ uses
       ┌───────┼────────┬──────────┬───────────────┐
       ▼       ▼        ▼          ▼               ▼
  domain    git      config      theme            tui
  models    adapter  config.toml data             widgets
  ports     git2     persistence palettes          Ratatui primitives
```

- `ferrit-domain/` is the Git domain and port boundary. It knows no terminal or concrete
  repository implementation. `ferrit-git/src/repo` is the adapter, so `ui` never names
  `Repo` except to build the app.
- `ferrit-app/src/ui/` is the application-facing UI: `App`, events, controllers, components and the run
  loop; `ui/components/` has one file per piece (a pane, a popup, a sheet, a screen): its
  state, its keys and how it is drawn, so a feature is in one place.
- `ferrit-theme/` owns palettes, schemes and persisted theme values. It knows no
  config file, `App`, or Git adapter.
- `ferrit-tui/` owns reusable widgets, terminal chrome, overlays and interactive
  color controls. It knows no `App` or Git adapter.
- `ferrit-config/` owns `config.toml` parsing, validation, section persistence and paths.

Rules that hold today, and that the tests and lints keep:

- Only `ferrit-git/src/repo` names `git2`, and nothing in `ferrit-domain` imports `ratatui`
  or `crossterm`. `ui/image` draws the preview. Rows handed to the UI are owned core
  model types. `tests/layering.rs` checks it on the
  sources, along with `ui/` reaching the adapter only through the injected repository
  factory, and `git` never reaching into `app`.
- `ferrit-tui/` is the reusable terminal toolkit. It knows nothing about `App`; its
  widgets and controls know nothing of Git or theme persistence.
- `ui/` reaches git through `GitPort` and `GitRepositoryFactory` (`ferrit-domain::port`).
  The concrete `RepoFactory` is injected by the binary; tests can use `FakeGit`
  (`ferrit-git/src/fake.rs`). A contract suite (`tests/fake_git_contract.rs`) runs
  the same scenarios on both so the fake cannot drift.
- Errors keep their type up to the screen: `GitError`, `ConfigError`, `ImageError` and
  `AppError` (`thiserror`), with no `Result<_, String>` in `ui/` or `git/`. The only
  `String`s are in view state that is already text (a diff note, a settings footer).
- Every `git` process is built in one function (`ferrit-git/src/repo/exec.rs`), so the command log sees
  every command (`tests/git_exec.rs`).
- No `unsafe`, no `unwrap`/`expect`/`panic` outside tests (`Cargo.toml` `[lints]`).

Known gaps, each with a plan:
- `ui/image` owns terminal image protocol detection and decoding; `git` only supplies image bytes.
- `App` is smaller (87 fields to 26) but not small: the create-remote flow and what the user
  is told are still loose on it. Drawing reads `&App`: a frame returns what it learned as a
  `Landed` value (where each pane landed, for the mouse), and the animations, the toast, the
  image protocol and the diff cache live in a `RenderState` that `draw` takes out of `App` for
  the length of the frame (`PLAN_24_DRAW_VIEW.md`).
- A component decides from an `Env` (a read-only view of the model) and returns `Event`s;
  `ui/reducer.rs` is the one place that changes the state. A component that
  owns a lot of its own state (the settings sheet, the git config screen, the dashboard) is
  a struct over the parts of the app it changes, borrowed for the call. No component writes
  an `impl App` (`tests/layering.rs`). The flows that cross the popups, the workers and
  the repository are files of `ui/` itself: `publish.rs` (creating the GitHub repository)
  and `loading.rs` (reading the diff and the image preview off the UI thread).
- The app library still exposes more than a library would: integration tests reach into
  most of `ui`, and `App`'s public fields force their types to be
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
(`tests/layering.rs`). A feature keeps its name across the roles it plays: domain
capability modules, `ferrit-git/src/repo/` adapter modules, and
`ferrit-app/src/ui/components/<x>.rs` interface modules.

| Path | Holds |
| --- | --- |
| `crates/ferrit/src/main.rs` | CLI parsing, askpass dispatch and terminal bootstrap |
| `crates/ferrit-app/src/ui/` | `App`, input, controllers, feature components, projections and workers |
| `crates/ferrit-config/src/` | `config.toml` model, validation, persistence and SSH config parsing |
| `crates/ferrit-theme/src/` | palette, terminal scheme and persisted theme config |
| `crates/ferrit-tui/src/theme/` | interactive colour picker controls |
| `crates/ferrit-tui/src/widgets/` | reusable terminal widgets, chrome and overlays |
| `crates/ferrit-git/src/` | concrete adapters: `repo`, askpass and feature-gated `FakeGit` |
| `crates/ferrit-domain/src/` | Git models, ports, command log, credential rules, errors, parsers, plans, stats and hosting/config rules; no concrete adapter |
| `crates/ferrit-app/src/replay/` | the scripted test harness (see ADR 2) |
| `crates/ferrit-app/tests/` | app and headless integration tests; shared helpers live in `tests/common` |
| `crates/ferrit/tests/` | binary contract tests requiring `CARGO_BIN_EXE_ferrit` |
| `test/scripts/` | replay scripts |

## How it is tested

Four levels, from cheap to broad: unit tests beside the code, property tests for the
parsers of text read from git (`tests/proptest_diff.rs`), integration tests in `tests/`
(headless `TestBackend` frames and `App` seams such as `feed_key`), and replay
scripts that press keys on a deterministic fixture repository and assert on the screen.
See [ADR 2](adr/0002-replay-scripts-as-integration-tests.md) and `PLAN_SELF_TESTING.md`.
