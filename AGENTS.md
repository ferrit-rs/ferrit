# ferrit, agent guide

- Rapid iteration: commit and push straight to `main`, no branches, no PRs.
- Every visible change adds a line under `## [Unreleased]` in `CHANGELOG.md`.
- If you change something related to `PLAN_N`, make sure to also change the content of the file.

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





## Explaining and planning visual behavior

- When explaining, interpreting, or planning a visual concept, layout, or UI behavior with the user, include an ASCII diagram. Show what you understand the user wants and what the interface should look like or do, so the user can check the plan before implementation.

### Self improving

- When I correct a behavior/pattern/preference (not a one-off fact) and you judge it will recur, append one bullet under `## Inbox` in `__SKILLS_LEARNINGS/LEARNINGS.md` (`YYYY-MM-DD [domain] avoid X, do Y, because Z`) and mirror it to auto-memory as `feedback`. You decide, no keyword. Then print: `📝 learning saved: "<one-line>" (say "drop it" to undo)`. Skip: project trivia, anything already enforced by lint/tsconfig/biome/CI, low-confidence guesses. `learn this` forces it.
