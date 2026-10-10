//! Ferrit application library, exposed so headless render tests can call
//! `tui::draw` against a `TestBackend` without a terminal. See
//! `docs/PLAN_SELF_TESTING.md`.

pub mod ui;

/// The scripted test harness behind `--replay` and `--fixture`. Test seam, only
/// built with the `test-util` feature.
#[cfg(feature = "test-util")]
pub mod replay;
