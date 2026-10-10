#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::pathbuf_init_then_push,
    clippy::print_stdout,
    elided_lifetimes_in_paths,
    reason = "integration test scaffolding: a failed setup is the assertion"
)]
//! The dashboard screen (`docs/PLAN_13_DASHBOARD.md`, D3b) rendered into a
//! `TestBackend`. Layout, colour and edge-case tests use hand-built `RepoStats`
//! at a fixed "now" (no git, no clock); a few tests go through `App` on fixture
//! repositories dated relative to the real clock, so they assert on labels and
//! figure shapes, never on relative-time text.

mod edge_cases;
mod layouts;
mod sections;
mod support;
mod through_app;
