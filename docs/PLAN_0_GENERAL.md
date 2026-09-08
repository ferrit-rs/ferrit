# Plan 0: overview

The map for `ferrit`. Each phase has its own `PLAN_N_*.md` with the detail.
This file holds the vision, the principles, the architecture, and the phase
index. Keep it short; the phase files carry the weight.

## North star

A lazygit-style terminal UI for git, in Rust. Keyboard-first, fast on large
repos, small and predictable keymap. Not a git porcelain replacement on the
command line: the value is the TUI.

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
phase 1:  single crate, module boundary only
  src/
    git/       <- no ratatui import allowed here
    image/     <- preview decode + terminal graphics detection
    ui/        <- no git logic here
    events.rs  <- glue: terminal input + fs-watch + poll -> one channel
    app.rs     <- glue: state + event -> update -> draw

later:  split into two crates in a workspace
  ferrit-git/   library: open repo, status, diff, stage, commit, branch, ...
                unit-tested against fixture repos, no terminal
  ferrit/       binary: ratatui TUI, depends on ferrit-git
```

The split is deferred, but the module boundary in phase 1 is drawn so the
split is a move, not a rewrite.

## Phases

| # | File | Scope | Status |
| --- | --- | --- | --- |
| 0 | `PLAN_0_GENERAL.md` | this overview | living |
| 1 | `PLAN_1_LAYOUT.md` | layout only, mock data, keyboard nav, no git | done |
| 2 | `PLAN_2_GIT_BACKEND.md` | read-only git via `git2`: feed Status, Files, Branches, Commits, Stash with real data; blob reads + image preview | in progress (G0, G1, G3..G6 done; G2 partial) |
| 2.5 | (no file) | live refresh: `src/events.rs` multiplexes terminal input, a recursive fs-watch on the worktree and a 10s poll; a change from another shell re-snapshots on its own, lazygit style | done |
| 3 | `PLAN_3_DIFF_VIEW.md` | real diffs in the right pane via `git diff` / `git show` subprocess (lazygit style, honours user `git config`), git-native colouring, hunk navigation, scrolling | planned |
| 4 | `PLAN_4_SCROLL_BEHAVIOR.md` | right-pane scroll keys (lazygit style, no left-pane fight), viewport-aware clamp, scrollbar widget, mouse wheel | done |
| 5 | `PLAN_5_STAGING.md` | stage / unstage at file, hunk, line; refresh after | planned |
| 6 | `PLAN_6_COMMIT.md` | commit popup (message input), amend, fixup | planned |
| 7 | `PLAN_7_BRANCHES.md` | checkout, create, delete, fast-forward, merge | todo |
| 8 | `PLAN_8_REMOTE.md` | fetch, pull, push, upstream tracking, ahead/behind | todo |
| 9 | `PLAN_9_STASH.md` | stash push, pop, apply, drop | todo |
| 10 | `PLAN_10_REBASE.md` | interactive rebase todo editor, continue / abort / skip, conflict flow | todo |
| 11 | `PLAN_11_POLISH.md` | config file, themes, keymap customization, real command-log capture, help | todo |

Cross-cutting:

- `PLAN_SELF_TESTING.md` (headless snapshot tests + `vhs` screenshot tapes)
  applies to every phase from 1 on.
- Live refresh (`src/events.rs`) is already wired: every phase from 3 on that
  adds a cached, rebuilt-on-nav right-pane value must also rebuild it on a
  background `AppEvent::Refresh`, without discarding scroll or view state that
  belongs to an unchanged selection. See `PLAN_3_DIFF_VIEW.md` "App wiring".

### Not scheduled (revisit after phase 11)

- AI commit message generation (see `INSPIRATION.md`: lazygitrs, gmsg)
- PR / issue panel (see `INSPIRATION.md`: blippy)
- Mouse support
- Undo view built on the reflog (see `INSPIRATION.md`: git-time-machine)
- Worktree management (see `INSPIRATION.md`: gwm)

## Definition of "v1.0"

Phases 1 through 9 done and stable on Linux and macOS. Phase 10 (rebase) can
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
