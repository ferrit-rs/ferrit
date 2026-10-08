# 4. A strict lint policy, enforced in CI

Status: accepted (`Cargo.toml` `[lints]`, `clippy.toml`, `.github/workflows/ci.yml`)

## Context

A solo project grows fast and nobody reviews it. The compiler and clippy are the only
reviewers, so they should be strict from the start.

## Decision

- `unsafe_code = "forbid"`.
- Clippy `all`, `pedantic`, `nursery` and `cargo` at warn, with a curated deny list
  (`unwrap_used`, `expect_used`, `panic`, `indexing_slicing`, `print_stdout`,
  `pub_use`, `exit`, ...) and a short, commented list of allowed lints that fight the
  codebase.
- CI runs `cargo clippy --all-targets --all-features -- -D warnings`, rustdoc with
  `-D warnings --document-private-items`, `cargo deny`, `cargo machete` and nextest.
  The toolchain is pinned in `rust-toolchain.toml`; `clippy.toml` sets the MSRV lower
  than the toolchain on purpose.
- Tests and examples relax the panic family with a file-level `#![allow(..)]`, because
  a failed setup is the assertion.

## Consequences

- No panicking shortcut reaches production code: errors are typed (`thiserror`) and
  surface in the UI.
- Every `allow` is visible and explained.
- The cost is churn when a new clippy release adds a lint, absorbed by the pinned
  toolchain.
- The same `#![allow]` header is repeated in each test file; `docs/PLAN_23_TEST_SUPPORT.md`
  centralises it.
