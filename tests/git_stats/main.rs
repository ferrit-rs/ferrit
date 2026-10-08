#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::pathbuf_init_then_push,
    elided_lifetimes_in_paths,
    reason = "integration test scaffolding: a failed setup is the assertion, helper ergonomics beat lint-cleanliness here"
)]
//! `Repo::stats` on fixture repositories with fixed dates, several authors, a
//! `.mailmap`, conventional prefixes, a merge commit, branches ahead / behind /
//! merged / stale and a tag. See `docs/PLAN_13_DASHBOARD.md`, milestones D0
//! and D1. "Now" is fixed at 2026-09-29 12:00 UTC (a Tuesday) so every bucket
//! is predictable.

mod authors;
mod calendar;
mod churn;
mod hot_files;
mod kinds_branches;
mod support;
mod totals;
mod windows;
