//! Hand-built data and render helpers shared by the dashboard screen tests.

use crate::common::{TempDir, commit_all, configure_identity, git};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use ferrit::domain::git::Repo;
use ferrit::domain::git::error::GitError;
use ferrit::domain::git::model::Operation;
use ferrit::domain::git::operation::{OperationOutcome, Step};
use ferrit::domain::git::rebase::RebaseEdit;
use git2::Repository;

/// Run git, allowing failure (a conflict exits non-zero by design), with the
/// editors neutralised so nothing waits on a terminal.
pub(crate) fn try_git(dir: &Path, args: &[&str], envs: &[(&str, &str)]) -> bool {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(dir).args(args).env("GIT_EDITOR", "true");
    for (key, value) in envs {
        cmd.env(key, value);
    }
    cmd.output().unwrap().status.success()
}

/// `f` committed as base, one, two, three on `main`, one line each.
pub(crate) fn history(tag: &str) -> TempDir {
    let dir = TempDir::new(tag);
    let repo = Repository::init(dir.path()).unwrap();
    configure_identity(dir.path());
    git(dir.path(), &["checkout", "-q", "-b", "main"]);
    for content in ["base", "one", "two", "three"] {
        fs::write(dir.path().join("f"), format!("{content}\n")).unwrap();
        commit_all(&repo, content);
    }
    dir
}

/// Start `git rebase -i` over the last three commits with `todo` (three
/// lines, `%1` `%2` `%3` standing for oldest to newest).
pub(crate) fn interactive_rebase(dir: &Path, todo: &str) -> bool {
    let short = |rev: &str| git(dir, &["rev-parse", "--short", rev]);
    let text = todo
        .replace("%1", &short("HEAD~2"))
        .replace("%2", &short("HEAD~1"))
        .replace("%3", &short("HEAD"));
    let file = dir.join(".git").join("ferrit-test-todo");
    fs::write(&file, text).unwrap();
    let editor = format!("cp {}", file.display());
    try_git(
        dir,
        &["rebase", "-i", "HEAD~3"],
        &[("GIT_SEQUENCE_EDITOR", editor.as_str())],
    )
}

pub(crate) fn operation(dir: &TempDir) -> Option<Operation> {
    Repo::open(dir.path()).unwrap().operation()
}

/// `r`, `s`, `t` each add their own file, so dropping `r` (the root) leaves
/// nothing for `s` to conflict with.
pub(crate) fn independent_history(tag: &str) -> TempDir {
    let dir = TempDir::new(tag);
    let repo = Repository::init(dir.path()).unwrap();
    configure_identity(dir.path());
    git(dir.path(), &["checkout", "-q", "-b", "main"]);
    for name in ["r", "s", "t"] {
        fs::write(dir.path().join(format!("{name}.txt")), "x\n").unwrap();
        commit_all(&repo, name);
    }
    dir
}

/// Configured editors that would hang or record a call, if git ever ran them.
pub(crate) fn poison_editors(dir: &TempDir) -> PathBuf {
    let marker = dir.path().join(".git").join("editor-ran");
    let script = dir.path().join(".git").join("poison.sh");
    fs::write(
        &script,
        format!("#!/bin/sh\ntouch '{}'\nexit 1\n", marker.display()),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    }
    for key in ["core.editor", "sequence.editor"] {
        git(dir.path(), &["config", key, script.to_str().unwrap()]);
    }
    marker
}

/// A bare `origin` and a clone with `history`'s four commits pushed.
pub(crate) fn pushed_history(tag: &str) -> (TempDir, TempDir) {
    let origin = TempDir::new(&format!("{tag}-origin"));
    git(origin.path(), &["init", "-q", "--bare", "-b", "main"]);
    let work = TempDir::new(&format!("{tag}-work"));
    git(
        Path::new("."),
        &[
            "clone",
            "-q",
            origin.path().to_str().unwrap(),
            work.path().to_str().unwrap(),
        ],
    );
    configure_identity(work.path());
    git(work.path(), &["checkout", "-q", "-b", "main"]);
    let repo = Repository::open(work.path()).unwrap();
    for content in ["base", "one", "two", "three"] {
        fs::write(work.path().join("f"), format!("{content}\n")).unwrap();
        commit_all(&repo, content);
    }
    git(work.path(), &["push", "-q", "-u", "origin", "main"]);
    (origin, work)
}

pub(crate) fn step(dir: &TempDir, step: Step) -> Result<OperationOutcome, GitError> {
    Repo::open(dir.path())?.operation_step(step)
}

pub(crate) fn conflicted_merge(dir: &TempDir) {
    git(dir.path(), &["checkout", "-q", "-b", "side", "HEAD~2"]);
    fs::write(dir.path().join("f"), "side\n").unwrap();
    git(dir.path(), &["commit", "-qam", "side"]);
    git(dir.path(), &["checkout", "-q", "main"]);
    assert!(!try_git(dir.path(), &["merge", "side"], &[]));
}

pub(crate) fn resolve(dir: &TempDir, content: &str) {
    fs::write(dir.path().join("f"), format!("{content}\n")).unwrap();
    git(dir.path(), &["add", "f"]);
}

pub(crate) fn assert_failed_with(result: Result<OperationOutcome, GitError>, needle: &str) {
    match result {
        Err(GitError::OperationFailed(message)) => {
            assert!(message.contains(needle), "{message:?} lacks {needle:?}");
        },
        other => panic!("expected OperationFailed containing {needle:?}, got {other:?}"),
    }
}

pub(crate) fn hash_of(dir: &TempDir, rev: &str) -> String {
    git(dir.path(), &["rev-parse", rev])
}

pub(crate) fn edit(
    dir: &TempDir,
    rev: &str,
    edit: &RebaseEdit,
) -> Result<OperationOutcome, GitError> {
    let hash = hash_of(dir, rev);
    Repo::open(dir.path())?.rebase_edit(&hash, edit)
}

pub(crate) fn subjects(dir: &TempDir) -> Vec<String> {
    git(dir.path(), &["log", "--format=%s"])
        .lines()
        .map(str::to_owned)
        .collect()
}

pub(crate) fn assert_rebase_failed(result: Result<OperationOutcome, GitError>, needle: &str) {
    match result {
        Err(GitError::RebaseFailed(message)) => {
            assert!(message.contains(needle), "{message:?} lacks {needle:?}");
        },
        other => panic!("expected RebaseFailed containing {needle:?}, got {other:?}"),
    }
}
