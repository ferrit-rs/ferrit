# 2. A replay harness for end-to-end behaviour

Status: accepted (`docs/PLAN_SELF_TESTING.md`)

## Context

A TUI is hard to test: the behaviour is "I press these keys on this repository
and the screen shows this". Unit tests on `App` methods do not prove that, and
screenshot tools are slow and unstable.

## Decision

- A small script language (`test/scripts/*.script`) drives the real `App` in
  process: `fixture`, `key`, `click-text`, `exec`, `expect-text`, `expect-no-text`,
  git checks. The runner (`ferrit::replay::runner`) renders to a `TestBackend`.
- Fixtures build deterministic repositories (fixed author, fixed commit dates,
  local config for every knob a user's config could change), so commit ids are the
  same on every machine.
- A script must assert something: `tests/replay.rs` fails a script that only presses
  keys.
- No time-based waiting. `async-key` delivers events until `App::is_idle`; the only
  clock is a 30 s safety limit.
- No snapshot library. Full frames are not stable (branch ages change daily), so
  scripts assert on text, and `snapshot LABEL` only keeps a frame for a human.

## Consequences

- Behaviour is tested the way a user sees it, with the same code path as the binary
  (`ferrit --replay` runs the same runner).
- The harness and the fixtures live in the library (`#[doc(hidden)] pub mod replay`),
  which widens the public surface. `docs/PLAN_23_TEST_SUPPORT.md` moves it behind a
  `test-support` feature.
- Scripts are a second test language to learn; the cost is one page of directives.

## Alternatives considered

- **`insta` snapshots.** Rejected: unstable frames, one more dependency.
- **`vhs` tapes as the only check.** Kept for screenshots in a non-gating CI job, but
  too slow and flaky to gate on.
