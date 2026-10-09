//! `git init`, for the welcome screen (`docs/PLAN_16_START_WITHOUT_REPO.md`).

use std::path::Path;

use super::Repo;
use super::read::stderr;
use crate::git::error::{GitError, GitResult};
use crate::git::exec;

impl Repo {
    /// `git init` in `dir`, then open the repository it made. No flag: the
    /// first branch is whatever the user's `init.defaultBranch` says, as for
    /// `git init` in a shell. It goes through `exec`, so the command log shows
    /// it. A folder that cannot be written, or does not exist, is git's own
    /// message and nothing is created.
    pub fn init(dir: &Path) -> GitResult<Self> {
        let mut cmd = exec::git(dir);
        cmd.arg("init");
        let out = exec::output(&mut cmd)
            .map_err(|e| GitError::InitFailed(format!("cannot run git: {e}")))?;
        if !out.status.success() {
            let message = stderr(&out);
            return Err(GitError::InitFailed(if message.is_empty() {
                "git init refused".to_owned()
            } else {
                message
            }));
        }
        Self::open(dir)
    }
}
