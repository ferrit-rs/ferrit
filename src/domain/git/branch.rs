//! Checkout, create, delete, fast-forward and merge branches by shelling
//! out to `git`, so hooks (`post-checkout`, `pre-merge-commit`,
//! `post-merge`) and git's own safety messaging (a dirty worktree an
//! checkout would clobber, an unmerged delete, a merge conflict) apply the
//! way they do for the user's own `git`. See `docs/PLAN_8_BRANCHES.md`.
//!
//! Same rule as the rest of `git::`: no `ratatui`, one subprocess per
//! action. Reads (is a branch checked out, what is its upstream) still go
//! through `git2`, the same split `apply.rs`/`commit.rs` already make.

use std::path::Path;
use std::process::Command;

use git2::{BranchType, Repository};

use crate::domain::git::diff::{stderr, workdir};
use crate::domain::git::error::{GitError, GitResult};
use crate::domain::git::exec;

/// What a merge actually did. Not a plain `()`: "it worked" has two shapes
/// ferrit's UI treats differently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MergeOutcome {
    /// Exit 0: a merge commit (or a fast-forward git decided to do anyway)
    /// landed clean.
    Merged,
    /// Exit non-zero, but `repo.state()` shows a merge in progress
    /// (`MERGE_HEAD` written, conflict markers in the worktree) rather
    /// than some other failure. Ordinary, expected git behaviour ferrit
    /// currently has no UI for finishing, so this is not a `GitError`.
    Conflicted,
}

/// `git -C <workdir> <...args>`, run to completion, mapping a non-zero exit
/// to `err(stderr)`. Same shape as `apply.rs::run_git`, kept separate: the
/// two modules map failure to different `GitError` variants, and threading
/// that mapping through one shared helper would obscure more than the
/// handful of duplicated lines save.
fn run_git(
    workdir: &Path,
    build: impl FnOnce(&mut Command),
    err: impl Fn(String) -> GitError,
) -> GitResult<()> {
    let mut cmd = exec::git(workdir);
    build(&mut cmd);
    let out = exec::output(&mut cmd).map_err(|e| err(format!("cannot run git: {e}")))?;
    if !out.status.success() {
        return Err(err(stderr(&out)));
    }
    Ok(())
}

/// `git checkout <name>`. Fails (message verbatim) on a dirty worktree
/// that checkout would clobber; ferrit does not stash-and-pop on the
/// user's behalf. A no-op, exit 0, on the already-checked-out branch —
/// not special-cased.
pub(super) fn checkout(repo: &Repository, name: &str) -> GitResult<()> {
    let workdir = workdir(repo)?;
    run_git(
        workdir,
        |cmd| {
            cmd.arg("checkout").arg(name);
        },
        GitError::CheckoutFailed,
    )
}

/// `git checkout -b <name>`, always from the current `HEAD` (branching
/// from an arbitrary commit is a deferred nicety, see this plan's "After
/// phase 8").
pub(super) fn create_branch(repo: &Repository, name: &str) -> GitResult<()> {
    let workdir = workdir(repo)?;
    run_git(
        workdir,
        |cmd| {
            cmd.arg("checkout").arg("-b").arg(name);
        },
        GitError::BranchFailed,
    )
}

/// `git branch -d <name>` (or `-D` when `force`). Refuses the currently
/// checked-out branch the same way `git` does; that error surfaces
/// verbatim rather than being pre-checked here — `git` is the one source
/// of truth for "is this actually `HEAD`".
pub(super) fn delete_branch(repo: &Repository, name: &str, force: bool) -> GitResult<()> {
    let workdir = workdir(repo)?;
    run_git(
        workdir,
        |cmd| {
            cmd.arg("branch")
                .arg(if force { "-D" } else { "-d" })
                .arg(name);
        },
        GitError::BranchFailed,
    )
}

/// Is `name` the currently checked-out branch? Decides `fast_forward`'s
/// mechanism.
fn is_checked_out(repo: &Repository, name: &str) -> bool {
    repo.find_branch(name, BranchType::Local)
        .is_ok_and(|b| b.is_head())
}

/// `name`'s upstream, in the short form (`origin/main`) usable as a fetch
/// source. `git2` read, not a subprocess: this is a lookup, not a mutation.
fn upstream_shorthand(repo: &Repository, name: &str) -> GitResult<String> {
    let branch = repo
        .find_branch(name, BranchType::Local)
        .map_err(GitError::Read)?;
    let upstream = branch.upstream().map_err(|_| {
        GitError::BranchFailed(format!("no tracking information for branch '{name}'"))
    })?;
    upstream
        .name()
        .map_err(GitError::Read)?
        .map(str::to_owned)
        .ok_or_else(|| {
            GitError::BranchFailed(format!("no tracking information for branch '{name}'"))
        })
}

/// Fast-forward `name` to its upstream. Two mechanisms depending on whether
/// `name` is checked out:
/// - checked out: `git merge --ff-only @{u}` (git refuses to let anything
///   else write to `HEAD`'s own branch, confirmed empirically — a local
///   `git fetch .` into the checked-out branch is rejected outright).
/// - not checked out: `git fetch . <upstream-shorthand>:refs/heads/<name>`
///   — a *local* fetch (source `.`, this same repository) that moves
///   `name`'s ref to match its already-known upstream ref, entirely from
///   data already on disk. No network, no auth: lazygit's own "fast-
///   forward a branch you're not on" trick. A plain (non-`+`-forced) `git
///   fetch` ref update already refuses a non-fast-forward, so no extra
///   safety check is needed on ferrit's side.
pub(super) fn fast_forward(repo: &Repository, name: &str) -> GitResult<()> {
    let workdir = workdir(repo)?;
    if is_checked_out(repo, name) {
        return run_git(
            workdir,
            |cmd| {
                cmd.args(["merge", "--ff-only", "@{u}"]);
            },
            GitError::BranchFailed,
        );
    }
    let upstream = upstream_shorthand(repo, name)?;
    let refspec = format!("{upstream}:refs/heads/{name}");
    run_git(
        workdir,
        |cmd| {
            cmd.arg("fetch").arg(".").arg(&refspec);
        },
        GitError::BranchFailed,
    )
}

/// `git merge -m <default message> <name>` into the current branch. A real
/// merge, not `--ff-only`: may create a merge commit (unless git decides a
/// fast-forward works), may conflict. `-m` supplies the message directly,
/// same trick `commit.rs` uses to avoid a `core.editor` spawn; git still
/// fast-forwards on its own when the history allows it, `-m` only matters
/// once a real merge commit is needed.
///
/// Exit code, checked empirically rather than trusted from memory: a real
/// conflict is exit *non-zero* (`git merge` does not exit 0 and leave
/// `MERGE_HEAD` behind, unlike the assumption this plan started from) —
/// `repo.state()` is what tells a conflict apart from any other failure.
pub(super) fn merge_branch(repo: &Repository, name: &str) -> GitResult<MergeOutcome> {
    let workdir = workdir(repo)?;
    let message = format!("Merge branch '{name}'");
    let out = exec::output(exec::git(workdir).args(["merge", "-m", &message, name]))
        .map_err(|e| GitError::MergeFailed(format!("cannot run git: {e}")))?;
    if out.status.success() {
        return Ok(MergeOutcome::Merged);
    }
    if repo.state() == git2::RepositoryState::Merge {
        return Ok(MergeOutcome::Conflicted);
    }
    Err(GitError::MergeFailed(stderr(&out)))
}
