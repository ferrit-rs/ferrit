# 5. A port in front of git

Status: accepted, done (`docs/PLAN_21_GIT_PORT.md`)

## Context

`App` held a concrete `git::Repo` and called about 50 of its methods. That made
every test about app logic build a real repository and spawn `git`, made
failures that git rarely produces on demand (a rejecting hook) impossible to test,
and tied the application to `git2`.

## Decision

- A set of traits by role (`GitRead`, `GitIndex`, `GitHistory`, `GitBranches`,
  `GitStash`, `GitRemote`, `GitConfig`) in `domain/git/port.rs`, bundled as
  `GitPort`, which also says how to `reopen` a second handle for a worker thread.
- `Repo` implements them by forwarding to its own methods. `App` holds a
  `Box<dyn GitPort>`; dynamic dispatch costs nothing next to a subprocess, and a
  generic `App<G>` would spread through every screen and test.
- `FakeGit` is an in-memory implementation that records calls and can fail the next
  call of a method. A contract suite runs the same scenarios on `Repo` and `FakeGit`.

## Consequences

- Tests of app logic (`tests/app_fake_git.rs`) run in milliseconds and can inject
  failures. The contract suite already found a real difference: `Repo` cannot unstage
  before the first commit.
- The adapter lives in `infra/git/`: for each `domain/git/<x>.rs` that holds the types, the
  code that reads with `git2` or runs `git` is `infra/git/<x>.rs`. `tests/layering.rs`
  keeps `git2` out of the domain.
- The inherent methods of `Repo` stay (tests call them directly), so the trait methods
  share their names; one `#[allow(clippy::same_name_method)]` records why.

## Alternatives considered

- **One big `GitRepository` trait.** A single 60-method interface that every fake would
  have to implement; the role traits keep each concern readable.
- **Generics instead of `dyn`.** Faster in theory, invasive in practice.
