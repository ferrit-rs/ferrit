//! The `git2` and subprocess half of `crate::git::refs`: the types are there.

use crate::git::error::{GitError, GitResult};
use crate::git::model::BranchEntry;
use crate::git::refs::MergeOutcome;
use crate::git::repo::read::{stderr, workdir};
use crate::git::repo::read_error;
use crate::git::repo::{Repo, exec};
use git2::{BranchType, Repository};
use std::path::Path;
use std::process::Command;

/// Read local branches, HEAD first, then by recent tip and name.
pub(crate) fn branches(repo: &Repository) -> GitResult<Vec<BranchEntry>> {
    let mut out: Vec<BranchEntry> = repo
        .branches(Some(BranchType::Local))
        .map_err(read_error)?
        .map(|res| {
            let (branch, _) = res.map_err(read_error)?;
            let is_head = branch.is_head();
            let name = branch
                .name()
                .map_err(read_error)?
                .unwrap_or("(invalid utf-8)")
                .to_owned();
            let (upstream, ahead, behind) = match branch.upstream() {
                Ok(up) => {
                    let up_name = up.name().ok().flatten().map(str::to_owned);
                    let (ahead, behind) = match (branch.get().target(), up.get().target()) {
                        (Some(local), Some(remote)) => {
                            repo.graph_ahead_behind(local, remote).unwrap_or((0, 0))
                        },
                        _ => (0, 0),
                    };
                    (up_name, ahead, behind)
                },
                Err(_) => (None, 0, 0),
            };
            let tip_time = branch
                .get()
                .target()
                .and_then(|oid| repo.find_commit(oid).ok())
                .map_or(0, |commit| commit.time().seconds());
            Ok(BranchEntry {
                name,
                is_head,
                upstream,
                ahead,
                behind,
                tip_time,
            })
        })
        .collect::<GitResult<Vec<_>>>()?;

    out.sort_by(|a, b| match (a.is_head, b.is_head) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => b
            .tip_time
            .cmp(&a.tip_time)
            .then_with(|| a.name.cmp(&b.name)),
    });
    Ok(out)
}

// --- branch ---
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
pub(crate) fn checkout(repo: &Repository, name: &str) -> GitResult<()> {
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
pub(crate) fn create_branch(repo: &Repository, name: &str) -> GitResult<()> {
    let workdir = workdir(repo)?;
    run_git(
        workdir,
        |cmd| {
            cmd.arg("checkout").arg("-b").arg(name);
        },
        GitError::BranchFailed,
    )
}

/// `git checkout -b <name> <start> --no-track`: a new branch at a commit or ref
/// (`refs/heads/<branch>` for `n` on Branches), checked out, instead of always at
/// `HEAD` (`docs/PLAN_12_POLISH.md` P4); never tracks the branch it starts from.
pub(crate) fn create_branch_at(repo: &Repository, name: &str, hash: &str) -> GitResult<()> {
    let workdir = workdir(repo)?;
    run_git(
        workdir,
        |cmd| {
            cmd.arg("checkout")
                .arg("-b")
                .arg(name)
                .arg(hash)
                .arg("--no-track");
        },
        GitError::BranchFailed,
    )
}

/// `git branch -m <old> <new>`. Git's own refusals (a name already taken, an
/// invalid one) surface verbatim.
pub(crate) fn rename_branch(repo: &Repository, old: &str, new: &str) -> GitResult<()> {
    let workdir = workdir(repo)?;
    run_git(
        workdir,
        |cmd| {
            cmd.arg("branch").arg("-m").arg(old).arg(new);
        },
        GitError::BranchFailed,
    )
}

/// `git branch -d <name>` (or `-D` when `force`). Refuses the currently
/// checked-out branch the same way `git` does; that error surfaces
/// verbatim rather than being pre-checked here — `git` is the one source
/// of truth for "is this actually `HEAD`".
pub(crate) fn delete_branch(repo: &Repository, name: &str, force: bool) -> GitResult<()> {
    let workdir = workdir(repo)?;
    run_git(
        workdir,
        |cmd| {
            cmd.arg("branch")
                .arg(if force { "-D" } else { "-d" })
                .arg(name);
        },
        |stderr| {
            if stderr.contains("is not fully merged") {
                GitError::BranchNotMerged(stderr)
            } else {
                GitError::BranchFailed(stderr)
            }
        },
    )
}

/// Is `name` the currently checked-out branch? Decides `fast_forward`'s
/// mechanism.
fn is_checked_out(repo: &Repository, name: &str) -> bool {
    repo.find_branch(name, BranchType::Local)
        .is_ok_and(|b| b.is_head())
}

/// `name`'s upstream, as a full ref usable as a local fetch source.
/// `git2` read, not a subprocess: this is a lookup, not a mutation.
fn upstream_ref(repo: &Repository, name: &str) -> GitResult<String> {
    let branch = repo
        .find_branch(name, BranchType::Local)
        .map_err(read_error)?;
    let upstream = branch.upstream().map_err(|_| {
        GitError::BranchFailed(format!("no tracking information for branch '{name}'"))
    })?;
    Ok(upstream.get().name().map_err(read_error)?.to_owned())
}

/// Fast-forward `name` to its upstream. Two mechanisms depending on whether
/// `name` is checked out:
/// - checked out: `git merge --ff-only @{u}` (git refuses to let anything
///   else write to `HEAD`'s own branch, confirmed empirically — a local
///   `git fetch .` into the checked-out branch is rejected outright).
/// - not checked out: `git fetch . <upstream-ref>:refs/heads/<name>`
///   — a *local* fetch (source `.`, this same repository) that moves
///   `name`'s ref to match its already-known upstream ref, entirely from
///   data already on disk. No network, no auth: lazygit's own "fast-
///   forward a branch you're not on" trick. A plain (non-`+`-forced) `git
///   fetch` ref update already refuses a non-fast-forward, so no extra
///   safety check is needed on ferrit's side.
pub(crate) fn fast_forward(repo: &Repository, name: &str) -> GitResult<()> {
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
    let upstream = upstream_ref(repo, name)?;
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
pub(crate) fn merge_branch(repo: &Repository, name: &str) -> GitResult<MergeOutcome> {
    merge(repo, name, false)
}

#[allow(
    clippy::same_name_method,
    reason = "the `GitPort` branch role forwards to these methods under the same names"
)]
impl Repo {
    /// `git checkout <name>`. See `docs/PLAN_8_BRANCHES.md`.
    pub fn checkout(&self, name: &str) -> GitResult<()> {
        checkout(&self.inner, name)
    }

    /// `git checkout -b <name>` from the current `HEAD`.
    pub fn create_branch(&self, name: &str) -> GitResult<()> {
        create_branch(&self.inner, name)
    }

    /// `git branch -d <name>` (`-D` when `force`).
    pub fn delete_branch(&self, name: &str, force: bool) -> GitResult<()> {
        delete_branch(&self.inner, name, force)
    }

    /// Fast-forward `name` to its upstream, checked out or not.
    pub fn fast_forward(&self, name: &str) -> GitResult<()> {
        fast_forward(&self.inner, name)
    }

    /// `git checkout -b <name> <hash> --no-track`; `hash` may be a ref.
    pub fn create_branch_at(&self, name: &str, hash: &str) -> GitResult<()> {
        create_branch_at(&self.inner, name, hash)
    }

    /// `git branch -m <old> <new>`.
    pub fn rename_branch(&self, old: &str, new: &str) -> GitResult<()> {
        rename_branch(&self.inner, old, new)
    }

    /// `git merge --no-ff <name>`: always a merge commit.
    pub fn merge_branch_no_ff(&self, name: &str) -> GitResult<MergeOutcome> {
        merge_branch_no_ff(&self.inner, name)
    }

    /// `git merge --squash <name>`, then a commit when `commit` is set.
    pub fn merge_squash(&self, name: &str, commit: bool) -> GitResult<()> {
        merge_squash(&self.inner, name, commit)
    }

    /// `git merge <name>` into the current branch.
    pub fn merge_branch(&self, name: &str) -> GitResult<MergeOutcome> {
        merge_branch(&self.inner, name)
    }
}

/// `git merge --no-ff`: always a merge commit, even when a fast-forward would do.
pub(crate) fn merge_branch_no_ff(repo: &Repository, name: &str) -> GitResult<MergeOutcome> {
    merge(repo, name, true)
}

/// `git merge --squash`: the branch's changes land in the index and the
/// worktree, `HEAD` does not move. With `commit`, one ordinary commit follows.
/// A conflict is an error here: `--squash` writes no `MERGE_HEAD`, so there is
/// no merge in progress to continue; the conflicted files show in Files.
pub(crate) fn merge_squash(repo: &Repository, name: &str, commit: bool) -> GitResult<()> {
    let workdir = workdir(repo)?;
    run_git(
        workdir,
        |cmd| {
            cmd.args(["merge", "--squash", name]);
        },
        GitError::MergeFailed,
    )?;
    if commit {
        let message = format!("Squash merge '{name}'");
        run_git(
            workdir,
            |cmd| {
                cmd.args(["commit", "-m", &message]);
            },
            GitError::MergeFailed,
        )?;
    }
    Ok(())
}

fn merge(repo: &Repository, name: &str, no_ff: bool) -> GitResult<MergeOutcome> {
    let workdir = workdir(repo)?;
    let message = format!("Merge branch '{name}'");
    let mut cmd = exec::git(workdir);
    cmd.arg("merge");
    if no_ff {
        cmd.arg("--no-ff");
    }
    cmd.args(["-m", &message, name]);
    let out = exec::output(&mut cmd)
        .map_err(|e| GitError::MergeFailed(format!("cannot run git: {e}")))?;
    if out.status.success() {
        return Ok(MergeOutcome::Merged);
    }
    if repo.state() == git2::RepositoryState::Merge {
        return Ok(MergeOutcome::Conflicted);
    }
    Err(GitError::MergeFailed(stderr(&out)))
}
