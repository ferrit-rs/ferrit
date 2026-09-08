# ferrit, agent guide

- Rapid iteration: commit and push straight to `main`, no branches, no PRs.
- Every visible change adds a line under `## [Unreleased]` in `CHANGELOG.md`.

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
