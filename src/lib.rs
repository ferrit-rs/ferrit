//! `ferrit` internals, exposed as a library so the headless render tests can
//! call `app::screens::draw` against a `TestBackend` without a terminal. See
//! `docs/PLAN_SELF_TESTING.md`.

pub mod app;
pub mod components;
pub mod config;
pub mod domain;
pub mod infra;
pub mod keybindings;
/// The scripted test harness behind `--replay` and `--fixture`. Test seam, only
/// built with the `test-util` feature.
#[cfg(feature = "test-util")]
pub mod replay;
pub mod theme;
