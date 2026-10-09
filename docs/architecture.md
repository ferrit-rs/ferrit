# Architecture

Ferrit is one crate with a thin binary (`src/main.rs`, a `clap` wrapper) over a library
(`src/lib.rs`). `src/` has four parts and a leaf, each with one job:

```
            main.rs  (clap, terminal setup)
               │
               ▼
        ┌──────────────┐  reads the model, draws   ┌──────────────────────────┐
        │ ui/          │ ────────────────────────► │ app/                     │
        │ screens,     │                           │ the model (App + state/) │
        │ widgets,     │                           │ what changes it (the     │
        │ terminal     │                           │ handlers), keymap, hints │
        └──────────────┘                           └────────────┬─────────────┘
                                                                │ uses
                                          ┌─────────────────────┼───────────────────┐
                                          ▼                     ▼                   ▼
                                  ┌──────────────┐      ┌──────────────┐     ┌──────────────┐
                                  │ git/         │      │ config/      │     │ theme/       │
                                  │ model, port, │      │ config.toml, │     │ palette,     │
                                  │ fake, repo/  │      │ settings     │     │ scheme,      │
                                  │ (git2)       │      │ rows         │     │ [theme]      │
                                  └──────────────┘      └──────────────┘     └──────────────┘
```

- `git/` is the backend and knows nothing of the rest. Inside it, the model and the
  `GitPort` traits are the domain and `git/repo` is the adapter, so `app` never names
  `Repo` except to build the app.
- `app/` is the model (`App` and `app/state/`) and what changes it. `app/state/` is plain
  data (where the cursor is, what popup is up, what the right pane shows) and does not
  draw.
- `ui/` only draws: the screens read `App`, the widgets are reusable pieces that know
  nothing of git or `App`.
- `config/` is `config.toml`; `theme/` is the leaf both `config` and `ui` use (colours).

Rules that hold today, and that the tests and lints keep:

- Only `git/repo` names `git2`, and nothing in `git` imports `ratatui` or `crossterm`
  (`git/image` draws the preview). Rows handed to
  the UI are owned model types (`git/model.rs`). `tests/layering.rs` checks it on the
  sources, along with `app/` reaching `git/repo` only in `App::open` and `git init`, and
  `git` never reaching into `app`.
- `ui/widgets/` knows nothing about git or `App`; `theme/` knows nothing of what is drawn
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
- The handlers in `app/` still decide some things themselves (`create_remote`,
  `git_config_edit`, `dashboard`, `settings_keys`, `popup_keys`, `commit`): the decision is
  to move into the domain, as `git::staging`, `git::remote` and `git::branch` did, leaving
  `App` only to run it and report. The screens read `App`'s fields (`pub(crate)`), which
  couples `ui/screens` to `app`, which is the direction wanted (`ui` reads `app`); `app` still calls `ui` to draw in its run loop and to start the terminal.
- The library still exposes more than a library would: the integration tests reach into
  most of `app` and `ui`, and `App`'s public fields force their types to be
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
(`tests/layering.rs`). A feature keeps its name across the roles it plays:
`git/<x>.rs` the types and rules, `git/repo/` how `Repo` does it (one file per trait of
the port, not per feature), `app/<x>.rs` what a key does with it, `app/state/<x>.rs` what
the model keeps of it, `ui/screens/<x>.rs` how it is drawn.

| Path | Holds |
| --- | --- |
| `src/app/mod.rs` | `App`, which owns the parts and orchestrates what reads several of them (the run loop, a refresh, the event match) |
| `src/app/` | everything that writes the behaviour of `App` (`impl App`): `events`, routing a key or a click (`input`, `dispatch`), what a key does for each feature (`staging`, `branch`, `stash`, `rebase`, `commit`, `remote`, `create_remote`, `git_config`, `git_config_edit`, `welcome`, `askpass`, `dashboard`, `sheet`, `menu`, `context_menu`, `popup_keys`, `diff_query`, `image_query`, `drill_nav`, `settings_keys`), the background work (`workers`, `refresh`), `error`, `mock`, and the remappable bindings (`keymap`) with the key bar and help built from them (`hints`). A handler picks its target, calls the domain's decision (`git::staging`, `git::remote`, ...), then reports; no other folder writes an `impl App` (`tests/layering.rs`) |
| `src/app/state/` | what the model remembers, one file per thing: the panes (`nav`, `pane_rows`, `right_pane` with its diff cursor, `hit_areas`, the drill-downs, the selection keys, the diff views, what the right pane loads), what can sit over them (`popup`, `confirm`, `modal`, the menus, the commit editor `commit_draft`, the create-remote form), the side sheets, `help`, `full_screens`, `render_state`, `prefs` (what is loaded and what it makes) and `theme_editor` |
| `src/ui/screens/` | drawing only: reads `&App` and returns what the frame learned as a `Landed`; `row_lines` are the styled lines of git rows |
| `src/ui/widgets/` | reusable widgets (donut, heat map, toast, drawer, ...) and `tui_overlay/` (vendored overlay code, with its upstream licence) |
| `src/ui/terminal.rs` | the terminal lifecycle |
| `src/config/` | `config.toml` (`mod.rs`, `error.rs`) and `settings` (the rows of the settings sheet) |
| `src/theme/` | how ferrit looks, and nothing else: `palette`, the terminal `scheme` (colour depth), the `[theme]` config (`theme_config`) and the colour picker |
| `src/git/` | the git types and pure logic (model, diff parsing, statistics, config, hosting rules); `port.rs` (the traits) and `fake.rs` (the in-memory git); `profile`, `identity`, `authorship` (commit identities) and `image/` (format detection, preview) |
| `src/git/repo/` | the `git2` and subprocess adapter: `Repo`, in files that follow the traits of `git/port.rs` (`read`, `index`, `history`, `branches`, `stashes`, `remotes`, `gitconfig`, `statistics`) |
| `src/replay/` | the scripted test harness (see ADR 2) |
| `tests/` | integration tests, `app_*` drive `App`, `git_*` drive a real repository; `tests/common` holds the shared helpers, and the big ones are test crates in a folder (`main.rs`, `support.rs`, one module per behaviour) |
| `test/scripts/` | replay scripts |

## How it is tested

Four levels, from cheap to broad: unit tests beside the code, property tests for the
parsers of text read from git (`tests/proptest_diff.rs`), integration tests in `tests/`
(headless `TestBackend` frames and `App` seams such as `feed_key`), and replay
scripts that press keys on a deterministic fixture repository and assert on the screen.
See [ADR 2](adr/0002-replay-scripts-as-integration-tests.md) and `PLAN_SELF_TESTING.md`.
