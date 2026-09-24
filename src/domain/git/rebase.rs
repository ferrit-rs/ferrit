//! Rewrite history from the Commits pane: reword, drop, edit, squash or fixup
//! one commit, and fold pending `fixup!` commits, each as one
//! `git rebase -i` run with a todo ferrit generates. See
//! `docs/PLAN_11_REBASE.md`.
//!
//! ferrit never opens `$EDITOR` (the terminal is in raw mode on an alternate
//! screen). The todo is supplied through `GIT_SEQUENCE_EDITOR="cp <file>"` and
//! every other editor is `GIT_EDITOR=true`.
//!
//! A reword is a `pick` followed by `exec git commit --amend -F <file>`, not a
//! `reword` line: the exec line lives in git's own copy of the todo, so it
//! still runs after the rebase stopped on a conflict earlier in the range and
//! was continued with a neutral editor. The message file therefore outlives
//! the run that created it; it is removed once the rebase is done.

use std::fs;
use std::path::{Path, PathBuf};

use git2::{Oid, Repository, Sort};

use crate::domain::git::diff::workdir;
use crate::domain::git::error::{GitError, GitResult};
use crate::domain::git::exec;
use crate::domain::git::operation::{OperationOutcome, current, settle};

/// What to do with the selected commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RebaseEdit {
    /// Replace its message (the new one, whole).
    Reword(String),
    Drop,
    /// Stop at it, leaving the rebase in progress.
    Edit,
    /// Fold into the commit below it, keeping both messages.
    Squash,
    /// Fold into the commit below it, dropping this message.
    Fixup,
}

/// The todo for `commits` (full hashes, oldest first) with `target` carrying
/// `edit` and every other commit picked. A reword is a `pick` plus an `exec`
/// amend reading `message_file`.
pub fn build_todo(
    commits: &[String],
    target: &str,
    edit: &RebaseEdit,
    message_file: &Path,
) -> String {
    let mut lines: Vec<String> = Vec::with_capacity(commits.len() + 1);
    for hash in commits {
        if hash != target {
            lines.push(format!("pick {hash}"));
            continue;
        }
        match edit {
            RebaseEdit::Reword(_) => {
                lines.push(format!("pick {hash}"));
                lines.push(format!(
                    "exec git commit --amend -q --allow-empty -F {}",
                    shell_quote(&message_file.to_string_lossy())
                ));
            },
            RebaseEdit::Drop => lines.push(format!("drop {hash}")),
            RebaseEdit::Edit => lines.push(format!("edit {hash}")),
            RebaseEdit::Squash => lines.push(format!("squash {hash}")),
            RebaseEdit::Fixup => lines.push(format!("fixup {hash}")),
        }
    }
    if lines.is_empty() {
        return String::new();
    }
    let mut todo = lines.join("\n");
    todo.push('\n');
    todo
}

/// `text` as one shell word, safe for a path containing spaces or quotes.
fn shell_quote(text: &str) -> String {
    format!("'{}'", text.replace('\'', "'\\''"))
}

/// Full message of the commit `hash`, for pre-filling the reword popup.
pub(super) fn commit_message(repo: &Repository, hash: &str) -> GitResult<String> {
    let commit = find_commit(repo, hash)?;
    Ok(commit.message().unwrap_or_default().trim_end().to_owned())
}

fn find_commit<'r>(repo: &'r Repository, hash: &str) -> GitResult<git2::Commit<'r>> {
    Oid::from_str(hash)
        .and_then(|oid| repo.find_commit(oid))
        .map_err(|_| GitError::NoSuchCommit(hash.to_owned()))
}

/// Reword, drop, edit, squash or fixup `hash` (a full `CommitEntry` hash).
pub(super) fn rebase_edit(
    repo: &Repository,
    hash: &str,
    edit: &RebaseEdit,
) -> GitResult<OperationOutcome> {
    ensure_idle(repo)?;
    let target = find_commit(repo, hash)?;
    // Squash and fixup fold into the commit below, so the range must reach
    // one commit further back to include it.
    let anchor = match edit {
        RebaseEdit::Squash | RebaseEdit::Fixup => {
            let below = target
                .parent(0)
                .map_err(|_| GitError::RebaseFailed("no commit below to fold into".to_owned()))?;
            below.parent(0).ok().map(|p| p.id())
        },
        _ => target.parent(0).ok().map(|p| p.id()),
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
    finish(repo, &dir, settle(repo, &out))
}

/// Fold every `fixup!` / `squash!` commit after `hash`'s parent into its
/// target (`git rebase -i --autosquash`).
pub(super) fn autosquash(repo: &Repository, hash: &str) -> GitResult<OperationOutcome> {
    ensure_idle(repo)?;
    let target = find_commit(repo, hash)?;
    let anchor = target.parent(0).ok().map(|p| p.id());
    linear_range(repo, anchor)?;
    let dir = scratch_dir(repo)?;

    let mut cmd = rebase_command(repo, anchor)?;
    cmd.arg("--autosquash").env("GIT_SEQUENCE_EDITOR", "true");
    let out = exec::output(&mut cmd)
        .map_err(|e| GitError::RebaseFailed(format!("cannot run git: {e}")))?;
    finish(repo, &dir, settle(repo, &out))
}

/// Refuse to start a rewrite while a merge, rebase, cherry-pick or revert is
/// stopped. Checked first: mid-rebase `HEAD` is a half-rewritten history.
fn ensure_idle(repo: &Repository) -> GitResult<()> {
    if current(repo).is_some() {
        return Err(GitError::RebaseFailed(
            "an operation is already in progress".to_owned(),
        ));
    }
    Ok(())
}

/// The commits after `anchor` up to `HEAD`, oldest first, as full hashes;
/// `anchor == None` means the whole history. Refuses a range holding a merge
/// commit and an unborn branch.
fn linear_range(repo: &Repository, anchor: Option<Oid>) -> GitResult<Vec<String>> {
    let mut walk = repo.revwalk().map_err(GitError::Read)?;
    walk.push_head()
        .map_err(|_| GitError::RebaseFailed("there are no commits yet".to_owned()))?;
    if let Some(anchor) = anchor {
        walk.hide(anchor).map_err(GitError::Read)?;
    }
    walk.set_sorting(Sort::TOPOLOGICAL | Sort::REVERSE)
        .map_err(GitError::Read)?;

    let mut commits = Vec::new();
    for oid in walk {
        let oid = oid.map_err(GitError::Read)?;
        let commit = repo.find_commit(oid).map_err(GitError::Read)?;
        if commit.parent_count() > 1 {
            return Err(GitError::RebaseFailed(
                "the range holds a merge commit; ferrit rebases linear history only".to_owned(),
            ));
        }
        commits.push(oid.to_string());
    }
    Ok(commits)
}

/// `git rebase -i <anchor>` (or `--root`), the editors neutralised.
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
    exec::output(&mut cmd).map_err(|e| GitError::RebaseFailed(format!("cannot run git: {e}")))
}

/// Map the settled result to the API's, and tidy the scratch directory unless
/// the rebase is still running (its message file may yet be needed).
fn finish(
    repo: &Repository,
    dir: &Path,
    settled: Result<OperationOutcome, String>,
) -> GitResult<OperationOutcome> {
    if !matches!(settled, Ok(OperationOutcome::Stopped { .. })) && current(repo).is_none() {
        let _ = fs::remove_dir_all(dir);
    }
    settled.map_err(GitError::RebaseFailed)
}

/// `<git dir>/ferrit/`, emptied. Never inside the worktree: a helper file
/// there would show up as an untracked change.
fn scratch_dir(repo: &Repository) -> GitResult<PathBuf> {
    let dir = repo.path().join("ferrit");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir)
        .map_err(|e| GitError::RebaseFailed(format!("cannot create {}: {e}", dir.display())))?;
    Ok(dir)
}

fn write(path: &Path, text: &str) -> GitResult<()> {
    fs::write(path, text)
        .map_err(|e| GitError::RebaseFailed(format!("cannot write {}: {e}", path.display())))
}
