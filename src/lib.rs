//! `ferrit` internals, exposed as a library so the headless render tests can
//! call `components::screens::draw` against a `TestBackend` without a terminal. See
//! `docs/PLAN_SELF_TESTING.md`.

pub mod components;
pub mod domain;
