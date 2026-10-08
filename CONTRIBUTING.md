# Contributing to Ferrit

Thanks for helping improve Ferrit. Contributions are welcome in the form of
bug reports, feature ideas, documentation, tests, and code.

## Before you start

- Search existing issues before opening a new one.
- For a large change, open an issue first so the design can be discussed.
- Keep changes focused. Avoid unrelated cleanup in the same change.
- Update the relevant `PLAN_N` file when changing planned behavior.
- Add a line under `## [Unreleased]` in `CHANGELOG.md` for every visible change.

## Development setup

Ferrit pins its Rust toolchain in `rust-toolchain.toml`.

```bash
git clone https://github.com/ferrit-rs/ferrit
cd ferrit
cargo run
```

Source responsibilities are separated by area:

- `src/domain/`: Git models, the `GitPort` traits and their adapters, profile, and image logic.
  `app/` goes through `GitPort`; a test that only needs app logic can use `FakeGit` and
  `App::with_git` instead of building a repository.
- `src/app/`: Ferrit state, events, screens, keymap, and terminal lifecycle.
- `src/components/`: reusable UI primitives and isolated `tui_overlay` code.

## Making changes

1. Fork the repository and create a focused branch.
2. Implement the smallest change that solves the problem.
3. Add or update unit, integration, or replay coverage.
4. Update documentation and the changelog when behavior is user-visible.
5. Run the checks below.
6. Open a pull request with a clear summary, testing notes, and screenshots or
   terminal recordings for UI changes.

Maintainers may commit directly to `main`; outside contributors should use a
fork and pull request.

## Checks

Run the checks used by CI when possible:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features --document-private-items
cargo machete
cargo deny check
cargo nextest run --all-features
```

CI also builds on the minimum Rust version declared in `Cargo.toml` (`rust-version`),
runs the tests on macOS, and reports coverage. Test helpers shared by the integration
tests (`TempDir`, `git`, `commit_all`, `configure_identity`) live in `tests/common`;
use them instead of writing another copy.

If `cargo-nextest` is unavailable, `cargo test --all-features` is a useful
fallback. For behavior changes, run a replay script too:

```bash
cargo run -- --replay test/scripts/40-stage.script --dump-frames /tmp/ferrit-frames
```

## Commit requirements

Commits must be signed off under the Developer Certificate of Origin:

```bash
git commit -s -m "type: short imperative summary"
```

Use a short, imperative subject. Explain motivation and trade-offs in the body
when the change is not self-explanatory.

## Pull requests

Pull requests should:

- explain what changed and why;
- describe how it was tested;
- call out compatibility, UX, or migration concerns;
- keep generated files and unrelated formatting changes out of the diff.

Be respectful and constructive in reviews. See [`MAINTAINERS.md`](MAINTAINERS.md)
for project ownership and contact details.

## License

By contributing, you agree that your contribution is released under Ferrit's
[MIT license](LICENSE) and that your commits satisfy the DCO sign-off above.
