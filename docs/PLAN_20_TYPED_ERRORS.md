# Plan: phase 20, typed errors end to end

**Status: planned.** First slice of the architecture clean-up (phases 20 to 23). It
changes no screen: every message the user reads today stays word for word. What
changes is what travels between the layers: a typed error instead of a `String`.

## Goal

An error keeps its type from the place it happens (a git subprocess, a config write,
a worker thread) up to the single place that turns it into text (the Status pane and
the toast). `App` stops matching on strings, `AppError` stops having a catch-all that
accepts any `String`, and a reader of the code can follow a failure by its type.

```
Today                                   This phase
git / io / toml                         git / io / toml
   │ .to_string()  (type lost here)        │ GitError / ConfigError / ...
   ▼                                       ▼
AppEvent::RemoteDone { Result<_, String> } AppEvent::RemoteDone { Result<_, AppError> }
   ▼                                       ▼
App.last_error: Option<String>          App.last_error: Option<AppError>
   ▼                                       ▼
Status pane / toast (text)              Status pane / toast (AppError::to_string())
```

## The gap this fixes

The typed enums exist and are good (`GitError`, `src/domain/git/error.rs`, thiserror,
one variant per operation; `AppError`, `src/app/error.rs`). They are bypassed:

- `src/app/error.rs`: `impl From<String> for AppError` and `impl From<&str> for AppError`
  both build `AppError::Operation(String)`. Any `String` becomes an "error" with no
  category, so `?` on a `Result<_, String>` compiles and the type is lost silently.
- `src/app/mod.rs:599` and `:601`: `last_error: Option<String>` and
  `watch_error: Option<String>`. The error is rendered to text when it is stored
  (`AppError::Refresh(error).to_string()`, `mod.rs:1174`), so nothing downstream can
  tell a refresh failure from a remote failure.
- `src/app/events.rs:46-52`: `AppEvent::RemoteDone { message: Result<String, String> }`
  and `RemoteCreated(Result<String, String>)`. The doc comment (events.rs:34-40) says
  the string is deliberate ("this module stays git-agnostic"). That reason no longer
  holds: `GitError` is `Send` and owned, and `RefreshDone` already carries an owned
  `Box<RefreshCompletion>`.
- `src/app/mod.rs:165`, `diff_query.rs:48,55`, `image_query.rs:16,19`,
  `config/mod.rs:276,293,319`, `create_remote.rs:102,539`, `remote.rs:208`,
  `domain/git/operation.rs:117`, `domain/git/rebase.rs:219`: `Result<_, String>`.
- `AppError::NothingStaged` duplicates `GitError::NothingStaged`.
- `GitError::BranchFailed` is classified downstream by matching git's stderr
  (`error.rs` doc comment on `BranchFailed`, and the "delete refused for being
  unmerged" case).

Counting honestly: `grep "Result<[^>]*, String>" src` finds about 45 sites, but about
33 of them are in `src/replay/` (the test and demo harness: `fixture.rs`, `runner.rs`,
`script.rs`, `cli.rs`, `tape.rs`). Those stay `String` for now (see Out of scope). The
real target is the ~12 sites in `src/app/` and `src/domain/git/`.

## Approach

1. **One error type per boundary, not one global enum.**
   - `GitError` (exists) for everything a git subprocess or `git2` does.
   - `ConfigError` (new, `src/app/config/error.rs`, thiserror): `Io(io::Error)`,
     `Serialize(toml::ser::Error)`, `Parse`. Replaces the `String` of `save_theme`,
     `save_sections`, `section_value`.
   - `ImageError` (new, small): decode and read failures of `image_query::load`.
   - `AppError` (exists) wraps them with `#[from]`, and loses `Operation(String)`,
     `From<String>`, `From<&str>`.
2. **Typed events.** `AppEvent::RemoteDone { op, message: Result<String, AppError> }`
   and `RemoteCreated(Result<String, AppError>)`. `RefreshCompletion.snapshot` and
   `DiffCompletion.result` carry `Result<_, AppError>`. The worker threads stop calling
   `.to_string()` before `send`.
3. **Classify once.** Replace stderr substring matching with a function
   `GitError::classify(op, stderr) -> GitError` in `domain/git/error.rs`, so the one
   special case (unmerged branch refused) becomes a variant
   `GitError::BranchNotMerged(String)` and the UI matches on the variant.
4. **Render at the edge.** `last_error` and `watch_error` become `Option<AppError>`.
   The only `.to_string()` is where the Status pane and toast are drawn
   (`screens/`). `refresh_failure: Option<String>` (the dedupe key, `mod.rs:1171`)
   becomes a comparison of `AppError` by `to_string()` once, kept private.
5. **Dropped on purpose:** `AppError::NothingStaged` (use `GitError::NothingStaged`
   through `Git(#[from])`), and `Refresh(String)` / `Background(String)` become
   `Refresh(#[source] Box<AppError>)` or the typed cause where one exists.

Kept: every `#[error("...")]` text, so the screens and the existing assertions do not
move.

## What it has to resolve

- `GitError` holds `git2::Error`, which is `Send` but not `Clone`. `AppEvent` is not
  required to be `Clone` (check `events.rs`), but `refresh_failure` clones today. Compare
  by rendered text instead of cloning the error.
- `RemoteDone` is also built by tests that drive `on_remote_done` with their own
  channel (`remote.rs:200-208` doc). They pass `Err(String)` today and must pass an
  `AppError` after: a small `AppError::from(GitError::PushFailed(..))` in each test.
- `explain_push_after_creation(line: String)` (`remote.rs`) rewrites a message by
  content. It takes and returns `AppError` and matches the variant
  (`GitError::NoUpstream`, `PushFailed`).

## State on `App`

| Field | Before | After |
|---|---|---|
| `last_error` | `Option<String>` | `Option<AppError>` |
| `watch_error` | `Option<String>` | `Option<AppError>` |
| `remote_refresh_error` | `Option<String>` | `Option<AppError>` |
| `refresh_failure` | `Option<String>` | `Option<String>` (dedupe key only, private) |

Test seam: `App::last_error_text(&self) -> Option<String>` (renders), so the 60-odd
existing assertions on `last_error` text change by one method name, not by content.

## Impl sketch

```rust
// src/app/error.rs
#[derive(Debug, thiserror::Error)]
pub(crate) enum AppError {
    #[error(transparent)] Git(#[from] GitError),
    #[error(transparent)] Config(#[from] ConfigError),
    #[error(transparent)] Image(#[from] ImageError),
    #[error("commit message cannot be empty")] EmptyCommitMessage,
    #[error("no commit yet to amend")] NoCommitToAmend,
    #[error("repository refresh failed: {0}")] Refresh(#[source] Box<AppError>),
    #[error("background operation failed: {0}")] Background(#[source] Box<AppError>),
}
// no From<String>, no From<&str>, no Operation(String)

// src/domain/git/error.rs
impl GitError {
    pub(crate) fn classify(op: GitOp, stderr: String) -> Self { /* one match, tested */ }
}
```

## Out of scope

- **`src/replay/` (about 33 `Result<_, String>`).** It is a test and demo harness, and
  a script parse error is naturally a message. A `ReplayError` is a nice follow-up but
  carries no architectural signal. Lands in phase 23 (test support) if wanted.
- **Splitting `App` (phase 22).** `last_error` stays on `App` here.
- **The `GitPort` trait (phase 21).** Error types here are what the port will return,
  so doing this first keeps phase 21 mechanical.
- **Localisation or message rewording.** Texts do not change.

## Self-testing (see `PLAN_SELF_TESTING.md`)

New file `tests/app_errors.rs`, plus the existing `tests/app_remote.rs`,
`app_commit.rs`, `app_branch.rs`, `config.rs`, `diff_app.rs` updated by method name only.

- `classify` table test: each `GitOp` with representative stderr gives the expected
  variant (nothing staged, unmerged branch, no upstream, plain failure).
- `RemoteDone(Err(GitError::PushFailed(..)))` shows the same Status line as today.
- `RemoteDone(Err(NoUpstream))` after create-remote keeps the `explain_push_after_creation` text.
- A repeated refresh failure opens one toast, not two (existing behaviour, kept).
- `save_theme` on a read-only path returns `ConfigError::Io` and the settings sheet
  shows the same line as today.
- A compile-level guard: a test module with `static_assertions`-style `fn _no_from_string()`
  is not needed; the absence of the impls is enforced by the compiler (a `?` on a
  `String` stops compiling).
- All prior phase tests stay green.

## Milestones

- **C0, pin.** Add `App::last_error_text` and move every assertion to it. Add the
  `classify` table test against the *current* substring logic. No behaviour change,
  suite green.
- **C1, classify.** Introduce `GitError::classify` and `BranchNotMerged`. Callers in
  `domain/git/branch.rs`, `commit.rs`, `remote.rs` use it. Suite green.
- **C2, leaf errors.** `ConfigError` and `ImageError`; convert `config/mod.rs`,
  `image_query.rs`. Suite green.
- **C3, events.** `RemoteDone`, `RemoteCreated`, `RefreshCompletion`, `DiffCompletion`
  carry `AppError`. Workers stop stringifying. Update the events.rs doc comment.
- **C4, state.** `last_error`, `watch_error`, `remote_refresh_error` become
  `Option<AppError>`; rendering moves to the draw edge.
- **C5, close.** Delete `From<String>`, `From<&str>`, `AppError::Operation`,
  `AppError::NothingStaged`. `cargo clippy --all-targets --all-features -- -D warnings`
  clean, layering held (`src/domain` has no `ratatui`), all prior phases green.

## Definition of done (phase 20)

- [ ] `grep -rn "Result<[^>]*, String>" src/app src/domain` returns nothing.
- [ ] `grep -n "impl From<String>\|impl From<&str>" src/app/error.rs` returns nothing.
- [ ] `last_error`, `watch_error`, `remote_refresh_error` are `Option<AppError>`.
- [ ] No stderr substring match outside `GitError::classify`.
- [ ] Every user-visible error text is unchanged (the existing assertions pass untouched
      apart from the `last_error_text` rename).
- [ ] `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`,
      `cargo nextest run`, `cargo doc` with `-D warnings` pass.
- [ ] `PLAN_0_GENERAL.md` row 20 marked done; no `CHANGELOG.md` line (nothing visible).

## After phase 20

Phase 21 (`PLAN_21_GIT_PORT.md`) defines `trait GitPort` whose methods return
`Result<_, GitError>`. This phase makes sure that is the only error type crossing that
boundary.
