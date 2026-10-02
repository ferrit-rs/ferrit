//! Create commits by shelling out to `git commit`, so hooks
//! (`pre-commit`/`commit-msg`/`post-commit`), GPG/SSH signing, and
//! `commit.*` config all apply the way they do for the user's own `git`.
//! See `docs/PLAN_7_COMMIT.md`.
//!
//! No `ratatui` import, same rule as the rest of `git::`.

use std::io::Write as _;
use std::process::Stdio;

use git2::{Repository, Status, StatusOptions};

use crate::domain::git::diff::{stderr, workdir};
use crate::domain::git::error::{GitError, GitResult};
use crate::domain::git::exec;

/// What kind of commit to make.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommitKind {
    Normal,
    Amend,
    /// Amend the message only (`--amend --only`), ignoring whatever is
    /// currently staged.
    Reword,
    /// `git commit --fixup=<target>`. `target` is a full commit hash; git
    /// writes the `fixup! <subject>` message itself, so `commit`'s own
    /// `message` argument is ignored for this kind.
    Fixup {
        target: String,
    },
    /// `git commit --squash=<target>`, with a caller-supplied message.
    Squash {
        target: String,
    },
}

impl CommitKind {
    /// Popup title, `docs/PLAN_7_COMMIT.md`'s "Amend HEAD" / "Reword HEAD".
    pub fn title(&self) -> &'static str {
        match self {
            Self::Normal => "Commit",
            Self::Amend => "Amend HEAD",
            Self::Reword => "Reword HEAD",
            Self::Fixup { .. } => "Fixup",
            Self::Squash { .. } => "Squash",
        }
    }
}

/// Per-commit toggles, both visible in the popup footer
/// (`docs/PLAN_7_COMMIT.md` "Sign-off default": never a silent `-s`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CommitOpts {
    pub sign_off: bool,
    pub no_verify: bool,
    pub author: Option<String>,
}

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

/// The file the first commit holds.
const INITIAL_FILE: &str = "README.md";

/// The first commit of a repository with none: an empty `README.md` (an
/// existing one is committed as it is, never overwritten), message `Initial
/// commit`. Only that file goes in: anything else staged stays staged, and the
/// other files of the folder stay as they are. `Ok(false)` and nothing done when
/// the repository already has a commit, so asking twice is harmless. Hooks,
/// signing and the identity are `git commit`'s own. See
/// `docs/PLAN_15_CREATE_REMOTE.md`.
pub(super) fn initial_commit(repo: &Repository, author: Option<String>) -> GitResult<bool> {
    match repo.head() {
        Ok(_) => return Ok(false),
        Err(e) if e.code() == git2::ErrorCode::UnbornBranch => {},
        Err(e) => return Err(GitError::Read(e)),
    }
    let workdir = workdir(repo)?;
    let readme = workdir.join(INITIAL_FILE);
    if !readme.exists() {
        std::fs::write(&readme, "")
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
        "Initial commit",
        opts,
        Some(INITIAL_FILE),
    )?;
    Ok(true)
}

/// `HEAD`'s full hash, read straight after a successful `git commit`
/// subprocess so the caller can show/select the new commit.
fn head_hash(repo: &Repository) -> GitResult<String> {
    let head = repo.head().map_err(GitError::Read)?;
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
            let commit = repo.find_commit(oid).map_err(GitError::Read)?;
            Ok(commit.message().ok().map(str::to_owned))
        },
        Err(e) if e.code() == git2::ErrorCode::UnbornBranch => Ok(None),
        Err(e) => Err(GitError::Read(e)),
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
    let text = std::fs::read_to_string(path).ok()?;
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
    let statuses = repo.statuses(Some(&mut opts)).map_err(GitError::Read)?;
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
