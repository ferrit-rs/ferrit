//! Concrete Git adapters for ferrit.
//!
//! Domain types and ports live in [`ferrit_domain`]. This crate owns the `git2`
//! repository adapter, Git subprocesses, credential prompting, SSH config
//! discovery and the optional in-memory test adapter.

#![warn(missing_docs)]

pub mod askpass;
pub mod command_log;
#[cfg(feature = "test-util")]
pub mod fake;
pub mod repo;
pub mod ssh_config;
