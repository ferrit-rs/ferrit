//! The `git2` and subprocess half of `ferrit_domain::commit`.

use crate::repo::read::{stderr, workdir};
use crate::repo::read_error;
use crate::repo::{Repo, exec};
use ferrit_domain::commit::{CommitKind, CommitOpts, INITIAL_FILE, INITIAL_MESSAGE};
use ferrit_domain::error::{GitError, GitResult};
use git2::{Repository, Status, StatusOptions};
use std::fs;
use std::io::Write as _;
use std::process::Stdio;

/// Run `git commit` with `message` on stdin (`-F -`), except for `Fixup`.
pub(crate) fn commit(
    repo: &Repository,
    kind: &CommitKind,
    message: &str,
    opts: CommitOpts,
) -> GitResult<String> {
    run(repo, kind, message, opts, None)
}

/// `commit`, optionally limited to one path (`--only -- <path>`).
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
            args.extend(["--amend".to_owned(), "--only".to_owned()]);
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
        args.extend(["-F".to_owned(), "-".to_owned()]);
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
        .map_err(|error| GitError::CommitFailed(format!("cannot run git: {error}")))?;
    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| GitError::CommitFailed("no stdin pipe to git commit".to_owned()))?;
    if writes_message {
        stdin
            .write_all(message.as_bytes())
            .map_err(|error| GitError::CommitFailed(format!("cannot write message: {error}")))?;
    }
    drop(stdin);

    let out = child
        .wait_with_output()
        .map_err(|error| GitError::CommitFailed(format!("cannot run git: {error}")))?;
    tracked.finish_with_stdout(out.status.code(), &out.stdout);
    if !out.status.success() {
        let out_text = String::from_utf8_lossy(&out.stdout);
        if out_text.contains("nothing to commit") || out_text.contains("no changes added to commit")
        {
            return Err(GitError::NothingStaged);
        }
        return Err(GitError::CommitFailed(stderr(&out)));
    }

    head_hash(repo)
}

pub(crate) fn has_commits(repo: &Repository) -> bool {
    repo.head().is_ok()
}

fn head_hash(repo: &Repository) -> GitResult<String> {
    let head = repo.head().map_err(read_error)?;
    let oid = head
        .target()
        .ok_or_else(|| GitError::CommitFailed("HEAD has no target after commit".to_owned()))?;
    Ok(oid.to_string())
}

pub(crate) fn head_message(repo: &Repository) -> GitResult<Option<String>> {
    match repo.head() {
        Ok(head) => {
            let oid = head
                .target()
                .ok_or_else(|| GitError::CommitFailed("HEAD has no target".to_owned()))?;
            let commit = repo.find_commit(oid).map_err(read_error)?;
            Ok(commit.message().ok().map(str::to_owned))
        },
        Err(error) if error.code() == git2::ErrorCode::UnbornBranch => Ok(None),
        Err(error) => Err(read_error(error)),
    }
}

pub(crate) fn template(repo: &Repository) -> Option<String> {
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

pub(crate) fn staged_count(repo: &Repository) -> GitResult<usize> {
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
        .filter(|entry| entry.status().intersects(staged))
        .count())
}

pub(crate) fn initial_commit(repo: &Repository, author: Option<String>) -> GitResult<bool> {
    match repo.head() {
        Ok(_) => return Ok(false),
        Err(error) if error.code() == git2::ErrorCode::UnbornBranch => {},
        Err(error) => return Err(read_error(error)),
    }
    let workdir = workdir(repo)?;
    let readme = workdir.join(INITIAL_FILE);
    if !readme.exists() {
        fs::write(&readme, "").map_err(|error| {
            GitError::CommitFailed(format!("cannot create {INITIAL_FILE}: {error}"))
        })?;
    }
    let mut add = exec::git(workdir);
    add.args(["add", "--", INITIAL_FILE]);
    let out = exec::output(&mut add)
        .map_err(|error| GitError::CommitFailed(format!("cannot run git: {error}")))?;
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

#[allow(
    clippy::same_name_method,
    reason = "the `GitPort` history role forwards commit methods under their public names"
)]
impl Repo {
    /// Run `git commit` with the requested commit kind and options.
    pub fn commit(&self, kind: &CommitKind, message: &str, opts: CommitOpts) -> GitResult<String> {
        commit(&self.inner, kind, message, opts)
    }

    /// Whether the current branch has a commit.
    pub fn has_commits(&self) -> bool {
        has_commits(&self.inner)
    }

    /// Create the initial README commit when the repository has no commits.
    pub fn initial_commit(&self, author: Option<String>) -> GitResult<bool> {
        initial_commit(&self.inner, author)
    }

    /// Read the configured commit template with comments removed.
    pub fn commit_template(&self) -> Option<String> {
        template(&self.inner)
    }

    /// Read `HEAD`'s message for an amend or reword editor.
    pub fn head_message(&self) -> GitResult<Option<String>> {
        head_message(&self.inner)
    }

    /// Count paths staged relative to `HEAD`.
    pub fn staged_count(&self) -> GitResult<usize> {
        staged_count(&self.inner)
    }
}
