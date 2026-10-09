//! Operations.

use crate::common::git;
use std::fs;

use crate::support::{
    assert_failed_with, conflicted_merge, history, interactive_rebase, operation, resolve, step,
    try_git,
};
use ferrit::git::model::Operation;
use ferrit::git::rebase::{OperationOutcome, Step};
use ferrit::git::repo::Repo;

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

#[test]
fn no_operation_means_no_step() {
    let dir = history("step-none");
    assert_failed_with(step(&dir, Step::Abort), "no operation in progress");
}

#[test]
fn a_merge_continues_once_resolved_and_refuses_before() {
    let dir = history("step-merge");
    conflicted_merge(&dir);

    assert_failed_with(step(&dir, Step::Continue), "unmerged files");
    assert_eq!(operation(&dir), Some(Operation::Merge), "nothing changed");

    resolve(&dir, "merged");
    assert_eq!(step(&dir, Step::Continue).unwrap(), OperationOutcome::Done);
    assert_eq!(operation(&dir), None);
    assert_eq!(
        git(dir.path(), &["rev-list", "--parents", "-n1", "HEAD"])
            .split(' ')
            .count(),
        3,
        "a merge commit: hash plus two parents"
    );
}

#[test]
fn a_merge_aborts_and_cannot_be_skipped() {
    let dir = history("step-merge-abort");
    conflicted_merge(&dir);
    let before = git(dir.path(), &["rev-parse", "HEAD"]);

    assert_failed_with(step(&dir, Step::Skip), "cannot be skipped");
    assert_eq!(operation(&dir), Some(Operation::Merge));

    assert_eq!(step(&dir, Step::Abort).unwrap(), OperationOutcome::Done);
    assert_eq!(operation(&dir), None);
    assert_eq!(git(dir.path(), &["rev-parse", "HEAD"]), before);
}

#[test]
fn a_rebase_refuses_to_continue_over_an_unresolved_file() {
    let dir = history("step-rebase-refuse");
    assert!(!interactive_rebase(
        dir.path(),
        "drop %1\npick %2\npick %3\n"
    ));

    assert_failed_with(step(&dir, Step::Continue), "needs merge");
    assert_eq!(
        operation(&dir),
        Some(Operation::Rebase { step: 2, total: 3 }),
        "state unchanged"
    );
}

#[test]
fn a_resolved_rebase_continues_to_the_end() {
    let dir = history("step-rebase-done");
    assert!(!interactive_rebase(
        dir.path(),
        "drop %1\npick %2\npick %3\n"
    ));
    resolve(&dir, "two");

    assert_eq!(step(&dir, Step::Continue).unwrap(), OperationOutcome::Done);
    assert_eq!(operation(&dir), None);
    assert_eq!(
        git(dir.path(), &["log", "--format=%s"])
            .lines()
            .collect::<Vec<_>>(),
        ["three", "two", "base"],
        "`one` was dropped"
    );
}

#[test]
fn skipping_can_land_on_the_next_conflict_and_abort_restores_everything() {
    let dir = history("step-rebase-skip");
    let before = git(dir.path(), &["rev-parse", "HEAD"]);
    assert!(!interactive_rebase(
        dir.path(),
        "drop %1\npick %2\npick %3\n"
    ));

    // Skipping `two` leaves `three` to apply on `base`: it conflicts too.
    assert_eq!(
        step(&dir, Step::Skip).unwrap(),
        OperationOutcome::Stopped { conflicted: true }
    );
    assert_eq!(
        operation(&dir),
        Some(Operation::Rebase { step: 3, total: 3 })
    );

    assert_eq!(step(&dir, Step::Abort).unwrap(), OperationOutcome::Done);
    assert_eq!(operation(&dir), None);
    assert_eq!(git(dir.path(), &["rev-parse", "HEAD"]), before);
}

#[test]
fn a_continue_that_hits_the_next_conflict_is_stopped_not_an_error() {
    let dir = history("step-rebase-next");
    assert!(!interactive_rebase(
        dir.path(),
        "drop %1\npick %2\npick %3\n"
    ));
    resolve(&dir, "resolved differently"); // `three` will not apply on this

    assert_eq!(
        step(&dir, Step::Continue).unwrap(),
        OperationOutcome::Stopped { conflicted: true }
    );
    assert_eq!(
        operation(&dir),
        Some(Operation::Rebase { step: 3, total: 3 })
    );
}

#[test]
fn an_edit_stop_continues_to_the_end() {
    let dir = history("step-edit");
    assert!(interactive_rebase(
        dir.path(),
        "pick %1\nedit %2\npick %3\n"
    ));
    assert_eq!(step(&dir, Step::Continue).unwrap(), OperationOutcome::Done);
    assert_eq!(operation(&dir), None);
}

#[test]
fn cherry_pick_and_revert_take_all_three_steps() {
    let dir = history("step-pick");
    git(dir.path(), &["checkout", "-q", "-b", "other", "HEAD~3"]);
    fs::write(dir.path().join("f"), "other\n").unwrap();
    git(dir.path(), &["commit", "-qam", "other"]);

    for (name, action, expect_commit) in [
        ("abort", Step::Abort, false),
        ("skip", Step::Skip, false),
        ("continue", Step::Continue, true),
    ] {
        let before = git(dir.path(), &["rev-list", "--count", "HEAD"]);
        assert!(
            !try_git(dir.path(), &["cherry-pick", "main"], &[]),
            "{name}"
        );
        assert_eq!(operation(&dir), Some(Operation::CherryPick));
        if action == Step::Continue {
            resolve(&dir, "picked");
        }
        assert_eq!(
            step(&dir, action).unwrap(),
            OperationOutcome::Done,
            "{name}"
        );
        assert_eq!(operation(&dir), None, "{name}");
        let grew = git(dir.path(), &["rev-list", "--count", "HEAD"]) != before;
        assert_eq!(grew, expect_commit, "{name}");
        git(dir.path(), &["reset", "-q", "--hard", "HEAD"]);
    }

    git(dir.path(), &["checkout", "-q", "main"]);
    for (name, action) in [("abort", Step::Abort), ("skip", Step::Skip)] {
        assert!(
            !try_git(dir.path(), &["revert", "--no-edit", "HEAD~1"], &[]),
            "{name}"
        );
        assert_eq!(operation(&dir), Some(Operation::Revert));
        assert_eq!(
            step(&dir, action).unwrap(),
            OperationOutcome::Done,
            "{name}"
        );
        assert_eq!(operation(&dir), None, "{name}");
    }
}
