# ferrit, agent guide

Read this before making changes. It is the source of truth for how work lands
in this repo. `CLAUDE.md` just imports this file.

## Workflow: push straight to main

Rapid iteration. Commit to `main` and push. No feature branches, no PRs for
normal work. Keep each commit small, green, and self contained so the history
stays bisectable.

- Pull (or fetch and rebase) before you start; `main` moves fast and often has
  another session's uncommitted work in the tree. Only stage the files you
  actually changed.
- If a change is genuinely risky or wants review, a short lived branch is fine,
  but the default is `main`.

## Changelog is mandatory

Every commit that adds, changes, removes, or fixes visible behaviour adds a
line under `## [Unreleased]` in `CHANGELOG.md`, in the right group
(`Added` / `Changed` / `Fixed` / `Removed`). Format: Keep a Changelog.

Skip the entry only for pure refactors, tests, docs, and CI that a user would
never notice.

## Before every commit

```
cargo build && cargo clippy --all-targets && cargo test
```

All three must pass. No warnings.

## Style

- No em dashes, no decorative `---` / `***` rules in prose, comments, or commit
  messages. Use commas, colons, parentheses, separate sentences.
- Conventional commits: `feat(scope):`, `fix(scope):`, `refactor(scope):`, ...
- Match the surrounding code: comment density, naming, module layout. Domain
  logic lives under `src/<domain>/` (see `src/git/`, `src/image/`).
- No Claude or AI attribution anywhere in commits or history.
