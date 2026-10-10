#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "integration test scaffolding: a failed setup is the assertion"
)]
//! The background creation of the GitHub repository (`docs/PLAN_15_CREATE_REMOTE.md`,
//! R2): the slot, the busy label, the answer. No popups yet. `gh` is a fake
//! script, so nothing here reaches GitHub.

mod creating;
mod first_commit;
mod g_key;
mod popups;
mod push_after;
mod ssh_host;
mod support;
mod x_menu;
