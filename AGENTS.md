# ferrit, agent guide

- Rapid iteration: commit and push straight to `main`, no branches, no PRs.
- Every visible change adds a line under `## [Unreleased]` in `CHANGELOG.md`.
- Releases: bump `Cargo.toml`, date the `CHANGELOG.md` section, commit `chore: release ferrit X.Y.Z`, then run `scripts/release.sh` (a dry run) and `scripts/release.sh --execute` to publish the crate and tag it. Never `cargo publish` or tag by hand.
- If you change something related to `PLAN_N`, make sure to also change the content of the file.

## Tests

- Integration tests share their helpers through `tests/common/mod.rs` (`TempDir`, `git`,
  `commit_all`, `configure_identity`). Reuse them; do not paste another copy.
- A test about app logic, not about git, uses `FakeGit` and `App::with_git`. A test about
  git itself uses a real temporary repository. New `FakeGit` behaviour comes with a
  scenario in `tests/fake_git_contract.rs` that also runs on `Repo`.
- Free-form text read from `git` (the diff parser, the line-selection rewrite) is covered by
  property tests in `tests/proptest_diff.rs`; a new parser of that kind gets properties there
  before it gets more examples.
- A test file that grows past about 700 lines becomes a folder (`main.rs`, `support.rs`,
  one module per behaviour), like `tests/git_rebase/`.

## Before writing code

Adapted from [ponytail](https://github.com/dietrichgebert/ponytail).

The best code is the code you never wrote. Walk this ladder and stop at the
first "yes":

1. Does it need to exist at all? If not, do not write it.
2. Is it already in the codebase?
3. Is it in the Rust standard library?
4. Is it a native feature of the language or platform?
5. Is it in a dependency already listed in `Cargo.toml`?
6. Can it be one line?
7. Only then write the minimal code the task needs, nothing more.

Adding a new dependency, a new module, or an abstraction with a single caller
needs a reason stated in the commit message.

Keep distinct logic in separate files or modules. `src/git/` is the backend (Git types and traits, identities, image, and `git/repo`, the `git2` and subprocess adapter) and knows nothing of the rest; `src/app/` is the model (`App`, `app/state/`) and what changes it; `src/ui/` only draws (screens, widgets, the terminal lifecycle); `src/config/` and `src/theme/` hold the settings and the colours. Avoid mixing those responsibilities in one file.





## Explaining and planning visual behavior

- When explaining, interpreting, or planning a visual concept, layout, or UI behavior with the user, include an ASCII diagram. Show what you understand the user wants and what the interface should look like or do, so the user can check the plan before implementation.

### Self improving

- When I correct a behavior/pattern/preference (not a one-off fact) and you judge it will recur, append one bullet under `## Inbox` in `__SKILLS_LEARNINGS/LEARNINGS.md` (`YYYY-MM-DD [domain] avoid X, do Y, because Z`) and mirror it to auto-memory as `feedback`. You decide, no keyword. Then print: `📝 learning saved: "<one-line>" (say "drop it" to undo)`. Skip: project trivia, anything already enforced by lint/tsconfig/biome/CI, low-confidence guesses. `learn this` forces it.
