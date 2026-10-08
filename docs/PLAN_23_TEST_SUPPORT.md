# Plan: phase 23, test support and a narrow public API

**Status: planned.** Fourth and last slice of the architecture clean-up. It makes the test
suite easy to read and extend, and makes the library surface say what is public on purpose.

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
        replay (doc hidden)                         pub(crate); replay, mock behind "test-support"
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
   - `app::mock` and `replay` move behind a `test-support` cargo feature
     (`#[cfg(any(test, feature = "test-support"))]`); integration tests enable it through
     `[dev-dependencies] ferrit = { path = ".", features = ["test-support"] }`. A normal
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
- **C5, API.** `test-support` feature, `pub(crate)` pass, `missing_docs` on `domain`;
  `cargo publish --dry-run` clean via `scripts/release.sh` (dry run).
- **C6, close.** clippy clean, docs build with `-D warnings`, all prior C green.

## Definition of done (phase 23)

- [ ] `grep -l "struct TempDir" tests/*.rs` returns nothing.
- [ ] No test file over 800 lines.
- [ ] `proptest_diff.rs` runs in CI; `proptest-regressions/` is committed.
- [ ] `cargo build --release` without `test-support` does not compile `mock` or `replay`.
- [ ] `cargo doc --document-private-items --no-deps` with `-D warnings` passes.
- [ ] `scripts/release.sh` dry run succeeds.
- [ ] Test count is not lower than before the phase (`cargo nextest list | wc -l`).
- [ ] `PLAN_0_GENERAL.md` row 23 marked done; `CHANGELOG.md` gets one line only if the
      published crate surface changed.

## After phase 23

The architecture work is closed. Open ideas, none planned: `ReplayError`, a `ViewModel`
for the sheets and popups (see phase 22, Out of scope), coverage in CI.
