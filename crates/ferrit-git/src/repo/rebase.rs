//! The `git2` and subprocess half of `ferrit_domain::rebase`.

use crate::repo::read::workdir;
use crate::repo::read_error;
use crate::repo::{Repo, exec};
use ferrit_domain::error::{GitError, GitResult};
use ferrit_domain::model::Operation;
use ferrit_domain::rebase::{OperationOutcome, Step, flag};
use ferrit_domain::rebase::{RebaseEdit, build_todo, shell_quote};
use git2::{Oid, Repository, RepositoryState, Sort};
use std::fs;
use std::path::{Path, PathBuf};

/// Full message of the commit `hash`, for pre-filling the reword popup.
pub(crate) fn commit_message(repo: &Repository, hash: &str) -> GitResult<String> {
    let commit = find_commit(repo, hash)?;
    Ok(commit.message().unwrap_or_default().trim_end().to_owned())
}

fn find_commit<'r>(repo: &'r Repository, hash: &str) -> GitResult<git2::Commit<'r>> {
    Oid::from_str(hash)
        .and_then(|oid| repo.find_commit(oid))
        .map_err(|_| GitError::NoSuchCommit(hash.to_owned()))
}

/// Reword, drop, edit, squash or fixup `hash` (a full `CommitEntry` hash).
pub(crate) fn rebase_edit(
    repo: &Repository,
    hash: &str,
    edit: &RebaseEdit,
) -> GitResult<OperationOutcome> {
    ensure_idle(repo)?;
    let target = find_commit(repo, hash)?;
    let anchor = match edit {
        RebaseEdit::Squash | RebaseEdit::Fixup => {
            let below = target
                .parent(0)
                .map_err(|_| GitError::RebaseFailed("no commit below to fold into".to_owned()))?;
            below.parent(0).ok().map(|parent| parent.id())
        },
        _ => target.parent(0).ok().map(|parent| parent.id()),
    };
    let commits = linear_range(repo, anchor)?;
    let target_hash = target.id().to_string();
    if !commits.contains(&target_hash) {
        return Err(GitError::RebaseFailed(
            "that commit is not on the current branch".to_owned(),
        ));
    }

    let dir = scratch_dir(repo)?;
    let message_file = dir.join("message");
    let todo_file = dir.join("todo");
    if let RebaseEdit::Reword(message) = edit {
        write(&message_file, &format!("{}\n", message.trim_end()))?;
    }
    write(
        &todo_file,
        &build_todo(&commits, &target_hash, edit, &message_file),
    )?;

    let out = run_rebase(repo, anchor, &todo_file)?;
    finish(repo, &dir, settle(repo, &out, GitError::RebaseFailed))
}

/// Fold every `fixup!` / `squash!` commit after `hash`'s parent into its
/// target (`git rebase -i --autosquash`).
pub(crate) fn autosquash(repo: &Repository, hash: &str) -> GitResult<OperationOutcome> {
    ensure_idle(repo)?;
    let target = find_commit(repo, hash)?;
    let anchor = target.parent(0).ok().map(|parent| parent.id());
    linear_range(repo, anchor)?;
    let dir = scratch_dir(repo)?;

    let mut cmd = rebase_command(repo, anchor)?;
    cmd.arg("--autosquash").env("GIT_SEQUENCE_EDITOR", "true");
    let out = exec::output(&mut cmd)
        .map_err(|error| GitError::RebaseFailed(format!("cannot run git: {error}")))?;
    finish(repo, &dir, settle(repo, &out, GitError::RebaseFailed))
}

fn ensure_idle(repo: &Repository) -> GitResult<()> {
    if current(repo).is_some() {
        return Err(GitError::RebaseFailed(
            "an operation is already in progress".to_owned(),
        ));
    }
    Ok(())
}

fn linear_range(repo: &Repository, anchor: Option<Oid>) -> GitResult<Vec<String>> {
    let mut walk = repo.revwalk().map_err(read_error)?;
    walk.push_head()
        .map_err(|_| GitError::RebaseFailed("there are no commits yet".to_owned()))?;
    if let Some(anchor) = anchor {
        walk.hide(anchor).map_err(read_error)?;
    }
    walk.set_sorting(Sort::TOPOLOGICAL | Sort::REVERSE)
        .map_err(read_error)?;

    let mut commits = Vec::new();
    for oid in walk {
        let oid = oid.map_err(read_error)?;
        let commit = repo.find_commit(oid).map_err(read_error)?;
        if commit.parent_count() > 1 {
            return Err(GitError::RebaseFailed(
                "the range holds a merge commit; ferrit rebases linear history only".to_owned(),
            ));
        }
        commits.push(oid.to_string());
    }
    Ok(commits)
}

fn rebase_command(repo: &Repository, anchor: Option<Oid>) -> GitResult<std::process::Command> {
    let mut cmd = exec::git(workdir(repo)?);
    cmd.env("GIT_EDITOR", "true").arg("rebase").arg("-i");
    match anchor {
        Some(oid) => cmd.arg(oid.to_string()),
        None => cmd.arg("--root"),
    };
    Ok(cmd)
}

fn run_rebase(
    repo: &Repository,
    anchor: Option<Oid>,
    todo_file: &Path,
) -> GitResult<std::process::Output> {
    let mut cmd = rebase_command(repo, anchor)?;
    cmd.env(
        "GIT_SEQUENCE_EDITOR",
        format!("cp {}", shell_quote(&todo_file.to_string_lossy())),
    );
    exec::output(&mut cmd)
        .map_err(|error| GitError::RebaseFailed(format!("cannot run git: {error}")))
}

fn finish(
    repo: &Repository,
    dir: &Path,
    settled: GitResult<OperationOutcome>,
) -> GitResult<OperationOutcome> {
    if !matches!(settled, Ok(OperationOutcome::Stopped { .. })) && current(repo).is_none() {
        let _ = fs::remove_dir_all(dir);
    }
    settled
}

fn scratch_dir(repo: &Repository) -> GitResult<PathBuf> {
    let dir = repo.path().join("ferrit");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).map_err(|error| {
        GitError::RebaseFailed(format!("cannot create {}: {error}", dir.display()))
    })?;
    Ok(dir)
}

fn write(path: &Path, text: &str) -> GitResult<()> {
    fs::write(path, text).map_err(|error| {
        GitError::RebaseFailed(format!("cannot write {}: {error}", path.display()))
    })
}

/// The operation in progress, or `None` for a clean repository.
pub(crate) fn current(repo: &Repository) -> Option<Operation> {
    match repo.state() {
        RepositoryState::Clean
        | RepositoryState::Bisect
        | RepositoryState::ApplyMailbox
        | RepositoryState::ApplyMailboxOrRebase => None,
        RepositoryState::Merge => Some(Operation::Merge),
        RepositoryState::Revert | RepositoryState::RevertSequence => Some(Operation::Revert),
        RepositoryState::CherryPick | RepositoryState::CherryPickSequence => {
            Some(Operation::CherryPick)
        },
        RepositoryState::Rebase => Some(rebase_progress(repo, "rebase-apply", "next", "last")),
        RepositoryState::RebaseInteractive | RepositoryState::RebaseMerge => {
            Some(rebase_progress(repo, "rebase-merge", "msgnum", "end"))
        },
    }
}

fn rebase_progress(repo: &Repository, dir: &str, step_file: &str, total_file: &str) -> Operation {
    let read = |name: &str| {
        fs::read_to_string(repo.path().join(dir).join(name))
            .ok()
            .and_then(|text| text.trim().parse().ok())
            .unwrap_or(0)
    };
    Operation::Rebase {
        step: read(step_file),
        total: read(total_file),
    }
}

pub(crate) fn step(repo: &Repository, step: Step) -> GitResult<OperationOutcome> {
    let Some(operation) = current(repo) else {
        return Err(GitError::OperationFailed(
            "no operation in progress".to_owned(),
        ));
    };
    let (command, flag) = match (operation, step) {
        (Operation::Merge, Step::Skip) => {
            return Err(GitError::OperationFailed(
                "a merge cannot be skipped".to_owned(),
            ));
        },
        (Operation::Merge, _) => ("merge", flag(step)),
        (Operation::Rebase { .. }, _) => ("rebase", flag(step)),
        (Operation::CherryPick, _) => ("cherry-pick", flag(step)),
        (Operation::Revert, _) => ("revert", flag(step)),
    };
    let mut cmd = exec::git(workdir(repo)?);
    cmd.env("GIT_EDITOR", "true").arg(command).arg(flag);
    let out = exec::output(&mut cmd)
        .map_err(|error| GitError::OperationFailed(format!("cannot run git: {error}")))?;

    settle(repo, &out, GitError::OperationFailed)
}

pub(crate) fn settle(
    repo: &Repository,
    out: &std::process::Output,
    refuse: fn(String) -> GitError,
) -> GitResult<OperationOutcome> {
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let conflicted = repo
        .index()
        .is_ok_and(|mut index| index.read(true).is_ok() && index.has_conflicts());
    if out.status.success() {
        return Ok(match current(repo) {
            None => OperationOutcome::Done,
            Some(_) => OperationOutcome::Stopped { conflicted },
        });
    }
    if text.contains("CONFLICT (") && current(repo).is_some() {
        return Ok(OperationOutcome::Stopped { conflicted: true });
    }
    Err(refuse(text.trim().to_owned()))
}

#[allow(
    clippy::same_name_method,
    reason = "the `GitPort` history role forwards to these methods under the same names"
)]
impl Repo {
    /// Apply one interactive rebase edit to a commit.
    pub fn rebase_edit(&self, hash: &str, edit: &RebaseEdit) -> GitResult<OperationOutcome> {
        rebase_edit(&self.inner, hash, edit)
    }

    /// Autosquash fixup and squash commits after `hash`.
    pub fn autosquash(&self, hash: &str) -> GitResult<OperationOutcome> {
        autosquash(&self.inner, hash)
    }

    /// Read a commit message for a reword editor.
    pub fn commit_message(&self, hash: &str) -> GitResult<String> {
        commit_message(&self.inner, hash)
    }

    /// Continue, skip or abort the current Git operation.
    pub fn operation_step(&self, operation: Step) -> GitResult<OperationOutcome> {
        step(&self.inner, operation)
    }

    /// Return the merge, rebase, cherry-pick or revert currently in progress.
    pub fn operation(&self) -> Option<Operation> {
        current(&self.inner)
    }
}
