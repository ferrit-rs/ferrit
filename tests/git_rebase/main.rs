#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::pathbuf_init_then_push,
    clippy::iter_on_single_items,
    clippy::format_collect,
    elided_lifetimes_in_paths,
    reason = "integration test scaffolding: a failed setup is the assertion, helper ergonomics beat lint-cleanliness here"
)]
//! `Repo::operation` and `Snapshot::operation` (`docs/PLAN_11_REBASE.md` R1):
//! which merge, rebase, cherry-pick or revert git is stopped in, read from the
//! repository state, plus the progress of a rebase.

#[path = "../common/mod.rs"]
mod common;

mod edge_cases;
mod operations;
mod rewriting;
mod support;
