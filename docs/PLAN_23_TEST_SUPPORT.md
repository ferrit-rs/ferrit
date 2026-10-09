# Plan: phase 23, test support and a narrow public API

**Status: in progress (C0 to C3 and the property tests done; the API narrowing is open).** Fourth and last slice of the architecture clean-up. It makes the test
suite easy to read and extend, and makes the library surface say what is public on purpose.

What differs from the sketch below, and why:

- **No `tempfile` dependency.** `tests/common/mod.rs` keeps a 20-line `TempDir` (now with a
  counter in the name, and `child()` for a nested directory). Adding a crate to remove 20 lines
  failed the "is it already in the codebase?" step of `AGENTS.md`.
- **Shared in `tests/common`:** `TempDir`, `git`, `commit_all` and `configure_identity` (the
  variants that were identical in 17 to 28 files). 28 files lost their copy, about 1,600 lines
  went. Eight files keep a `TempDir` of their own on purpose: they build a repository in the
  constructor, canonicalise the path, or expose the inner path (`git_init`, `git_backend`,
  `app_welcome`, `app_attach`, `drilled_keybar`, `git_initial_commit`, `fake_git_contract`, and
  the support modules of `git_stats` and `dashboard_screen`).
- **Four files split into test crates** (`tests/<name>/main.rs` + `support.rs` + one module per
  behaviour): `dashboard_screen`, `app_create_remote`, `git_stats`, `git_rebase`. Test names
  and count are unchanged; the largest file left is `tests/config.rs` at 775 lines.
- **The 10-line `#![allow]` header stays in each test crate.** Rust has no way to share an
  inner attribute across crates, and a workspace lint table cannot differ per target.
- **`proptest` added, for two properties' worth of code** (`tests/proptest_diff.rs`, 8
  properties): the diff parser (any text parses without panicking and every range it reports
  slices the text in order; a generated well-formed diff parses back to the files, paths, hunk
  headers, counts and bodies that made it, with git's `@@ -5 +5 @@` form for a count of 1) and
  `apply::transform_body` (selecting everything is the identity; whatever is selected the old
  side never changes; the new side is exactly the context, the unselected deletions and the
  selected additions; selecting nothing leaves no change; an out-of-range index is ignored).
  Checked by breaking the code: demoting an unselected `-` to context wrongly, and defaulting a
  missing hunk count to 0, each fail a property and shrink to a minimal input. It is a
  dev-dependency only; every crate it pulls in is MIT or Apache-2.0, which `deny.toml` allows.
  `tests/proptest_diff.proptest-regressions` keeps the minimal failing cases from that check,
  which proptest replays on every run.
- **The `test-util` feature is in** (it was going to be `test-support`; clippy's
  `redundant_feature_names` refuses that suffix). It gates `ferrit::replay` (the harness behind
  `--replay` and `--fixture`, 1,300 lines) and `domain::git::fake::FakeGit`, and the hidden
  flags of `main.rs`. The integration tests turn it on through
  `[dev-dependencies] ferrit = { path = ".", features = ["test-util"] }`, so a plain
  `cargo test` works; `cargo publish --dry-run` packages and builds without it, and a default
  `cargo build` rejects `--replay` (checked). By hand: `cargo run --features test-util -- --replay SCRIPT`.
  The CI tape job and `.dev-tools/flow-compare.sh` build with it.
- **`app::mock` stays public and un-gated:** the production repo-free path reads its sample
  text (`mock::RIGHT_DIFF`, `COMMAND_LOG`) and image bytes, so gating it would change behaviour.
- **Not done:** `pub(crate)` for most of the library and `#![warn(missing_docs)]` on `domain`
  (212 items without a doc today). Most modules are public because a type of theirs appears in
  a public signature (`App.help`, `App.theme`, the `*Completion` events), and `unnameable_types`
  is a warning here; making them private is a design change of `App`'s public surface, not a
  visibility sweep. Measured: 194 of the missing docs are in `domain/git`, 48 of them in
  `model.rs`.

## Goal

One shared test kit instead of 36 copies of the same helper, property tests where input is
free-form (diff and patch parsing), big test files split by behaviour, and a public API
limited to what `main.rs` and the tests really need, with test-only seams behind a
feature.

```
Today                                       This phase
tests/app_commit.rs   ┐                     tests/common/mod.rs   TempRepo, RepoBuilder, lint allow
tests/app_stage.rs    │ each has its own    tests/app_commit.rs   use common::*;
...36 files           ┘ struct TempDir      tests/app_stage.rs    use common::*;
(10-line #![allow] header x 59)             tests/proptest_diff.rs  generated hunks round-trip
lib.rs: pub mod app, components, domain,    lib.rs: pub mod app (curated); components, domain
        replay (doc hidden)                         pub(crate); replay, mock behind "test-util"
```

## The gap this fixes

- `grep -l "struct TempDir" tests/*.rs | wc -l` is 36. Each copy builds a directory from
  `temp_dir()`, the pid and nanoseconds, and each file repeats `configure_identity` and
  `commit_all`. A fix to the helper means 36 edits.
- Every one of the 59 test files starts with the same `#![allow(clippy::unwrap_used, ...)]`
  block, because `Cargo.toml` denies the panic family and tests need it.
- No `tests/common/`, no builder, no fixtures: tests describe a repository with a run of
  raw `git` commands.
- Four files are over 1000 lines (`dashboard_screen.rs` 1456, `app_create_remote.rs` 1196,
  `git_stats.rs` 1044, `git_rebase.rs` 1011).
- The parsers that read free-form text have only example tests: `domain/git/diff/parse.rs`
  (`tests/diff_parse.rs`, 4 KB) and `apply.rs` (`tests/apply_patch.rs`, 1.7 KB).
- `src/lib.rs` exports `pub mod app`, `components`, `domain` and `#[doc(hidden)] pub mod
  replay`, so almost every item is public: it is a test-access seam, not a designed API.
  `app::mock` and `replay` ship in the library.

## Approach

1. **`tests/common/mod.rs`.**
   ```rust
   pub struct TempRepo { dir: tempfile::TempDir }
   impl TempRepo {
       pub fn new() -> Self;                       // git init + identity
       pub fn file(self, path: &str, body: &str) -> Self;
       pub fn commit(self, message: &str) -> Self;
       pub fn branch(self, name: &str) -> Self;
       pub fn path(&self) -> &Path;
       pub fn open(&self) -> ferrit::domain::git::Repo;
   }
   ```
   Fluent, consuming builder, so a test reads like the scenario. The shared
   `#![allow(...)]` lives once, in `common`, and each test file keeps a one-line allow.
2. **`tempfile` as a dev-dependency.** Reason for the commit message: it removes a
   hand-rolled, collision-prone temp directory (pid + nanos) and cleans up on panic.
   Nothing else is added for it (AGENTS.md ladder, step 5).
3. **`proptest` for the two parsers**, as a dev-dependency. Reason: the input is
   free-form text from `git diff`; properties are cheap to state and find edge cases
   examples do not. Properties:
   - generated hunks (`+`/`-`/context lines, with and without the "no newline" marker)
     parse and re-render to the same text;
   - for any hunk and any non-empty line selection, `apply` of the built patch followed by
     the inverse leaves the index unchanged;
   - the parser never panics on arbitrary bytes (it returns an error or an empty diff).
4. **Split big files by behaviour**, not by size: `dashboard_screen.rs` into
   `dashboard_layout.rs` (frames), `dashboard_keys.rs` (input), `dashboard_worker.rs`
   (async); the same idea for the other three. Names say what the file proves.
5. **Move the next tests to `FakeGit`** (phase 21's fake) wherever the assertion is about
   app logic, not about git: `app_branch.rs`, `app_stash.rs`, `app_remote.rs`. The
   `git_*.rs` files stay on real repositories, as they test the adapter.
6. **Narrow the API.**
   - `domain` and `components` become `pub(crate)` modules where `main.rs` and
     `tests/` do not use them; what tests need is re-exported through the one place that
     should (see the `pub_use = "deny"` lint: use `pub mod` paths, not `pub use`).
   - `#![warn(missing_docs)]` on `domain`.
   - `app::mock` and `replay` move behind a `test-util` cargo feature
     (`#[cfg(any(test, feature = "test-util"))]`); integration tests enable it through
     `[dev-dependencies] ferrit = { path = ".", features = ["test-util"] }`. A normal
     `cargo install ferrit` no longer builds them.

## What it has to resolve

- `Cargo.toml` denies `unwrap_used`, `expect_used`, `panic` and others. In `common`, a
  failed setup must still fail the test: helpers return `Self` and use `expect` under a
  file-level `#![allow(clippy::expect_used)]`; this is the one place that allow lives.
- A self-dev-dependency with a feature is the standard way to enable a feature for tests
  only. Check `cargo publish --dry-run` still packages (AGENTS.md: release goes through
  `scripts/release.sh`, never by hand).
- `cargo nextest` runs each test in its own process, so `TempRepo` needs no global state.

## State on `App`

None. No `App` change.

## Impl sketch

```rust
// tests/common/mod.rs
#![allow(clippy::expect_used, clippy::unwrap_used)]
impl TempRepo {
    pub fn commit(self, message: &str) -> Self {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-m", message]);
        self
    }
}

// tests/app_commit.rs
let repo = TempRepo::new().file("a.txt", "one").commit("init").file("a.txt", "two");
let mut app = App::from_path(repo.path());
```

## Out of scope

- **`insta` snapshots.** The replay harness and `test/flows/*.flow` already do snapshot
  testing in the project's own format; adding a second would split the story. Revisit only
  if the flow format becomes a burden.
- **Coverage tooling and CI jobs** (`cargo-llvm-cov`, MSRV, OS matrix). Plain CI work, no
  plan needed; they ship as separate `ci:` commits.
- **`ReplayError`** to replace the `String` errors in `src/replay/`. Nice, no signal.
- **Rewriting assertions.** Moved tests keep their assertions; only setup changes.

## Self-testing (see `PLAN_SELF_TESTING.md`)

- `tests/common/mod.rs` has its own `#[test]`s in `tests/common_selftest.rs`: `TempRepo`
  creates a repository with the given commits and branches, and cleans up on drop.
- `tests/proptest_diff.rs`: the three properties above, with the failing-seed file
  `proptest-regressions/` committed.
- After each split, `cargo nextest list` shows the same test names (grep count equal).
- All prior phase tests stay green.

## Milestones

- **C0, kit.** `tests/common/mod.rs`, `tempfile`, `common_selftest.rs`. No existing test
  touched.
- **C1, migrate.** Replace the 36 `TempDir` copies, a handful of files per commit
  (`test: use TempRepo in ...`). Test count unchanged.
- **C2, proptest** for `diff_parse` and `apply_patch`.
- **C3, split** the four big files.
- **C4, fake-backed tests** for `app_branch`, `app_stash`, `app_remote`.
- **C5, API.** `test-util` feature, `pub(crate)` pass, `missing_docs` on `domain`;
  `cargo publish --dry-run` clean via `scripts/release.sh` (dry run).
- **C6, close.** clippy clean, docs build with `-D warnings`, all prior C green.

## Definition of done (phase 23)

- [ ] `grep -l "struct TempDir" tests/*.rs` returns nothing.
- [ ] No test file over 800 lines.
- [ ] `proptest_diff.rs` runs in CI; `proptest-regressions/` is committed.
- [ ] `cargo build --release` without `test-util` does not compile `mock` or `replay`.
- [ ] `cargo doc --document-private-items --no-deps` with `-D warnings` passes.
- [ ] `scripts/release.sh` dry run succeeds.
- [ ] Test count is not lower than before the phase (`cargo nextest list | wc -l`).
- [ ] `PLAN_0_GENERAL.md` row 23 marked done; `CHANGELOG.md` gets one line only if the
      published crate surface changed.

## After phase 23

The architecture work is closed. Open ideas, none planned: `ReplayError`, a `ViewModel`
for the sheets and popups (see phase 22, Out of scope), coverage in CI.
