//! Diff reads for the `git2` repository adapter.

use crate::repo::Repo;
use crate::repo::exec;
use crate::repo::read::{stderr, workdir};
use ferrit_domain::diff::{Diff, DiffOpts, DiffSide};
use ferrit_domain::error::{GitError, GitResult};
use git2::{Repository, Status};
use std::path::Path;
use std::process::Output;

/// One file's worktree-or-staged diff.
pub(crate) fn file_diff(
    repo: &Repository,
    path: &Path,
    side: DiffSide,
    opts: DiffOpts,
) -> GitResult<Diff> {
    let workdir = workdir(repo)?;
    let mut cmd = DiffCmd::base("diff", opts);
    if side == DiffSide::Staged {
        cmd = cmd.arg("--cached");
    }
    let out = cmd
        .arg("--")
        .arg(path.to_string_lossy().into_owned())
        .run(workdir)?;
    if !out.status.success() {
        return Err(GitError::DiffFailed(stderr(&out)));
    }
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    if text.trim().is_empty() && side == DiffSide::Worktree && is_untracked(repo, path) {
        let ni = DiffCmd::base("diff", opts)
            .arg("--no-index")
            .arg("--")
            .arg("/dev/null")
            .arg(path.to_string_lossy().into_owned())
            .run(workdir)?;
        match ni.status.code() {
            Some(0 | 1) => {},
            _ => return Err(GitError::DiffFailed(stderr(&ni))),
        }
        return Ok(Diff::new(String::from_utf8_lossy(&ni.stdout).into_owned()));
    }
    Ok(Diff::new(text))
}

/// A commit against its first parent (`git show`).
pub(crate) fn commit_diff(repo: &Repository, hash: &str, opts: DiffOpts) -> GitResult<Diff> {
    let out = DiffCmd::base("show", opts)
        .arg("-m")
        .arg("--first-parent")
        .arg("--decorate=short")
        .arg("--stat")
        .arg("-p")
        .arg(hash.to_owned())
        .run(workdir(repo)?)?;
    if !out.status.success() {
        let err = stderr(&out);
        if err.contains("bad object")
            || err.contains("unknown revision")
            || err.contains("ambiguous argument")
        {
            return Err(GitError::NoSuchCommit(hash.to_owned()));
        }
        return Err(GitError::DiffFailed(err));
    }
    Ok(Diff::new(String::from_utf8_lossy(&out.stdout).into_owned()))
}

/// A stash entry's patch, including untracked files.
pub(crate) fn stash_diff(
    repo: &Repository,
    oid: &str,
    header: &str,
    opts: DiffOpts,
) -> GitResult<Diff> {
    let out = DiffCmd::base("stash", opts)
        .after_subcommand("show")
        .arg("--stat")
        .arg("-p")
        .arg("--include-untracked")
        .arg(oid.to_owned())
        .run(workdir(repo)?)?;
    if !out.status.success() {
        return Err(GitError::DiffFailed(stderr(&out)));
    }
    Ok(Diff::new(format!(
        "{header}\n\n{}",
        String::from_utf8_lossy(&out.stdout)
    )))
}

fn is_untracked(repo: &Repository, path: &Path) -> bool {
    repo.status_file(path)
        .is_ok_and(|s| s.contains(Status::WT_NEW))
}

struct DiffCmd {
    args: Vec<String>,
}

impl DiffCmd {
    pub(crate) fn base(sub: &str, opts: DiffOpts) -> Self {
        let mut args = vec![
            sub.to_owned(),
            "--no-ext-diff".to_owned(),
            "--color=never".to_owned(),
            format!("--unified={}", opts.context),
            format!("--find-renames={}%", opts.rename_threshold),
            "--submodule".to_owned(),
        ];
        if opts.ignore_whitespace {
            args.push("--ignore-all-space".to_owned());
        }
        Self { args }
    }

    fn after_subcommand(mut self, verb: &str) -> Self {
        self.args.insert(1, verb.to_owned());
        self
    }

    fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    fn run(self, workdir: &Path) -> GitResult<Output> {
        exec::output(exec::git(workdir).args(&self.args))
            .map_err(|e| GitError::DiffFailed(format!("cannot run git: {e}")))
    }
}

#[allow(
    clippy::same_name_method,
    reason = "the `GitPort` diff role forwards to these methods under the same names"
)]
impl Repo {
    /// One file's worktree-or-staged diff.
    pub fn file_diff(&self, path: &Path, side: DiffSide, opts: DiffOpts) -> GitResult<Diff> {
        file_diff(&self.inner, path, side, opts)
    }

    /// One commit's diff against its first parent.
    pub fn commit_diff(&self, hash: &str, opts: DiffOpts) -> GitResult<Diff> {
        commit_diff(&self.inner, hash, opts)
    }

    /// The entry's stat and patch under `header`.
    pub fn stash_diff(&self, oid: &str, header: &str, opts: DiffOpts) -> GitResult<Diff> {
        stash_diff(&self.inner, oid, header, opts)
    }
}
