# Architecture

Ferrit is one crate with a thin binary (`src/main.rs`, a `clap` wrapper) over a library
(`src/lib.rs`). The library has three layers.

```
            main.rs  (clap, terminal setup)
               │
               ▼
┌─────────────────────────────┐        ┌────────────────────────────┐
│ app/                        │ uses   │ components/                │
│  App state, events, keys,   │───────►│  reusable ratatui widgets  │
│  screens, popups, config    │        │  and tui_overlay           │
└──────────────┬──────────────┘        │  (know nothing of git)     │
               │ uses                  └────────────────────────────┘
               ▼
┌─────────────────────────────┐
│ domain/                     │
│  git/      model, Repo, ops │──► git2 (reads) · `git` subprocess (writes)
│  profile/  identities       │
│  image/    format detection │
└─────────────────────────────┘
```

Rules that hold today, and that the tests and lints keep:

- `domain/git` imports no `ratatui` and no `crossterm`. Rows handed to the UI are owned
  model types (`domain/git/model.rs`), not `git2` types.
- `components/` knows nothing about git or `App`.
- Every `git` process is built in one function (`domain/git/exec.rs`), so the command
  log sees every command (`tests/git_exec.rs`).
- No `unsafe`, no `unwrap`/`expect`/`panic` outside tests (`Cargo.toml` `[lints]`).

Known gaps, each with a plan: `App` is a large struct with no seam in front of git
(`PLAN_21_GIT_PORT.md`, `PLAN_22_APP_SPLIT.md`), some errors are still strings
(`PLAN_20_TYPED_ERRORS.md`), and `domain/git` holds the `git2` adapter itself
(`PLAN_21_GIT_PORT.md`).

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
          domain::git::Repo      worker thread ─► AppEvent::*Done ─► App::on_*_done
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
| `src/app/mod.rs` | `App`, the run loop, the event match |
| `src/app/dispatch.rs`, `input.rs`, `keymap.rs` | actions, key routing, remappable keys |
| `src/app/*_actions.rs`, `staging.rs`, `commit.rs`, `remote.rs` | one feature each |
| `src/app/screens/` | drawing only |
| `src/app/config/`, `theme*.rs` | `config.toml` and the painted themes |
| `src/domain/git/` | the git model, reads, writes, statistics |
| `src/components/ui/` | widgets (donut, heat map, palette, toast, drawer, ...) |
| `src/components/tui_overlay/` | vendored overlay code, with its upstream licence |
| `src/replay/` | the scripted test harness (see ADR 2) |
| `tests/` | integration tests, `app_*` drive `App`, `git_*` drive a real repository |
| `test/scripts/` | replay scripts |

## How it is tested

Three levels, from cheap to broad: unit tests beside the code, integration tests in
`tests/` (headless `TestBackend` frames and `App` seams such as `feed_key`), and replay
scripts that press keys on a deterministic fixture repository and assert on the screen.
See [ADR 2](adr/0002-replay-scripts-as-integration-tests.md) and `PLAN_SELF_TESTING.md`.
