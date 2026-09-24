#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::pathbuf_init_then_push,
    clippy::iter_on_single_items,
    clippy::format_collect,
    elided_lifetimes_in_paths,
    reason = "integration test scaffolding: a failed setup is the assertion, helper ergonomics beat lint-cleanliness here"
)]
//! `Repo::operation` and `Snapshot::operation` (`docs/PLAN_11_REBASE.md` R1):
//! which merge, rebase, cherry-pick or revert git is stopped in, read from the
//! repository state, plus the progress of a rebase.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use ferrit::domain::git::Repo;
use ferrit::domain::git::model::Operation;
use git2::{IndexAddOption, Repository, Signature};

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let mut path = std::env::temp_dir();
        path.push(format!("ferrit-{tag}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn configure_identity(dir: &Path) {
    for (key, value) in [("user.name", "Test"), ("user.email", "test@example.com")] {
        let out = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["config", key, value])
            .output()
            .unwrap();
        assert!(out.status.success());
    }
}

fn commit_all(repo: &Repository, message: &str) {
    let mut index = repo.index().unwrap();
    index
        .add_all(["*"].iter(), IndexAddOption::DEFAULT, None)
        .unwrap();
    index.write().unwrap();
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
    let sig = Signature::now("Test", "test@example.com").unwrap();
    let parent = repo
        .head()
        .ok()
        .and_then(|h| h.target())
        .and_then(|oid| repo.find_commit(oid).ok());
    let parents: Vec<&git2::Commit> = parent.iter().collect();
    repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)
        .unwrap();
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

/// Run git, allowing failure (a conflict exits non-zero by design), with the
/// editors neutralised so nothing waits on a terminal.
fn try_git(dir: &Path, args: &[&str], envs: &[(&str, &str)]) -> bool {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(dir).args(args).env("GIT_EDITOR", "true");
    for (key, value) in envs {
        cmd.env(key, value);
    }
    cmd.output().unwrap().status.success()
}

/// `f` committed as base, one, two, three on `main`, one line each.
fn history(tag: &str) -> TempDir {
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
fn interactive_rebase(dir: &Path, todo: &str) -> bool {
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

fn operation(dir: &TempDir) -> Option<Operation> {
    Repo::open(dir.path()).unwrap().operation()
}

#[test]
fn a_clean_repository_has_no_operation() {
    let dir = history("op-clean");
    assert_eq!(operation(&dir), None);
    assert_eq!(
        Repo::open(dir.path())
            .unwrap()
            .snapshot()
            .unwrap()
            .operation,
        None
    );
}

#[test]
fn a_conflicted_merge_is_a_merge_until_aborted() {
    let dir = history("op-merge");
    git(dir.path(), &["checkout", "-q", "-b", "side", "HEAD~2"]);
    fs::write(dir.path().join("f"), "side\n").unwrap();
    git(dir.path(), &["commit", "-qam", "side"]);
    git(dir.path(), &["checkout", "-q", "main"]);
    assert!(!try_git(dir.path(), &["merge", "side"], &[]));

    assert_eq!(operation(&dir), Some(Operation::Merge));
    let snapshot = Repo::open(dir.path()).unwrap().snapshot().unwrap();
    assert_eq!(snapshot.operation, Some(Operation::Merge));

    git(dir.path(), &["merge", "--abort"]);
    assert_eq!(operation(&dir), None);
}

#[test]
fn a_conflicting_rebase_reports_its_step_and_total() {
    // Dropping `one` makes `two` conflict: step 2 of 3 (a drop is a step).
    let dir = history("op-rebase");
    assert!(!interactive_rebase(
        dir.path(),
        "drop %1\npick %2\npick %3\n"
    ));

    assert_eq!(
        operation(&dir),
        Some(Operation::Rebase { step: 2, total: 3 })
    );
    assert_eq!(operation(&dir).unwrap().label(), "REBASING 2/3");

    git(dir.path(), &["rebase", "--abort"]);
    assert_eq!(operation(&dir), None);
}

#[test]
fn an_edit_stop_is_a_rebase_with_no_conflict() {
    let dir = history("op-edit");
    assert!(interactive_rebase(
        dir.path(),
        "pick %1\nedit %2\npick %3\n"
    ));

    assert_eq!(
        operation(&dir),
        Some(Operation::Rebase { step: 2, total: 3 })
    );
    assert_eq!(
        git(dir.path(), &["status", "--porcelain"]),
        "",
        "nothing conflicted"
    );

    assert!(try_git(dir.path(), &["rebase", "--continue"], &[]));
    assert_eq!(operation(&dir), None);
}

#[test]
fn the_apply_backend_reads_next_and_last() {
    let dir = history("op-apply");
    git(dir.path(), &["checkout", "-q", "-b", "topic", "HEAD~3"]);
    fs::write(dir.path().join("f"), "topic\n").unwrap();
    git(dir.path(), &["commit", "-qam", "topic"]);
    assert!(!try_git(dir.path(), &["rebase", "--apply", "main"], &[]));

    assert_eq!(
        operation(&dir),
        Some(Operation::Rebase { step: 1, total: 1 })
    );
    git(dir.path(), &["rebase", "--abort"]);
    assert_eq!(operation(&dir), None);
}

#[test]
fn missing_progress_files_leave_a_plain_rebase_badge() {
    let dir = history("op-noprogress");
    assert!(!interactive_rebase(
        dir.path(),
        "drop %1\npick %2\npick %3\n"
    ));
    for name in ["msgnum", "end"] {
        fs::remove_file(dir.path().join(".git/rebase-merge").join(name)).unwrap();
    }

    let op = operation(&dir).expect("still a rebase");
    assert_eq!(op, Operation::Rebase { step: 0, total: 0 });
    assert_eq!(op.label(), "REBASING");
}

#[test]
fn a_conflicting_cherry_pick_and_revert_are_named() {
    let dir = history("op-pick");
    git(dir.path(), &["checkout", "-q", "-b", "other", "HEAD~3"]);
    fs::write(dir.path().join("f"), "other\n").unwrap();
    git(dir.path(), &["commit", "-qam", "other"]);
    assert!(!try_git(dir.path(), &["cherry-pick", "main"], &[]));
    assert_eq!(operation(&dir), Some(Operation::CherryPick));
    assert_eq!(operation(&dir).unwrap().label(), "CHERRY-PICKING");
    git(dir.path(), &["cherry-pick", "--abort"]);
    assert_eq!(operation(&dir), None);

    // Reverting `two` on top of `three` conflicts: they touch the same line.
    git(dir.path(), &["checkout", "-q", "main"]);
    assert!(!try_git(
        dir.path(),
        &["revert", "--no-edit", "HEAD~1"],
        &[]
    ));
    assert_eq!(operation(&dir), Some(Operation::Revert));
    assert_eq!(operation(&dir).unwrap().label(), "REVERTING");
    git(dir.path(), &["revert", "--abort"]);
    assert_eq!(operation(&dir), None);
}

#[test]
fn bisect_and_a_stopped_mailbox_apply_are_not_operations() {
    let dir = history("op-other");
    git(dir.path(), &["bisect", "start"]);
    assert_eq!(operation(&dir), None, "bisect has no ferrit flow");
    git(dir.path(), &["bisect", "reset"]);

    // A patch that no longer applies leaves `git am` waiting.
    let patch = git(dir.path(), &["format-patch", "-1", "HEAD~1", "--stdout"]);
    let file = dir.path().join(".git").join("ferrit-test.patch");
    fs::write(&file, format!("{patch}\n")).unwrap();
    assert!(!try_git(dir.path(), &["am", file.to_str().unwrap()], &[]));
    assert_eq!(
        operation(&dir),
        None,
        "am is ambiguous with a rebase, so it stays unnamed"
    );
    git(dir.path(), &["am", "--abort"]);
}

#[test]
fn labels_are_stable() {
    assert_eq!(Operation::Merge.label(), "MERGING");
    assert_eq!(
        Operation::Rebase { step: 1, total: 4 }.label(),
        "REBASING 1/4"
    );
    assert_eq!(Operation::Rebase { step: 0, total: 0 }.label(), "REBASING");
}
