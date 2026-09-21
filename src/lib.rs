//! `ferrit` internals, exposed as a library so the headless render tests can
//! call `app::screens::draw` against a `TestBackend` without a terminal. See
//! `docs/PLAN_SELF_TESTING.md`.

pub mod app;
pub mod components;
pub mod domain;
