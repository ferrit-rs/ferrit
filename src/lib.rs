//! `ferrit` internals, exposed as a library so the headless render tests can
//! call `ui::draw` against a `TestBackend` without a terminal. See
//! `docs/PLAN_SELF_TESTING.md`.

pub mod app;
pub mod git;
pub mod image;
pub mod mock;
pub mod theme;
pub mod tui;
pub mod ui;
