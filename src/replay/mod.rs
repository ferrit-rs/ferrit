//! The headless replay harness of `docs/PLAN_SELF_TESTING.md`: a script of
//! keys, expectations and git checks is run against a throwaway fixture
//! repository, one input, one update, one frame at a time. No terminal, no
//! timers, no sleeps, so a run is deterministic and a failure names the script
//! line and shows the frame it saw.
//!
//! - `script`: the format and its parser (shared by the runner and the
//!   `vhs` tape generator, so a directive is added in one place).
//! - `fixture`: named, deterministic repositories.
//! - `runner`: steps a script through an `App` and a `TestBackend`.
//! - `tape`: turns a script into a `vhs` tape for human-facing screenshots.
//!
//! Test scaffolding, not application code: it is the one place outside
//! `domain::git::exec` allowed to start `git` directly, because it builds and
//! inspects repositories rather than operating one on the user's behalf.

pub mod cli;
pub mod fixture;
pub mod runner;
pub mod script;
pub mod tape;
