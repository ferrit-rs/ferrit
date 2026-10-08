# Plan: phase 21, a port for git

**Status: in progress (C0 to C3 done; C4, the adapter move, open).** Second slice of the architecture clean-up. After phase 20 every
git call returns `Result<_, GitError>`; this phase puts a trait in front of them so
`app` depends on an abstraction and `git2` lives in an adapter.

What differs from the sketch below, and why:

- **Done.** `src/domain/git/port.rs` has seven role traits and `GitPort` (the bundle plus
  `reopen`). `Repo` implements each role by forwarding to its inherent method (the
  inherent `impl Repo` carries one `#[allow(clippy::same_name_method)]` with the reason, so
  the existing tests that call `Repo` directly keep working). `App.repo` is
  `Option<Box<dyn GitPort>>`; workers (refresh, diff, image, statistics, remote, create)
  get their handle from `GitPort::reopen`, so `app/` names the concrete `Repo` only in
  `App::open` (the composition root) and for `git init`, which runs before any repository
  exists. `Result<Box<RepoStats>, String>` in the dashboard became `AppError` on the way.
- **`FakeGit` and `App::with_git`** exist (`src/domain/git/fake.rs`). It models stage,
  unstage, discard, commit, checkout, create and delete branch; everything else returns
  `GitError::OperationFailed("FakeGit does not implement ...")`. It is not gated behind a
  feature yet: `PLAN_23_TEST_SUPPORT.md` moves it behind `test-support`.
- **The contract suite found a real gap**: on a repository with no commit yet, `Repo`
  cannot unstage (`could not resolve HEAD`), while the fake can. The scenario starts from
  one commit; the gap is recorded in the test, not fixed here.
- **`mock.rs` was not rebuilt on `FakeGit`.** It feeds `App::mock()` with canned rows and
  the render tests rely on those exact values; rewriting it would change frames for no gain.
- **C4 is not done, and is bigger than the sketch said.** 21 of the 33 files under
  `src/domain/git/` import `git2`, and most of them define a public type (`Diff`,
  `ConfigView`, `MergeOutcome`, `CommitKind`, ...) next to the code that reads it from
  `git2`. Moving the adapter means splitting each file into its types (staying in
  `domain/git/`) and its `git2` code (going to `infra/git/`), and `GitError` first has to
  stop naming `git2::Error` (box the source). 33 integration test files import
  `domain::git::` paths, so the move needs a path-compatibility step. Plan: (1) `GitError`
  without `git2`, (2) move `model.rs`, `error.rs`, `port.rs`, `fake.rs` and the pure
  parsers as they are, (3) one file per commit for the mixed ones, (4) turn on the layering
  test last.

## Goal

`App` talks to git through a trait defined in the domain, not through the concrete
`git::Repo`. The `git2` + subprocess implementation becomes one adapter among two: the
real one and an in-memory `FakeGit` used by unit tests. The visible app does not
change.

```
Today                                     This phase
app::App ──► domain::git::Repo (git2)     app::App ──► domain::git::GitPort (trait)
  98 call sites, no seam                                     ▲            ▲
mock.rs = canned data, repo = None                 infra::git::Repo   FakeGit (tests)
                                                   (git2 + exec)      (in memory)
```

Dependency rule after the phase: `app -> domain <- infra`. `domain` imports neither
`git2`, `ratatui` nor `ratatui_image`.

## The gap this fixes

- `src/app/mod.rs:546`: `repo: Option<git::Repo>`. `src/app/*.rs` call about 50
  distinct `Repo` methods (`stage_file`, `commit`, `delete_branch`, `stash_push`,
  `rebase_edit`, `push_cancellable`, `config_set`, `snapshot`, ...). A grep of
  `repo\.[a-z_]+\(` over `src/app` shows them.
- `src/domain/git/mod.rs:146`: `pub struct Repo { inner: git2::Repository, ... }`. The
  "domain" module is the adapter: about 20 files import `git2`, `exec.rs` spawns
  subprocesses, `GitError` wraps `git2::Error`.
- `src/app/mock.rs` is canned data for `App::mock()`, reached by leaving `repo` as
  `None`. It cannot answer "what happens when the commit fails". Tests that need git
  behaviour (`tests/app_stage.rs`, `app_commit.rs`, `app_branch.rs`, ...) build a real
  temp repository, shell out to `git`, and take hundreds of milliseconds each.
- `src/domain/image/preview.rs:5-6` and `detect.rs:21` import `ratatui_image`: a UI
  protocol inside `domain`.
- Worker threads cannot share a `git2::Repository` (not `Sync`), so they reopen one:
  `Repo::reopen_path` (`mod.rs:~190`), called 5 times in `src/app`. The port has to
  express that.

## Approach

1. **Small role traits, one bundle.** A 100-method trait would be a god interface. Split
   by what `App` does, in `src/domain/git/port.rs`:
   - `GitRead`: `snapshot`, `commit_diff`, `branch_log`, `head_message`, `commit_message`,
     `has_commits`, `has_conflict_markers`, `stats_with`, `identity_settings`.
   - `GitIndex`: `stage_file`, `stage_all`, `stage_all_except`, `discard_file`, hunk apply.
   - `GitHistory`: `commit`, `initial_commit`, `rebase_edit`, `operation_step`, `take_side`.
   - `GitBranches`: `create_branch`, `create_branch_at`, `delete_branch`, `rename_branch`,
     `checkout`, `merge_branch`, `merge_branch_no_ff`, `fast_forward`.
   - `GitStash`: push, push keeping index, pop, apply, drop, rename.
   - `GitRemote`: `remotes`, `set_remote_url`, `fetch_cancellable`, `pull_cancellable`,
     `push_cancellable`, `push_default_current`.
   - `GitConfig`: `config`, `config_set`, `config_add`, `config_unset`,
     `config_replace_value`, `config_unset_value`.
   - `pub trait GitPort: GitRead + GitIndex + ... + Send { fn reopen(&self) -> GitResult<Box<dyn GitPort>>; }`
     with a blanket impl, so `App` names one bound.
2. **`reopen` replaces `reopen_path`.** A worker calls `port.reopen()` on the UI thread
   (cheap clone of a path) and moves the `Box<dyn GitPort>` into the thread. `FakeGit`
   returns a clone sharing its state behind `Arc<Mutex<_>>`.
3. **Adapter move.** `Repo` and its `impl` blocks (`status.rs`, `refs.rs`, `commit.rs`,
   `stash.rs`, `rebase.rs`, `exec.rs`, `askpass.rs`, `ssh_config.rs`, `host.rs`, `init.rs`)
   go to `src/infra/git/`; `model.rs`, `error.rs`, `port.rs`, and the pure parsers that
   do not need `git2` (`diff/parse.rs`, `stats/*`, `config_keys.rs`) stay in
   `src/domain/git/`. `GitError::Open/Read` keep `git2::Error` behind a boxed
   `Box<dyn Error + Send + Sync>` source, so `domain` no longer names `git2`.
4. **`Box<dyn GitPort>` first, generics never.** `App<G: GitPort>` would infect
   `screens`, `events` and every test. Dynamic dispatch costs nothing next to a
   subprocess spawn.
5. **`FakeGit`.** In-memory: a `Vec<FileEntry>`, branches, commits, a failure injector
   (`fail_next(GitOp, GitError)`), and a call log (`calls() -> Vec<Call>`). It is the
   real answer to `mock.rs`; `App::mock()` builds on it and `mock.rs` shrinks to the
   canned data it feeds.
6. **Image.** Move the `ratatui_image`-dependent code (`preview.rs`, the picker half of
   `detect.rs`) to `src/infra/image/`. Keep the pure format detection in
   `src/domain/image/`.

## What it has to resolve

```
      UI thread                              worker thread
 App ── Box<dyn GitPort> ──reopen()──► Box<dyn GitPort> ──► git2 / exec
   │                                     (owned, Send)
   └─ cancel: Arc<AtomicBool> passed into *_cancellable(&cancel)
```

- `git2::Repository` is `Send` but not `Sync`; `GitPort: Send` is enough.
- Cancellable network calls take the flag as a parameter already; the trait signature
  keeps it.
- `isolate_config` (`Repo`, test only: a throwaway global config) is not on the port. It
  stays an inherent method on the adapter, used by tests that build a real `Repo`.
- `App::repo_name`, `welcome` (no repository): `repo: Option<Box<dyn GitPort>>` as today.

## State on `App`

| Field | Before | After |
|---|---|---|
| `repo` | `Option<git::Repo>` | `Option<Box<dyn GitPort>>` |

New seam: `App::with_git(port: Box<dyn GitPort>) -> App` (test and replay entry point;
`App::new` calls it with the real adapter). Borrow note: methods that read `self.repo`
and then mutate `self` keep the existing "take, use, put back" shape; the port is
`&self`-only on reads, so no new borrow conflicts.

## Impl sketch

```rust
// src/domain/git/port.rs
pub trait GitIndex {
    fn stage_file(&self, path: &Path) -> GitResult<()>;
    fn stage_all(&self) -> GitResult<()>;
    fn discard_file(&self, path: &Path) -> GitResult<()>;
}
pub trait GitPort: GitRead + GitIndex + GitHistory + GitBranches
    + GitStash + GitRemote + GitConfig + Send
{
    fn reopen(&self) -> GitResult<Box<dyn GitPort>>;
}

// src/infra/git/mod.rs
impl GitIndex for Repo { /* today's bodies, moved */ }

// tests: src/app/fake_git.rs (cfg(test) or feature "test-support")
let git = FakeGit::new().with_file("a.rs", Unstaged).fail_next(GitOp::Commit, GitError::NothingStaged);
let mut app = App::with_git(Box::new(git));
```

## Out of scope

- **Rewriting the adapter's internals.** The `git2` and subprocess code moves, it is not
  redesigned. Lands nowhere unless a bug asks for it.
- **Splitting `App` (phase 22).** `App` only changes the type of `repo` here.
- **Async git.** Workers stay threads and channels.
- **Porting every test to the fake.** Only the first batch moves here; the rest of
  `tests/app_*.rs` follow in phase 23 as they are touched. Real-repo tests of the adapter
  (`tests/git_*.rs`) stay real on purpose: they are what proves `Repo` is correct.

## Self-testing (see `PLAN_SELF_TESTING.md`)

- `tests/fake_git_contract.rs`: one generic suite run against both `Repo` (temp repo) and
  `FakeGit`: stage then status, commit then log, create then delete a branch, stash push
  then pop. If the fake disagrees with the real adapter the suite fails, so the fake
  cannot rot.
- `tests/app_stage.rs`, `app_commit.rs`: first cases moved to `FakeGit`, including the
  failure paths (`NothingStaged`, `CommitFailed`) that were hard to provoke before.
- Layering test `tests/layering.rs`: reads `src/domain/**/*.rs` and fails on `git2`,
  `ratatui`, `crossterm`, `ratatui_image` (a plain `include_str!` / `walkdir`-free scan).
- All prior phase tests stay green.

## Milestones

- **C0, trait, no behaviour change.** `port.rs` with the role traits; `impl` for `Repo`
  by delegating to the inherent methods; `App.repo` typed as the port. Suite green.
- **C1, reopen.** Replace `reopen_path` with `reopen()` in the five worker call sites.
- **C2, fake.** `FakeGit`, `App::with_git`, the contract suite, `mock.rs` rebuilt on it.
- **C3, first tests moved.** `app_stage.rs`, `app_commit.rs` to the fake.
- **C4, adapter move.** `src/infra/git/` and `src/infra/image/`; boxed error sources;
  the layering test turns on. Update the layout paragraph of `AGENTS.md`, the
  `lib.rs` module list and the `//!` docs.
- **C5, close.** `cargo clippy --all-targets --all-features -- -D warnings` clean; replay
  flows in `test/flows/` unchanged; all prior C green.

## Definition of done (phase 21)

- [ ] `grep -rn "git2\|ratatui" src/domain` returns nothing (code, not comments).
- [ ] `grep -rn "git::Repo" src/app` returns nothing except `App::new`.
- [ ] `FakeGit` passes the same contract suite as `Repo`.
- [ ] At least the staging and commit unit tests run without spawning `git`.
- [ ] `tests/layering.rs` passes and runs in CI.
- [ ] Replay flows produce the same frames as before (`.dev-tools/tui-shot.sh` diff empty).
- [ ] `AGENTS.md` layout paragraph and `PLAN_0_GENERAL.md` row 21 updated; no
      `CHANGELOG.md` line (nothing visible).

## After phase 21

Phase 22 (`PLAN_22_APP_SPLIT.md`) can break up `App` safely: with the port in place a
sub-state can be built and tested against `FakeGit` without a repository on disk.
