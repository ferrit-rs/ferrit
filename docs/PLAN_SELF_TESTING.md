# Plan: self-testing with screenshot proof

**Status: built (ST0 to ST3 and ST6+ done, ST4 and ST5 partly).** The replay
harness is `ferrit::replay` (`src/replay/`), 19 scripts in `test/scripts/` run
in `tests/replay.rs`, and the harness itself is tested in
`tests/replay_harness.rs`. What differs from the sketch below, and why:

- **No `xtask` crate; the fixtures live in the library.** The binary's
  `--fixture` must call the same code as the tests, and a separate crate would
  make the binary depend on a dev tool. `ferrit::replay::fixture` builds
  `canonical`, `history`, `conflict`, `detached` and `remote`; `--fixture NAME
  [--into DIR]` builds one and prints where, for the tapes.
- **The runner is in-process**, one function (`replay::runner::run`) that
  `tests/replay.rs` and `ferrit --replay` both call. The hidden flags
  (`--replay`, `--fixture`, `--into`, `--dump-frames`, `--size`, `--tape`) are
  refused in a release build unless `FERRIT_TEST` is set, and do not appear in
  `--help`.
- **No `insta`.** The sketch's `snapshot` asserted a region against a
  committed `.snap`. Not adopted: a new dependency for one feature, and full
  frames are not stable anyway (the Branches pane prints each branch's age,
  `266d`, which changes every day). `snapshot LABEL` keeps the frame under
  `target/tmp/replay/<script>/NNN-label.txt` for a human or an agent; the
  assertions are `expect-text`, `expect-no-text` and the git checks.
- **Directives added** to make real flows scriptable: `fixture NAME`, `key A B
  C` (several keys, `KeyBinding` syntax: `ctrl-d`, `space`, `pgdn`),
  `async-key K`, `exec ARGS` (run `git` in the fixture, no assertion), `write
  PATH "content"`, `config "toml"` (reopen the app with that configuration),
  `refresh`, and `=>` (exact) beside `->` (contains). `{dir}`, `{origin}` and
  `{other}` expand in `exec`, `write` and `git`.
- **`async-key` replaces the `wait-for` of `PLAN_9_REMOTE.md`.** It gives the
  app an event sender, feeds the key, then delivers events (`App::deliver_event`)
  until `App::is_idle`, all on the calling thread. The only clock is a 30 s
  safety limit that turns a hung script into a failure; a passing run never
  waits on time.
- **Deterministic fixtures.** Fixed author, commit dates one minute apart from
  a fixed epoch, and local git config for every knob a user's own config could
  change (`commit.gpgsign`, `pull.rebase`, `core.editor`, ...), so commit ids are
  the same on every machine.
- **A script must assert something.** `tests/replay.rs` fails a script with no
  `expect-text`, `expect-no-text` or git check: a script that only presses keys
  proves nothing. It also fails when a flow the plans name has lost its script.
- **`git status` output keeps its leading space** (only the end is trimmed): the
  first porcelain column is the index state.
- **Not done:** `--ansi` frame dumps, rendering with `vhs` here (it is not
  installed on this machine), `test/shots-ref/` and the tolerant comparison
  (ST4), and any check that the CI `tapes` job (ST5) runs: it is wired, its
  YAML parses, and it is `continue-on-error`, but it has not run.

## Running it

```
cargo test --test replay                      # every script; the gate
cargo test --test replay_harness              # the machinery itself
cargo run -- --replay test/scripts/40-stage.script --dump-frames /tmp/frames
cargo run -- --fixture canonical --into /tmp/fx   # a fixture to poke at
test/gen-tapes.sh                             # tapes for vhs, in test/tapes
```

A failure prints the script line, what was expected and the frame the run saw;
every run also leaves its snapshots (and, on a failure, `failure.txt`) under
`target/tmp/replay/<script>/`.

## Goal

Any agent working on `ferrit` (Claude included) can verify a feature works
end to end, on its own, and produce visual proof, with no human sitting in
front of the TUI.

Sharpened after studying lazygit's setup: the **correctness gate is
deterministic and headless**. Screenshots are artifacts for humans and for
PR review, never the thing CI depends on.

## What we can do better than lazygit

lazygit drives its real binary in a pseudo-terminal and asserts on
captured view text plus git state. Mature and effective, but:

- every integration test carries timing `Sleep`s, because a pty runs the
  real async event loop and you wait for it to settle
- they abandoned golden `.git`-directory snapshots: flaky, unreviewable,
  painful to update
- tests are Go; the demo `.tape` files that make the README GIFs are a
  separate mechanism

`ferrit` gets three things lazygit's stack cannot easily have:

1. **ratatui `TestBackend`.** Render `ui::draw` into memory, zero
   terminal, sub-millisecond. A whole fast layer with no clean gocui
   equivalent.
2. **A built-in headless replay mode.** The binary steps its own loop
   synchronously from a script file: one input, one update, one draw, one
   frame dump, next. No pty, no timers, no sleeps. Deterministic by
   construction.
3. **One script format** consumed by both the Rust test harness (the gate)
   and `vhs` (the GIF). A flow is defined once.

Adopted straight from lazygit's hard lessons: assert on text not pixels;
snapshot small targeted regions not whole screens; golden state is
structured git output, not a directory blob.

## The three mechanisms

### 1. `TestBackend` unit snapshots

`tests/render.rs`. Render one widget or one region, assert with `insta`.
Targeted: `left_column`, `right_pane`, `status_bar`, never the full
200x60 buffer. Covers `ui::draw` and the layout math in isolation. Cheap
enough to run on every save.

### 2. Headless replay harness (the correctness gate)

The binary gains a hidden test mode:

```
ferrit --replay SCRIPT --fixture NAME --dump-frames DIR [--size 120x40]
```

- `--fixture NAME` builds a throwaway repo and works inside it
- `--replay SCRIPT` reads a script (format below), applies each input to
  `App::update`, runs `ui::draw` into a `TestBackend` after every step
- `--dump-frames DIR` writes `NNN-label.txt` (plain char grid) per step;
  with `--ansi`, also `NNN-label.ansi` with colour attributes
- the loop is synchronous: input, update, draw, dump, next. No real event
  source, no clock, no sleeps

These flags are test-only: hidden from `--help`, behind a hidden `clap`
attribute, and a no-op unless a `FERRIT_TEST` env or debug build is set.
They never matter in normal use.

Driven from `tests/replay.rs`: for each script under `test/scripts/`, run
the binary (or an in-process entrypoint), then assert:

- frame text contains the expected strings, or matches a targeted `insta`
  snapshot of a named region
- `git -C <fixture>` state matches the golden block in the script

Non-zero exit on any mismatch. No timing knobs anywhere.

### 3. `vhs` artifacts (humans only, not a gate)

`test/tapes/*.tape` are generated from the same scripts by
`test/gen-tapes.sh`: script tokens map to `Type` / `Screenshot`, and
`Sleep`s are inserted only so the GIF is watchable. `vhs` renders real
PNG + GIF. These upload as PR artifacts and feed the README. CI never
fails on a pixel diff. A change to a reference PNG is a reviewable diff,
nothing more.

## Script format

`test/scripts/NN-name.script`, one directive per line:

```
# stage a file and commit it
size 120x40
key 2                       # focus Files
key j
key space                   # stage selection
snapshot files-staged       # dump frame + insta-assert region (focused left pane + right pane)
expect-text "M  Cargo.lock"
key c
type "test: replay commit"
key enter
snapshot after-commit
git status --porcelain=v2   -> "1 M. "
git log -1 --format=%s      -> "test: replay commit"
```

Directives: `size WxH`, `resize WxH`, `key <name>`, `type "<text>"`,
`snapshot <label>`, `expect-text "<s>"`, `expect-no-text "<s>"`,
`git <args...> -> "<expected substring>"`, `#` comment. The parser is
shared by `tests/replay.rs` and `gen-tapes.sh`; adding a directive is one
change in one place.

## Fixtures

An `xtask` binary owns repo construction, and nothing else does:

```
cargo run -p xtask -- fixture canonical [--into DIR]
```

Builds the canonical state from `PLAN_1_LAYOUT.md`: 4 commits on `main`;
branches `feat/tui-skeleton`, `fix/parse-args`; working tree with 1
modified (`src/main.rs`), 1 untracked (`docs/notes.md`), 1 staged
(`Cargo.lock`); empty stash. `--replay`'s `--fixture` calls the same
code. Later named fixtures: `conflict`, `detached`, `deep-history`.

Every fixture is a fresh temp dir. Never the real working tree.

## Golden git state

Not a `.git` snapshot. Structured, line-oriented, diffable, and written
inline in the script next to the step it follows:

- `git status --porcelain=v2`
- `git log --format='%h %s' -n N`
- `git stash list`
- `git rev-parse --abbrev-ref HEAD`

Compared as substring or exact, per directive. A reviewer reads the
intent straight from the script diff.

## Per-phase coverage

Each phase from 2 on lands, in the same commit as the feature:

- a `test/scripts/` script driving the flow, with its git golden block
- a targeted mechanism-1 snapshot of the region that changed

| Phase | What the script + golden assert |
| --- | --- |
| 1 layout | focus (`1`-`5`, `Tab`), `j`/`k` clamp, list scroll, resize `40x20`..`200x60` with no panic or horizontal overflow, `?` overlay toggle. No git assertions yet |
| 2 git read | fixture `canonical`; each pane renders the golden status / branches / log / stash |
| 3 diff | modify a file, `git::diff` hunks match golden; right pane shows them; hunk nav moves the viewport |
| 4 scroll behavior | `J` / `K` / page / half-page / `<` / `>` move `right_scroll` clamped to `line_count - viewport`; wheel routes by column; scrollbar present on overflow, absent when it fits |
| 5 staging | stage / unstage at file, hunk, line; `git status --porcelain=v2` golden confirms the index |
| 6 commit | commit moves `HEAD`; `git log -1` golden has the message and parent; amend and fixup shapes |
| 7 branches | checkout / create / delete / merge reflected in `git branch` and `rev-parse` golden |
| 8 remote | ahead / behind and upstream parsed against a local bare remote |
| 9 stash | push / pop / apply / drop change `git stash list` golden |
| 10 rebase | todo edited; continue / abort / skip drive the golden git state; conflict surfaced in a frame |

## The self-test loop an agent runs

```
1. cargo nextest run
     mechanism 1 + 2. seconds. deterministic, no sleeps. THIS IS THE GATE.
     fail -> read the insta diff or the frame .txt under target/, fix, repeat

2. (before a commit, and in CI)  test/gen-tapes.sh && vhs test/tapes/*.tape
     writes test/shots/*.png + *.gif from the same scripts

3. agent opens the PNG / GIF and looks at layout, alignment, polish

4. compare to test/shots-ref/*.png with a tolerance
     unchanged -> silent pass
     intended change -> regenerate, eyeball, copy over shots-ref/, commit with a note

5. attach the PNG / GIF to the PR as human-facing proof
```

Step 1 is the gate. Steps 2 to 5 are artifacts and review.

## Directory layout

```
xtask/                       fixture builder, nothing else
tests/
├── support/mod.rs           shared helpers
├── render.rs                mechanism 1: TestBackend region snapshots
├── replay.rs                mechanism 2: runs every test/scripts/*.script
└── git_backend.rs           src/domain/git/ unit tests (phase 2+)
tests/snapshots/             insta .snap files, committed and reviewed
test/
├── scripts/*.script         source of truth, one per flow
├── tapes/*.tape             generated from scripts, git-ignored
├── gen-tapes.sh
├── shots/                   generated PNG / GIF, git-ignored
└── shots-ref/*.png          committed reference images
```

## In scope

- the three mechanisms above
- the script format and a parser shared by the harness and `gen-tapes.sh`
- `xtask` fixture builder with the canonical state
- inline golden git blocks
- CI running mechanism 1 + 2 as the gate, `vhs` as artifact upload only

## Out of scope

- a matrix of real terminal emulators (iTerm2, Kitty, Windows Terminal,
  nested tmux). `vhs` renders one way; real-terminal quirks are bug
  reports, not this harness
- pixel-perfect fidelity to a specific font and theme
- performance benchmarking (its own concern)
- mouse input (not a feature, see `PLAN_1_LAYOUT.md`)
- fuzzing the event stream (maybe later)

## Milestones

- ✅ **ST0** the `canonical` fixture builds (`ferrit --fixture canonical`;
  `tests/replay_harness.rs`: same commit ids on every build, the screen of
  `PLAN_1_LAYOUT.md`); `tests/render.rs` has the mechanism-1 snapshots.
- ✅ **ST1** `--replay --fixture --dump-frames` (and `--size`, `--into`) in
  the binary, synchronous loop; a script runs green end to end and a failing
  one exits non-zero with its line and frame.
- ✅ **ST2** the parser covers every directive above; `tests/replay.rs` runs
  all of `test/scripts/`.
- ✅ **ST3** phase 1 covered by `10-layout.script`: focus, clamp, Tab order,
  resize `40x20` to `200x60`, the help overlay.
- 🟡 **ST4** `test/gen-tapes.sh` and `ferrit --tape` produce `vhs` tapes from
  the same scripts (12 of 19; the rest change the repository from outside the
  terminal and say so); tested in `tests/replay_harness.rs`. Not done:
  rendering PNG / GIF (no `vhs` here), `shots-ref/`, the tolerant comparison.
- 🟡 **ST5** the CI gate already was `nextest`, which runs the replay tests; a
  `tapes` job now generates the tapes and, best effort, renders one with
  `vhs`, uploading both as artifacts. Unverified until it runs on GitHub.
- ✅ **ST6+** the scripts for every phase, added retroactively:
  `20-status-files` (2), `30-diff` (3), `35-scroll` (4), `40-stage` (6),
  `50-commit` (7), `60-branches` and `65-merge` (8), `70-remote` (9),
  `80-stash` (10), `90-rewrite`, `91-operation`, `92-fixup`, `93-skip`,
  `95-conflict`, `96-detached` (11), `100-command-log` and `110-keymap` (12).
  Each new feature from here adds its script in the same change.

## Definition of done (for the harness)

- one script format defines a flow once; the Rust harness runs it
  headless and deterministic with no sleeps, asserting frame text and
  structured git state; this is the CI gate
- `TestBackend` targeted snapshots cover `ui::draw` regions
- `vhs` renders PNG / GIF from the same scripts as human-facing proof,
  not a gate
- an agent runs `cargo nextest run`, reads failures from frame dumps or
  `insta` diffs, and attaches `vhs` artifacts to the PR, with no terminal
  in the loop
- a feature without a script fails review

## Relationship to the other plans

- `PLAN_0_GENERAL.md` sets the `src/domain/git/` (no ratatui) vs `src/app/screens/`
  (no git logic) boundary that makes mechanisms 1 and 2 possible.
- `PLAN_1_LAYOUT.md` defines the mock data and target screen that ST0
  through ST3 assert against.
- The binary's `--replay` / `--fixture` / `--dump-frames` flags are
  test-only, hidden from `--help`, and inert in a normal run.
- Every `PLAN_N` from 2 on inherits the rule: land the script, the git
  golden, and the region snapshot with the feature.
