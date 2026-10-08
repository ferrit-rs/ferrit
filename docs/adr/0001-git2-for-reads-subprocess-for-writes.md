# 1. Read with `git2`, write with the `git` subprocess

Status: accepted (phases 2, 3, 6, 7; see `docs/PLAN_2_GIT_BACKEND.md` and
`docs/PLAN_7_COMMIT.md`)

## Context

Ferrit needs to read a repository fast and often (status, refs, log, blobs, on
every file change) and to change it in ways users expect to behave exactly like
their own `git` (commit, rebase, stash, fetch, push). Two backends exist:
libgit2 through `git2`, and the `git` executable.

## Decision

- **Reads** go through `git2`, in process. No process spawn per refresh, typed
  results, easy to run in a worker thread.
- **Mutations** shell out to `git`, through one function (`exec::git`, the only
  constructor of a git `Command`; `tests/git_exec.rs` scans the sources to keep
  it that way). Every command is recorded in the command log.
- The domain code returns owned model types (`domain/git/model.rs`) and never
  exposes a `git2` type to `app/`.

## Consequences

- Hooks (`pre-commit`, `commit-msg`), GPG/SSH signing, `commit.template` and the
  user's own config apply, because it is the user's `git`. libgit2 runs none of
  the hooks.
- The command log can show the exact command that ran, which lazygit users
  expect.
- Two backends to keep consistent, and a C dependency (`git2` with
  `vendored-libgit2`). Accepted.
- A subprocess is slow next to a trait call, so the planned `GitPort`
  (`docs/PLAN_21_GIT_PORT.md`) puts a seam in front of both and lets tests use
  an in-memory fake.

## Alternatives considered

- **`git2` for everything.** Rejected: skipped hooks and signing are a bug in a
  git client.
- **`git` subprocess for everything.** Rejected: too slow for the refresh loop.
- **`gix`.** Left open for fast reads later; not needed yet.
