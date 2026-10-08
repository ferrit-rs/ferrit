# Plan 0: overview

The map for `ferrit`. Each phase has its own `PLAN_N_*.md` with the detail.
This file holds the vision, the principles, the architecture, and the phase
index. Keep it short; the phase files carry the weight.

## North star

The everyday git manager for the terminal, in Rust, for two things at once: the
repository you already have (a full TUI view of git: stage, commit, branches, rebase,
a dashboard, the git config) and the whole way from an empty folder to a repository on
GitHub, without a browser (`i`, then `G`). One repository at a time, the one ferrit is
opened in. The panes and keys are in the lazygit family, which is a way in and not the
pitch. Keyboard-first, fast on large repos, small and predictable keymap. Not a git
porcelain replacement on the command line: the value is the TUI.

## Principles

- **Backend is headless and testable.** All git logic lives behind an API that
  never touches a terminal. The TUI is a thin consumer.
- **One focused pane at a time.** The lazygit model: 5 left panes, one big
  right pane that reflects the focused one.
- **Destructive actions confirm.** Anything that loses work asks first.
- **Shell out when libraries fall short.** `git2` / `gitoxide` for most things,
  `git` subprocess for the awkward cases (interactive rebase, some merges).
- **Small keymap.** Resist adding a binding for everything. Menus (`x`) hold
  the long tail.
- **MIT.** Contributions under DCO (`git commit -s`).
- **Every feature ships with proof.** A deterministic headless replay
  test plus a screenshot artifact, so any agent can self-verify a feature
  works. See `PLAN_SELF_TESTING.md`.

## Architecture

```
src/
  app/          Ferrit-specific state, events, screens, theme and terminal loop
  domain/       features: git (models + backend), profile, image
  components/   reusable UI primitives and isolated tui_overlay code
```

`components/` stays independent of Ferrit screens and app state. The current
crate keeps orchestration and presentation together; splitting the Git backend
into its own crate remains deferred.

## Phases

| # | File | Scope | Status |
| --- | --- | --- | --- |
| 0 | `PLAN_0_GENERAL.md` | this overview | living |
| 1 | `PLAN_1_LAYOUT.md` | layout only, mock data, keyboard nav, no git | ✅ done |
| 2 | `PLAN_2_GIT_BACKEND.md` | read-only git via `git2`: feed Status, Files, Branches, Commits, Stash with real data; blob reads + image preview | ✅ done (G0, G1, G3..G7; G2 waits for the replay harness, `PLAN_SELF_TESTING.md`) |
| 2.5 | (no file) | live refresh: `src/app/events.rs` multiplexes terminal input, a recursive fs-watch on the worktree and a 10s poll; a change from another shell re-snapshots on its own, lazygit style | ✅ done |
| 3 | `PLAN_3_DIFF_VIEW.md` | real diffs in the right pane via `git diff` / `git show` subprocess (lazygit style, honours user `git config`), git-native colouring, hunk navigation, scrolling | ✅ done |
| 4 | `PLAN_4_SCROLL_BEHAVIOR.md` | right-pane scroll keys (lazygit style, no left-pane fight), viewport-aware clamp, scrollbar widget, mouse wheel | ✅ done |
| 5 | `PLAN_5_CLICK_BEHAVIOR.md` | left-click to focus a pane and move its selection to the clicked row; groundwork for right-pane focus | ✅ done |
| 6 | `PLAN_6_STAGING.md` | stage / unstage at file, hunk, line; refresh after | ✅ done (S0-S4; S5 edge-case polish open) |
| 7 | `PLAN_7_COMMIT.md` | commit popup (message input), amend, fixup | ✅ done (C0-C3; C4/C5 polish open) |
| 8 | `PLAN_8_BRANCHES.md` | checkout, create, delete, fast-forward, merge | ✅ done |
| 9 | `PLAN_9_REMOTE.md` | fetch, pull, push, upstream tracking, ahead/behind | ✅ done |
| 10 | `PLAN_10_STASH.md` | stash push, pop, apply, drop, stash diff preview | ✅ done |
| 11 | `PLAN_11_REBASE.md` | reword / drop / squash / fixup / edit on any commit, autosquash, in-progress operation menu (continue / skip / abort), conflict marking | ✅ done |
| 12 | `PLAN_12_POLISH.md` | real command log, config file, keymap customization, generated help and keybars, `x` menu, palette and themes (eight slices P0 to P7) | ✅ done |
| 13 | `PLAN_13_DASHBOARD.md` | full-screen repository dashboard (`D`): totals, weekly activity, contributors, kinds of change, hot files, branch health, work in progress; computed off the UI thread | ✅ done (D0-D5; config keys for the charts open) |
| 14 | `PLAN_14_GIT_CONFIG.md` | git config editor (`C`): every key with its scope and origin, edit / add / unset at local or global scope through `git config`, typed toggles, secrets redacted | ✅ done (G0-G5) |
| 15 | `PLAN_15_CREATE_REMOTE.md` | create the GitHub repository from ferrit through `gh` (optional, from the `x` menu, private by default), wire `origin`, push with the existing credential popup | ✅ done (R0-R5) |
| 16 | `PLAN_16_START_WITHOUT_REPO.md` | start ferrit outside a repository: a welcome screen offering `git init` (with a question naming the folder), then the usual panes; `--path` keeps its error | ✅ done (W0-W4) |
| 17 | `PLAN_17_SETTINGS.md` | the settings sheet (click the author's name): ferrit's own settings only (theme, accent with the colour picker, mouse, wheel, diff, sign-off, command log), saved at once to `config.toml`; git identities and activity leave it | ✅ done |
| 18 | `PLAN_18_THEMES.md` | painted themes: Dark and Light that paint ferrit's whole screen (one paint pass over the frame buffer maps `Reset` and the ANSI names to the scheme's colours), two themes, no "follow the terminal"; 256-colour fallback; contrast test; fonts are the terminal's, out of scope | ✅ done |
| 19 | `PLAN_19_DASHBOARD_SHEET.md` | the dashboard (`D`) as a drawer over the dimmed panes, like the settings sheet, instead of a full-screen view; same content and worker; as wide as the page so two columns fit; one shared sheet state | ✅ done |
| 20 | `PLAN_20_TYPED_ERRORS.md` | typed errors end to end: no `Result<_, String>` in `app` and `domain`, `AppError` without a catch-all, errors rendered only at the UI edge | ✅ done |
| 21 | `PLAN_21_GIT_PORT.md` | a `GitPort` trait in the domain, `git2` + subprocess code moved to `infra/git`, an in-memory `FakeGit` checked by the same contract suite | 🔄 C0 to C3 done, adapter move (C4) open |
| 22 | `PLAN_22_APP_SPLIT.md` | `App` split into named sub-states (help, theme editor, snapshot, right pane, workers, hit areas), one `Modal` value for a popup or a key-bar question, `dispatch` as a router | 🔄 87 to 47 fields, navigation / identity / draw view open |
| 23 | `PLAN_23_TEST_SUPPORT.md` | shared `tests/common` kit, property tests for the diff parsers, big test files split, narrow public API with a `test-support` feature | 🔄 kit and file splits done, property tests and API narrowing open |

Status legend: ✅ done, 🔄 in progress, 📅 planned (has a `PLAN_N` file), 👉 todo
(no file yet), living (this page).

Cross-cutting:

- `PLAN_SELF_TESTING.md` (headless snapshot tests + `vhs` screenshot tapes)
  applies to every phase from 1 on. Status: the `TestBackend` frame tests
  (mechanism 1) and the `App` seam tests (`tests/app_*.rs`, `feed_key`) exist
  and are what phases 3 to 12 were tested with; the replay harness
  (`ferrit::replay`, `--replay`, 27 scripts in `test/scripts/`, run by
  `tests/replay.rs`) is built. `vhs` rendering and reference screenshots
  (ST4, ST5) are not.
- Live refresh (`src/app/events.rs`) is already wired: every phase from 3 on that
  adds a cached, rebuilt-on-nav right-pane value must also rebuild it on a
  background `AppEvent::Refresh`, without discarding scroll or view state that
  belongs to an unchanged selection. See `PLAN_3_DIFF_VIEW.md` "App wiring".

### Not scheduled (revisit after phase 12)

- AI commit message generation (see `INSPIRATION.md`: lazygitrs, gmsg)
- PR / issue panel (see `INSPIRATION.md`: blippy)
- Undo view built on the reflog (see `INSPIRATION.md`: git-time-machine)
- Worktree management (see `INSPIRATION.md`: gwm)

## Definition of "v1.0"

Phases 1 through 10 done and stable on Linux and macOS. Phase 11 (rebase) can
trail into v1.1. A user can run a full day of normal git work in `ferrit`
without dropping to the shell.

## Working agreement

- Push straight to `main`, small commits. No feature branches or PRs for
  normal work (see `AGENTS.md`). External contributions still come in under
  DCO with `git commit -s`.
- Every commit that changes visible behaviour adds a line under
  `## [Unreleased]` in `CHANGELOG.md`.
- Before each commit: `cargo build && cargo clippy --all-targets && cargo test`,
  all green, no warnings.
- Each phase: land its `PLAN_N` file first, then implement against it, then
  tick the milestones in that file.
- Update this table's Status column as phases move.
