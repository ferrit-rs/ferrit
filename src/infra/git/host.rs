//! The `git2` and subprocess half of `crate::domain::git::host`: the types are there.

use std::sync::atomic::AtomicBool;

use git2::Repository;

use super::diff::{stderr, workdir};
use crate::domain::git::error::{GitError, GitResult};
use crate::domain::git::exec;
use crate::domain::git::process::{combined_output, run_child};

use crate::domain::git::host::{CreateRequest, CreatedRepo, GhProgram, build_create_args, web_url};

/// `gh repo create <target> --private|--public --source <workdir> --remote
/// origin [--description …]`, without `--push`. Refuses, before running
/// anything, a target that fails validation or a repository that already has
/// an `origin` (`gh` would fail on it halfway). `gh`'s own refusal (a name
/// taken, no right to create there) comes back as its message. Nothing is
/// configured locally unless `gh` succeeds: it adds the remote itself, last.
pub(super) fn create_repo(
    repo: &Repository,
    gh: &GhProgram,
    req: &CreateRequest,
    cancel: &AtomicBool,
) -> GitResult<CreatedRepo> {
    let workdir = workdir(repo)?;
    let args = build_create_args(req, workdir).map_err(|e| GitError::HostFailed(e.to_string()))?;
    if repo.find_remote("origin").is_ok() {
        return Err(GitError::HostFailed(
            "a remote called origin already exists".to_owned(),
        ));
    }
    let mut cmd = exec::program(gh.program());
    cmd.args(args);
    let out = run_child(cmd, "gh", gh.timeout, Some(cancel), &GitError::HostFailed)?;
    if out.status.success() {
        Ok(CreatedRepo {
            web_url: web_url(&String::from_utf8_lossy(&out.stdout)),
        })
    } else {
        Err(GitError::HostFailed(combined_output(&out)))
    }
}

/// `git remote set-url <name> <url>`. Git refuses a remote that does not exist
/// and says so.
pub(super) fn set_remote_url(repo: &Repository, name: &str, url: &str) -> GitResult<()> {
    let mut cmd = exec::git(workdir(repo)?);
    cmd.args(["remote", "set-url", "--", name, url]);
    let out =
        exec::output(&mut cmd).map_err(|e| GitError::HostFailed(format!("cannot run git: {e}")))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(GitError::HostFailed(format!(
            "cannot set the URL of {name}: {}",
            stderr(&out)
        )))
    }
}
