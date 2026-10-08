//! Edge cases (r6).

use crate::common::{TempDir, configure_identity, git};
use std::fs;
use std::path::Path;

use crate::support::{
    edit, hash_of, history, independent_history, poison_editors, pushed_history, step, subjects,
    try_git,
};
use ferrit::domain::git::Repo;
use ferrit::domain::git::model::Operation;
use ferrit::domain::git::operation::{OperationOutcome, Step};
use ferrit::domain::git::rebase::RebaseEdit;

#[test]
fn a_detached_head_can_be_rewritten() {
    let dir = history("edge-detached");
    git(dir.path(), &["checkout", "-q", "--detach"]);
    let outcome = edit(&dir, "HEAD~1", &RebaseEdit::Reword("detached".to_owned())).unwrap();

    assert_eq!(outcome, OperationOutcome::Done);
    assert_eq!(subjects(&dir), ["three", "detached", "one", "base"]);
    assert!(
        !try_git(dir.path(), &["symbolic-ref", "-q", "HEAD"], &[]),
        "still detached"
    );
}

#[test]
fn the_root_commit_can_be_dropped_when_nothing_depends_on_it() {
    let dir = independent_history("edge-root-drop");
    let outcome = edit(&dir, "HEAD~2", &RebaseEdit::Drop).unwrap();
    assert_eq!(outcome, OperationOutcome::Done);
    assert_eq!(subjects(&dir), ["t", "s"]);
    assert!(!dir.path().join("r.txt").exists());
}

#[test]
fn the_root_commit_can_be_edited() {
    let dir = independent_history("edge-root-edit");
    let outcome = edit(&dir, "HEAD~2", &RebaseEdit::Edit).unwrap();
    assert_eq!(outcome, OperationOutcome::Stopped { conflicted: false });
    assert_eq!(git(dir.path(), &["log", "-1", "--format=%s"]), "r");
    assert_eq!(step(&dir, Step::Continue).unwrap(), OperationOutcome::Done);
    assert_eq!(subjects(&dir), ["t", "s", "r"]);
}

#[test]
fn no_rewrite_ever_opens_the_users_editor() {
    let dir = history("edge-editor");
    let marker = poison_editors(&dir);

    edit(&dir, "HEAD~1", &RebaseEdit::Reword("no editor".to_owned())).unwrap();
    edit(&dir, "HEAD~1", &RebaseEdit::Squash).unwrap();
    edit(&dir, "HEAD~1", &RebaseEdit::Fixup).unwrap();
    edit(&dir, "HEAD~1", &RebaseEdit::Edit).unwrap();
    step(&dir, Step::Continue).unwrap();
    let target = hash_of(&dir, "HEAD~1");
    Repo::open(dir.path()).unwrap().autosquash(&target).unwrap();
    fs::write(dir.path().join("f"), "conflicting\n").unwrap();
    git(dir.path(), &["commit", "-qam", "extra"]);
    edit(&dir, "HEAD~1", &RebaseEdit::Drop).unwrap();

    assert!(!marker.exists(), "an editor was launched");
}

#[test]
fn rewriting_pushed_commits_leaves_the_branch_ahead_and_behind() {
    let (_origin, work) = pushed_history("edge-pushed");
    edit(&work, "HEAD~1", &RebaseEdit::Reword("rewritten".to_owned())).unwrap();

    let header = Repo::open(work.path()).unwrap().snapshot().unwrap().header;
    assert_eq!((header.ahead, header.behind), (2, 2), "{header:?}");
}

#[test]
fn a_pull_that_starts_a_rebase_and_conflicts_is_a_stopped_rebase() {
    let (origin, work) = pushed_history("edge-pull");
    // Another clone changes `f`'s last line upstream ...
    let other = TempDir::new("edge-pull-other");
    git(
        Path::new("."),
        &[
            "clone",
            "-q",
            origin.path().to_str().unwrap(),
            other.path().to_str().unwrap(),
        ],
    );
    configure_identity(other.path());
    fs::write(other.path().join("f"), "upstream\n").unwrap();
    git(other.path(), &["commit", "-qam", "upstream change"]);
    git(other.path(), &["push", "-q", "origin", "main"]);
    // ... while `work` changes the same line locally.
    fs::write(work.path().join("f"), "local\n").unwrap();
    git(work.path(), &["commit", "-qam", "local change"]);
    git(work.path(), &["config", "pull.rebase", "true"]);

    let repo = Repo::open(work.path()).unwrap();
    assert!(repo.pull().is_err(), "the conflict is a failed pull");
    assert!(
        matches!(repo.operation(), Some(Operation::Rebase { .. })),
        "{:?}",
        repo.operation()
    );

    assert_eq!(step(&work, Step::Abort).unwrap(), OperationOutcome::Done);
    assert_eq!(
        git(work.path(), &["log", "-1", "--format=%s"]),
        "local change"
    );
}
