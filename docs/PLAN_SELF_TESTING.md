# Plan: self-testing with screenshot proof

## Goal

Any agent working on `ferrit` (Claude included) can verify a feature works
end to end, on its own, and produce visual proof. No human has to sit in
front of the TUI and click around.

Two layers, both run from `cargo` / a shell, both suitable for CI:

- **Layer A, headless assertions.** Render into an in-memory buffer, drive
  the state machine with synthetic key events, assert on cells and on git
  state. Fast, deterministic, no terminal.
- **Layer B, driven binary with screenshots.** Run the real `ferrit`
  binary in a headless terminal, send real keystrokes, capture real PNG
  and GIF frames, then cross-check the effect against `git` on disk.

Layer A answers "the right characters in the right place, the right git
result". Layer B answers "the real program, launched for real, looks and
behaves like this", and hands back an image a person can look at.

## Why both

`TestBackend` can lie by omission: it exercises `ui::draw` and `App::update`
but not `main`, not `tui::init` / `restore`, not the real event loop, not
terminal setup. Layer B covers that seam and gives artefacts for PR review.
Layer A is what you run on every save; Layer B is what you run before a
commit and in CI.

## Tooling

| Tool | Role | Install |
| --- | --- | --- |
| `cargo nextest` | test runner for Layer A | `cargo install cargo-nextest` |
| `insta` | snapshot assertions on rendered buffers | dev-dependency |
| `vhs` | scripted headless terminal, PNG + GIF output | `brew install vhs` (pulls `ttyd`, `ffmpeg`) |
| `git` | building fixture repos, cross-checking results | system |

`vhs` runs a real terminal (`ttyd` + a headless browser it manages) and
executes a `.tape` script of `Type` / `Enter` / `Sleep` / `Screenshot`
commands. It is the closest thing to Playwright for a TUI.

Nothing here needs a display server. `vhs` is headless by design.

## Repo fixtures

Tests never touch the real working tree. A fixture is a throwaway git repo
built in a temp dir.

```
tests/support/repo.rs        (Layer A: Rust helper)
  fn fixture_repo() -> TempDir
    git init
    write files, `git add`, `git -c user.email=... commit`
    make a branch or two, maybe a stash, maybe a dirty file
    return the TempDir (repo path = dir.path())

test/fixtures/build.sh       (Layer B: shell, same shape)
  mktemp -d, git init, seed commits/branches/stash, echo the path
```

The two helpers seed the **same** canonical state so Layer A snapshots and
Layer B screenshots describe the same repo. Canonical fixture:

```
- 4 commits on `main`
- branches: main, feat/tui-skeleton, fix/parse-args
- working tree: 1 modified (src/main.rs), 1 untracked (docs/notes.md),
  1 staged (Cargo.lock)
- stash: empty
```

This is exactly the mock data in `PLAN_1_LAYOUT.md`, so phase 1 screenshots
of the mock and phase 2+ screenshots of the real backend line up.

## Layer A: headless assertions

Lives in `tests/`. Depends on the module boundary from `PLAN_0_GENERAL.md`:
`src/git/` has no ratatui, so it is tested as a plain library; `src/ui/` is
tested through `TestBackend`.

```
tests/
├── support/
│   ├── mod.rs
│   └── repo.rs          fixture_repo(), helpers to mutate it
├── render.rs            TestBackend snapshots of ui::draw
├── nav.rs               focus + selection state machine
└── git_backend.rs       src/git/ against fixture repos (phase 2+)
```

Render test shape:

```rust
let mut term = Terminal::new(TestBackend::new(120, 40))?;
let mut app = App::new_with_mock();          // or App::open(fixture.path())
term.draw(|f| ui::draw(f, &app))?;
insta::assert_snapshot!("boot", term.backend());

app.update(Event::key(KeyCode::Char('2')));  // focus Files
app.update(Event::key(KeyCode::Char('j')));  // select down
term.draw(|f| ui::draw(f, &app))?;
insta::assert_snapshot!("files_focused_row2", term.backend());
```

`insta` writes `.snap` files under `tests/snapshots/`. First run creates
them; later runs diff. `cargo insta review` accepts or rejects a change,
same idea as updating a Playwright snapshot. The `.snap` files are
committed and reviewed like code.

What Layer A must cover, by phase:

| Phase | Layer A assertions |
| --- | --- |
| 1 layout | screen at 80x24, 120x40, 200x60 matches snapshot; `1`-`5` and `Tab` move focus; `j`/`k` clamp at both ends; list scrolls past pane height; resize down to 40x20 does not panic and does not overflow horizontally; `?` overlay toggles |
| 2 git read | `git::status`, `git::branches`, `git::log`, `git::stashes` against the fixture return the canonical state; panes render that state |
| 3 diff | `git::diff(path)` hunks match; right pane renders them; hunk navigation moves the viewport |
| 4 staging | `git::stage` / `git::unstage` at file, hunk, line change the index as `git status --porcelain` confirms |
| 5 commit | `git::commit(msg)` moves `HEAD`, sets message and parent; amend and fixup shapes correct |
| 6 branches | checkout / create / delete / merge reflected in refs |
| 7 remote | ahead / behind counts, upstream tracking parsed correctly (against a local bare "remote") |
| 8 stash | push / pop / apply / drop change the stash list |
| 9 rebase | todo list edited, continue / abort / skip drive the right git state; conflict surfaced |

Rule: every feature lands with its Layer A test in the same commit.

## Layer B: driven binary with screenshots

Lives in `test/tapes/`. One `.tape` per user-visible flow.

```
test/
├── fixtures/
│   └── build.sh
├── tapes/
│   ├── 00-boot.tape
│   ├── 01-focus-panes.tape
│   ├── 04-stage-and-commit.tape
│   └── ...
├── shots/                 generated PNG + GIF, git-ignored
└── shots-ref/             committed reference PNGs for regression
```

Tape shape:

```
Output test/shots/04-stage-and-commit.gif
Set FontSize 14
Set Width 1200
Set Height 800
Set Shell bash

# open ferrit inside the fixture repo (path exported by the runner)
Type "cd $FERRIT_FIXTURE && ferrit" Enter
Sleep 800ms
Screenshot test/shots/04-01-boot.png

Type "2"        Sleep 200ms          # focus Files
Type "j"        Sleep 150ms
Type " "        Sleep 250ms          # stage the selected file
Screenshot test/shots/04-02-staged.png

Type "c"        Sleep 250ms          # commit popup
Type "test: staged from a vhs tape" Enter
Sleep 400ms
Screenshot test/shots/04-03-committed.png

Type "q"
```

Runner script `test/run-tapes.sh`:

```
for tape in test/tapes/*.tape; do
  export FERRIT_FIXTURE="$(test/fixtures/build.sh)"
  vhs "$tape"
  # cross-check: the tape said it committed, so the fixture must show it
  git -C "$FERRIT_FIXTURE" log --oneline | grep -q "staged from a vhs tape" \
    || { echo "FAIL: $tape did not produce the commit"; exit 1; }
done
```

That grep line is the point: the screenshot shows the screen, the `git`
check proves the action actually happened. A screenshot alone can be a
frozen frame that lied.

## The self-test loop an agent runs

```
        ┌─────────────────────────────────────────────────────┐
        │ 1. cargo nextest run           (Layer A, seconds)    │
        │      fail  -> read the insta diff, fix, repeat       │
        │      pass  -> continue                               │
        ├─────────────────────────────────────────────────────┤
        │ 2. test/run-tapes.sh           (Layer B)             │
        │      builds a fresh fixture repo per tape            │
        │      runs vhs, writes test/shots/*.png + *.gif       │
        │      greps git state to confirm each action landed   │
        ├─────────────────────────────────────────────────────┤
        │ 3. agent opens test/shots/*.png and looks at them    │
        │      layout intact? right pane correct? no overflow? │
        ├─────────────────────────────────────────────────────┤
        │ 4. compare against test/shots-ref/*.png              │
        │      unchanged  -> silent pass                       │
        │      changed    -> show both, decide intended or bug │
        ├─────────────────────────────────────────────────────┤
        │ 5. hand the PNG / GIF to the human (attach to the    │
        │      PR, or send it) as the proof the feature works  │
        └─────────────────────────────────────────────────────┘
```

Steps 1 and 2 are scriptable and gated in CI. Steps 3 and 4 are the agent
actually looking at the image, which is what "self-test" means here.

## Visual regression

- `test/shots-ref/` holds the accepted PNG for each screenshot name.
- A change to a reference PNG is a reviewable diff in the PR, same status
  as changing a `.snap` file.
- Pixel-exact comparison is too brittle across `vhs` versions and fonts.
  Compare with a tolerance (for example ImageMagick `compare -metric AE`
  with a small threshold, or a perceptual hash). Store the threshold in
  `test/run-tapes.sh`.
- When a change is intended: regenerate, eyeball, copy `shots/` over
  `shots-ref/`, commit with a note on what moved and why.

## CI

```
jobs:
  layer-a:
    - cargo install cargo-nextest --locked
    - cargo nextest run --all-features
    - cargo insta test            # fails on any unreviewed snapshot change

  layer-b:
    - uses: charmbracelet/vhs-action
    - run: test/run-tapes.sh
    - uses: actions/upload-artifact   # test/shots/*.gif + *.png on every run
```

The Layer B artefacts attach to every PR, so a reviewer (and Richard) sees
the feature move without checking anything out.

## In scope

- `TestBackend` + `insta` harness and the `tests/` tree above.
- Fixture builders shared by both layers, seeding one canonical repo state.
- `vhs` tapes for each phase's headline flows.
- A runner that pairs every tape with a `git` assertion.
- Tolerant image comparison against committed reference PNGs.
- CI wiring for both layers with screenshot artefacts on PRs.

## Out of scope

- Testing against many real terminal emulators (iTerm2, Kitty, Windows
  Terminal, tmux nesting). `vhs` renders one way. Real-terminal quirks are
  handled by bug reports, not this harness.
- Pixel-perfect fidelity to a specific user's font and theme.
- Performance benchmarking (separate concern, separate doc if needed).
- Mouse input (not a feature yet, see `PLAN_1_LAYOUT.md`).
- Fuzzing the event stream (nice later, not now).

## Milestones

- **ST0** `insta` dev-dependency added, `tests/support/repo.rs` builds the
  canonical fixture, one render snapshot of the phase 1 mock screen passes.
- **ST1** Layer A covers all of phase 1: focus, selection clamp, scroll,
  resize, help overlay. Runs under `cargo nextest` in seconds.
- **ST2** `vhs` installed, `test/fixtures/build.sh` produces the canonical
  repo, `00-boot.tape` yields a PNG that matches the phase 1 layout.
- **ST3** `test/run-tapes.sh` runs every tape against a fresh fixture and
  fails loudly when a tape's `git` assertion does not hold.
- **ST4** `test/shots-ref/` seeded, tolerant comparison wired, a
  deliberate layout change is caught as a diff.
- **ST5** CI runs both layers, uploads screenshot artefacts on PRs.
- **ST6+** each later phase adds its Layer A tests and at least one tape in
  the same PR that ships the feature.

## Definition of done (for the harness)

- `cargo nextest run` exercises rendering and the git backend against
  throwaway fixtures, with committed `.snap` files.
- `test/run-tapes.sh` launches the real binary headless, drives it with
  keystrokes, writes PNG + GIF, and cross-checks each flow against `git`.
- An agent can run both, view the PNGs, compare to references, and attach
  the proof to a PR without a human touching a terminal.
- CI runs both layers and publishes the screenshots per PR.
- Adding a feature without a Layer A test fails review.

## Relationship to the other plans

- `PLAN_0_GENERAL.md` sets the `src/git/` (no ratatui) vs `src/ui/`
  (no git logic) boundary that makes Layer A possible. This doc depends
  on it.
- `PLAN_1_LAYOUT.md` defines the mock data and target screen that ST0
  through ST2 assert against.
- Every `PLAN_N` from 2 on inherits the rule: land the Layer A test and a
  tape with the feature.
