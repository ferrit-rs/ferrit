//! What a key does to the repository: the glue between `App` and the git code
//! of this domain. Each file picks its target, calls `git::<feature>` and
//! reports.

#![allow(
    missing_docs,
    reason = "glue between App and the git code: the documented API of the domain is outside this module"
)]
pub mod askpass;
pub mod branch_actions;
pub mod commit;
pub mod create_remote;
pub mod git_config;
pub mod git_config_edit;
pub mod rebase_actions;
pub mod remote;
pub mod staging;
pub mod stash_actions;
pub mod welcome;
