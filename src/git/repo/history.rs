//! The `git2` and subprocess half of `crate::git::commit`: the types are there.
//! The `git2` and subprocess half of `crate::git::rebase`: the types are there.
//! The `git2` and subprocess half of `crate::git::operation`: the types are there.

use super::read::{stderr, workdir};
use crate::git::commit::{CommitKind, CommitOpts, INITIAL_FILE, INITIAL_MESSAGE};
use crate::git::error::{GitError, GitResult};
use crate::git::exec;
use crate::git::model::Operation;
use crate::git::operation::{OperationOutcome, Step, flag};
use crate::git::rebase::{RebaseEdit, build_todo, shell_quote};
use crate::git::repo::read_error;
use git2::{Oid, Repository, RepositoryState, Sort, Status, StatusOptions};
use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::Stdio;

// --- commit ---
/// Run `git commit` with `message` on stdin (`-F -`), except for `Fixup`,
/// which writes its own message and reads none. Returns the new `HEAD`'s
/// full hash on success.
pub(super) fn commit(
    repo: &Repository,
    kind: &CommitKind,
    message: &str,
    opts: CommitOpts,
) -> GitResult<String> {
    run(repo, kind, message, opts, None)
}

/// `commit`, optionally limited to one path (`--only -- <path>`): whatever else
/// is staged stays staged and out of the commit.
fn run(
    repo: &Repository,
    kind: &CommitKind,
    message: &str,
    opts: CommitOpts,
    only: Option<&str>,
) -> GitResult<String> {
    let workdir = workdir(repo)?;
    let writes_message = !matches!(kind, CommitKind::Fixup { .. });

    let mut args = vec!["commit".to_owned()];
    match kind {
        CommitKind::Normal => {},
        CommitKind::Amend => args.push("--amend".to_owned()),
        CommitKind::Reword => {
            args.push("--amend".to_owned());
            args.push("--only".to_owned());
        },
        CommitKind::Fixup { target } => args.push(format!("--fixup={target}")),
        CommitKind::Squash { target } => args.push(format!("--squash={target}")),
    }
    if opts.sign_off {
        args.push("-s".to_owned());
    }
    if opts.no_verify {
        args.push("--no-verify".to_owned());
    }
    if let Some(author) = opts.author {
        args.push(format!("--author={author}"));
    }
    if writes_message {
        args.push("-F".to_owned());
        args.push("-".to_owned());
    }
    if let Some(path) = only {
        args.extend(["--only".to_owned(), "--".to_owned(), path.to_owned()]);
    }

    let mut cmd = exec::git(workdir);
    cmd.args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let tracked = exec::track(&cmd);
    let mut child = cmd
        .spawn()
        .map_err(|e| GitError::CommitFailed(format!("cannot run git: {e}")))?;

    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| GitError::CommitFailed("no stdin pipe to git commit".to_owned()))?;
    if writes_message {
        stdin
            .write_all(message.as_bytes())
            .map_err(|e| GitError::CommitFailed(format!("cannot write message: {e}")))?;
    }
    drop(stdin);

    let out = child
        .wait_with_output()
        .map_err(|e| GitError::CommitFailed(format!("cannot run git: {e}")))?;
    tracked.finish_with_stdout(out.status.code(), &out.stdout);
    if !out.status.success() {
        // "nothing to commit" / "no changes added to commit" land on
        // stdout, not stderr — `git commit` treats them as ordinary status
        // output, not an error message, even though the exit code is 1.
        let out_text = String::from_utf8_lossy(&out.stdout);
        if out_text.contains("nothing to commit") || out_text.contains("no changes added to commit")
        {
            return Err(GitError::NothingStaged);
        }
        return Err(GitError::CommitFailed(stderr(&out)));
    }

    head_hash(repo)
}

/// Whether `HEAD` points at a commit (an unborn branch has none).
pub(super) fn has_commits(repo: &Repository) -> bool {
    repo.head().is_ok()
}

/// `HEAD`'s full hash, read straight after a successful `git commit`
/// subprocess so the caller can show/select the new commit.
fn head_hash(repo: &Repository) -> GitResult<String> {
    let head = repo.head().map_err(read_error)?;
    let oid = head
        .target()
        .ok_or_else(|| GitError::CommitFailed("HEAD has no target after commit".to_owned()))?;
    Ok(oid.to_string())
}

/// `HEAD`'s current message, for pre-filling the Amend / Reword box.
/// `None` on an unborn branch (nothing to amend or reword yet).
pub(super) fn head_message(repo: &Repository) -> GitResult<Option<String>> {
    match repo.head() {
        Ok(head) => {
            let oid = head
                .target()
                .ok_or_else(|| GitError::CommitFailed("HEAD has no target".to_owned()))?;
            let commit = repo.find_commit(oid).map_err(read_error)?;
            Ok(commit.message().ok().map(str::to_owned))
        },
        Err(e) if e.code() == git2::ErrorCode::UnbornBranch => Ok(None),
        Err(e) => Err(read_error(e)),
    }
}

/// The message `commit.template` names, for pre-filling a new commit, or
/// `None` when no template is set, it cannot be read, or nothing is left of it.
/// `~` is expanded by git's own path handling; a relative path is relative to
/// the work tree, where `git commit` runs. Lines starting with `#` are
/// dropped, as `git commit` does when it opens an editor (ferrit commits with
/// `-F`, which would keep them), and so is trailing whitespace.
pub(super) fn template(repo: &Repository) -> Option<String> {
    let mut path = repo.config().ok()?.get_path("commit.template").ok()?;
    if path.is_relative() {
        path = repo.workdir()?.join(path);
    }
    let text = fs::read_to_string(path).ok()?;
    let message = text
        .lines()
        .filter(|line| !line.starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n");
    let message = message.trim_end();
    (!message.is_empty()).then(|| message.to_owned())
}

/// Count of paths staged relative to `HEAD`, the commit popup's
/// precondition (`c` is disabled at 0).
pub(super) fn staged_count(repo: &Repository) -> GitResult<usize> {
    let mut opts = StatusOptions::new();
    opts.include_untracked(false);
    let statuses = repo.statuses(Some(&mut opts)).map_err(read_error)?;
    let staged = Status::INDEX_NEW
        | Status::INDEX_MODIFIED
        | Status::INDEX_DELETED
        | Status::INDEX_RENAMED
        | Status::INDEX_TYPECHANGE;
    Ok(statuses
        .iter()
        .filter(|e| e.status().intersects(staged))
        .count())
}

/// The first commit of a repository with none: an empty `README.md` (an
/// existing one is committed as it is, never overwritten), message
/// `INITIAL_MESSAGE`. Only that file goes in: anything else staged stays staged, and the
/// other files of the folder stay as they are. `Ok(false)` and nothing done when
/// the repository already has a commit, so asking twice is harmless. Hooks,
/// signing and the identity are `git commit`'s own. See
/// `docs/PLAN_15_CREATE_REMOTE.md`.
pub(super) fn initial_commit(repo: &Repository, author: Option<String>) -> GitResult<bool> {
    match repo.head() {
        Ok(_) => return Ok(false),
        Err(e) if e.code() == git2::ErrorCode::UnbornBranch => {},
        Err(e) => return Err(read_error(e)),
    }
    let workdir = workdir(repo)?;
    let readme = workdir.join(INITIAL_FILE);
    if !readme.exists() {
        fs::write(&readme, "")
            .map_err(|e| GitError::CommitFailed(format!("cannot create {INITIAL_FILE}: {e}")))?;
    }
    let mut add = exec::git(workdir);
    add.args(["add", "--", INITIAL_FILE]);
    let out = exec::output(&mut add)
        .map_err(|e| GitError::CommitFailed(format!("cannot run git: {e}")))?;
    if !out.status.success() {
        return Err(GitError::CommitFailed(stderr(&out)));
    }
    let opts = CommitOpts {
        author,
        ..CommitOpts::default()
    };
    run(
        repo,
        &CommitKind::Normal,
        INITIAL_MESSAGE,
        opts,
        Some(INITIAL_FILE),
    )?;
    Ok(true)
}

// --- rebase ---
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
    finish(repo, &dir, settle(repo, &out, GitError::RebaseFailed))
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
    finish(repo, &dir, settle(repo, &out, GitError::RebaseFailed))
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
    settled: GitResult<OperationOutcome>,
) -> GitResult<OperationOutcome> {
    if !matches!(settled, Ok(OperationOutcome::Stopped { .. })) && current(repo).is_none() {
        let _ = fs::remove_dir_all(dir);
    }
    settled
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

// --- operation ---
/// The operation in progress, or `None` for a clean repository. Bisect and
/// mailbox (`git am`) states map to `None`: ferrit has no flow for them, and
/// `ApplyMailboxOrRebase` cannot be told apart from a plain `am`.
pub(super) fn current(repo: &Repository) -> Option<Operation> {
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
        // The `--apply` backend.
        RepositoryState::Rebase => Some(rebase_progress(repo, "rebase-apply", "next", "last")),
        // The default (merge) backend, interactive or not.
        RepositoryState::RebaseInteractive | RepositoryState::RebaseMerge => {
            Some(rebase_progress(repo, "rebase-merge", "msgnum", "end"))
        },
    }
}

/// `step` and `total` from the files git keeps in `<git dir>/<dir>/`.
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

/// Run `git <operation> --continue|--skip|--abort` for whatever operation is
/// in progress, with the editor neutralised (`GIT_EDITOR=true`: ferrit owns the
/// terminal, an editor would hang).
///
/// The outcome comes from the repository afterwards, not the exit code: a
/// `--continue` that reaches the next conflicting commit exits non-zero and
/// is `Stopped`, while git refusing to continue over an unresolved file also
/// exits non-zero and is an error. The two are told apart by git's
/// `CONFLICT (` report, which only a fresh conflict prints.
pub(super) fn step(repo: &Repository, step: Step) -> GitResult<OperationOutcome> {
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
        .map_err(|e| GitError::OperationFailed(format!("cannot run git: {e}")))?;

    settle(repo, &out, GitError::OperationFailed)
}

/// Where the repository stands after a subprocess that may have started or
/// advanced an operation, or the refusal text. Shared by `step` and by
/// `rebase`, so both read the same signals.
///
/// A failure is `Stopped` only when git reported a fresh `CONFLICT (` and an
/// operation is in progress. Any other failure is the refusal text, even if an
/// operation is still in progress (a rejecting hook, a `--continue` over an
/// unresolved file): the caller shows it, and the Status badge and `m` menu
/// are how the user leaves that state. `refuse` wraps that text in the error
/// variant of the operation that ran.
pub(super) fn settle(
    repo: &Repository,
    out: &std::process::Output,
    refuse: fn(String) -> GitError,
) -> GitResult<OperationOutcome> {
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let conflicted = repo.index().is_ok_and(|mut index| {
        // The subprocess rewrote the index; drop the cached copy.
        index.read(true).is_ok() && index.has_conflicts()
    });
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
