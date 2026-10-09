//! Rewriting.

use crate::common::{TempDir, commit_all, configure_identity, git};
use std::fs;
use std::path::Path;

use crate::support::{assert_rebase_failed, edit, hash_of, history, operation, step, subjects};
use ferrit::git::error::GitError;
use ferrit::git::model::Operation;
use ferrit::git::rebase::{OperationOutcome, Step};
use ferrit::git::rebase::{RebaseEdit, build_todo};
use ferrit::git::repo::Repo;
use git2::Repository;

#[test]
fn build_todo_picks_everything_and_marks_the_target() {
    let commits: Vec<String> = ["a", "b", "c"].map(str::to_owned).to_vec();
    let file = Path::new("/tmp/msg");
    let todo = |edit: RebaseEdit| build_todo(&commits, "b", &edit, file);

    assert_eq!(todo(RebaseEdit::Drop), "pick a\ndrop b\npick c\n");
    assert_eq!(todo(RebaseEdit::Edit), "pick a\nedit b\npick c\n");
    assert_eq!(todo(RebaseEdit::Squash), "pick a\nsquash b\npick c\n");
    assert_eq!(todo(RebaseEdit::Fixup), "pick a\nfixup b\npick c\n");
    assert_eq!(
        todo(RebaseEdit::Reword("x".to_owned())),
        "pick a\npick b\nexec git commit --amend -q --allow-empty -F '/tmp/msg'\npick c\n"
    );
}

#[test]
fn build_todo_quotes_a_path_with_spaces_and_quotes() {
    let commits = vec!["a".to_owned()];
    let todo = build_todo(
        &commits,
        "a",
        &RebaseEdit::Reword("x".to_owned()),
        Path::new("/tmp/it's here/message"),
    );
    assert!(todo.contains("-F '/tmp/it'\\''s here/message'"), "{todo}");
}

#[test]
fn rewording_an_older_commit_replaces_subject_and_body_and_keeps_the_rest() {
    let dir = history("rw-reword");
    let before_author = git(dir.path(), &["log", "-1", "--format=%an <%ae>", "HEAD~1"]);
    let outcome = edit(
        &dir,
        "HEAD~1",
        &RebaseEdit::Reword("new subject\n\nnew body".to_owned()),
    )
    .unwrap();

    assert_eq!(outcome, OperationOutcome::Done);
    assert_eq!(subjects(&dir), ["three", "new subject", "one", "base"]);
    assert_eq!(
        git(dir.path(), &["log", "-1", "--format=%b", "HEAD~1"]),
        "new body"
    );
    assert_eq!(
        git(dir.path(), &["log", "-1", "--format=%an <%ae>", "HEAD~1"]),
        before_author
    );
    assert_eq!(
        git(dir.path(), &["status", "--porcelain"]),
        "",
        "worktree clean"
    );
    assert_eq!(fs::read_to_string(dir.path().join("f")).unwrap(), "three\n");
    assert!(
        !dir.path().join(".git/ferrit").exists(),
        "scratch files removed"
    );
    assert_eq!(operation(&dir), None);
}

#[test]
fn the_root_commit_can_be_reworded() {
    let dir = history("rw-root");
    edit(&dir, "HEAD~3", &RebaseEdit::Reword("first".to_owned())).unwrap();
    assert_eq!(subjects(&dir), ["three", "two", "one", "first"]);
}

#[test]
fn a_repository_path_with_a_space_and_a_quote_works() {
    let outer = TempDir::new("rw-quote");
    let dir = outer.child("sp ace's");
    let repo = Repository::init(dir.path()).unwrap();
    configure_identity(dir.path());
    for content in ["base", "one", "two"] {
        fs::write(dir.path().join("f"), format!("{content}\n")).unwrap();
        commit_all(&repo, content);
    }
    let hash = git(dir.path(), &["rev-parse", "HEAD~1"]);
    let outcome = Repo::open(dir.path())
        .unwrap()
        .rebase_edit(&hash, &RebaseEdit::Reword("quoted".to_owned()))
        .unwrap();
    assert_eq!(outcome, OperationOutcome::Done);
    assert_eq!(subjects(&dir), ["two", "quoted", "base"]);
}

#[test]
fn dropping_removes_the_commit_and_stops_on_the_conflict_it_causes() {
    let dir = history("rw-drop");
    // `two` was written on top of `one`, so it conflicts without it.
    let outcome = edit(&dir, "HEAD~2", &RebaseEdit::Drop).unwrap();
    assert_eq!(outcome, OperationOutcome::Stopped { conflicted: true });
    assert_eq!(
        operation(&dir),
        Some(Operation::Rebase { step: 2, total: 3 })
    );

    git(dir.path(), &["rebase", "--abort"]);
    assert_eq!(subjects(&dir), ["three", "two", "one", "base"]);
}

#[test]
fn dropping_a_commit_nothing_depends_on_finishes() {
    let dir = history("rw-drop-clean");
    fs::write(dir.path().join("g"), "extra\n").unwrap();
    git(dir.path(), &["add", "g"]);
    git(dir.path(), &["commit", "-qm", "extra"]);
    let outcome = edit(&dir, "HEAD", &RebaseEdit::Drop).unwrap();
    assert_eq!(outcome, OperationOutcome::Done);
    assert_eq!(subjects(&dir), ["three", "two", "one", "base"]);
    assert!(!dir.path().join("g").exists());
}

#[test]
fn squash_folds_into_the_commit_below_keeping_both_messages() {
    let dir = history("rw-squash");
    let outcome = edit(&dir, "HEAD~1", &RebaseEdit::Squash).unwrap();
    assert_eq!(outcome, OperationOutcome::Done);

    assert_eq!(subjects(&dir), ["three", "one", "base"]);
    let message = git(dir.path(), &["log", "-1", "--format=%B", "HEAD~1"]);
    assert!(
        message.contains("one") && message.contains("two"),
        "{message}"
    );
    assert_eq!(fs::read_to_string(dir.path().join("f")).unwrap(), "three\n");
}

#[test]
fn fixup_folds_into_the_commit_below_and_drops_its_message() {
    let dir = history("rw-fixup");
    edit(&dir, "HEAD~1", &RebaseEdit::Fixup).unwrap();

    assert_eq!(subjects(&dir), ["three", "one", "base"]);
    let message = git(dir.path(), &["log", "-1", "--format=%B", "HEAD~1"]);
    assert!(
        message.contains("one") && !message.contains("two"),
        "{message}"
    );
}

#[test]
fn squashing_into_the_root_commit_uses_root() {
    let dir = history("rw-squash-root");
    edit(&dir, "HEAD~2", &RebaseEdit::Fixup).unwrap();
    assert_eq!(subjects(&dir), ["three", "two", "base"]);
}

#[test]
fn nothing_is_below_the_root_commit() {
    let dir = history("rw-squash-none");
    assert_rebase_failed(edit(&dir, "HEAD~3", &RebaseEdit::Squash), "no commit below");
    assert_eq!(operation(&dir), None);
}

#[test]
fn edit_stops_at_the_commit_without_a_conflict_and_continues_to_the_end() {
    let dir = history("rw-edit");
    let outcome = edit(&dir, "HEAD~1", &RebaseEdit::Edit).unwrap();
    assert_eq!(outcome, OperationOutcome::Stopped { conflicted: false });
    assert_eq!(
        operation(&dir),
        Some(Operation::Rebase { step: 1, total: 2 })
    );
    assert_eq!(
        git(dir.path(), &["log", "-1", "--format=%s"]),
        "two",
        "stopped at the edited commit"
    );

    assert_eq!(step(&dir, Step::Continue).unwrap(), OperationOutcome::Done);
    assert_eq!(subjects(&dir), ["three", "two", "one", "base"]);
}

#[test]
fn a_rejecting_hook_leaves_the_rebase_stopped_with_its_message_and_keeps_the_file() {
    let dir = history("rw-hook");
    let hook = dir.path().join(".git/hooks/commit-msg");
    fs::create_dir_all(hook.parent().unwrap()).unwrap();
    fs::write(&hook, "#!/bin/sh\necho 'no thanks' >&2\nexit 1\n").unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&hook, fs::Permissions::from_mode(0o755)).unwrap();
    }

    let result = edit(&dir, "HEAD~1", &RebaseEdit::Reword("rejected".to_owned()));
    assert!(
        matches!(result, Err(GitError::RebaseFailed(_))),
        "{result:?}"
    );
    assert!(
        matches!(operation(&dir), Some(Operation::Rebase { .. })),
        "stopped, not vanished"
    );
    assert!(
        dir.path().join(".git/ferrit/message").exists(),
        "kept for a continue"
    );

    assert_eq!(step(&dir, Step::Abort).unwrap(), OperationOutcome::Done);
    assert_eq!(subjects(&dir), ["three", "two", "one", "base"]);
}

#[test]
fn a_merge_commit_in_the_range_is_refused_before_anything_changes() {
    let dir = history("rw-merge");
    git(dir.path(), &["checkout", "-q", "-b", "side", "HEAD~3"]);
    fs::write(dir.path().join("s"), "side\n").unwrap();
    git(dir.path(), &["add", "s"]);
    git(dir.path(), &["commit", "-qm", "side"]);
    git(dir.path(), &["checkout", "-q", "main"]);
    git(
        dir.path(),
        &["merge", "--no-ff", "-qm", "merge side", "side"],
    );
    let head = hash_of(&dir, "HEAD");

    assert_rebase_failed(edit(&dir, "HEAD~2", &RebaseEdit::Drop), "merge commit");
    assert_eq!(hash_of(&dir, "HEAD"), head);
    assert_eq!(operation(&dir), None);
}

#[test]
fn a_dirty_worktree_is_refused_with_gits_message_and_no_rebase_state() {
    let dir = history("rw-dirty");
    fs::write(dir.path().join("f"), "local edit\n").unwrap();
    assert_rebase_failed(edit(&dir, "HEAD~1", &RebaseEdit::Fixup), "unstaged changes");
    assert_eq!(operation(&dir), None);
    assert_eq!(
        fs::read_to_string(dir.path().join("f")).unwrap(),
        "local edit\n"
    );
}

#[test]
fn a_second_rewrite_is_refused_while_one_is_stopped() {
    let dir = history("rw-busy");
    edit(&dir, "HEAD~2", &RebaseEdit::Drop).unwrap();
    assert_rebase_failed(
        edit(&dir, "HEAD", &RebaseEdit::Fixup),
        "already in progress",
    );
}

#[test]
fn a_commit_off_the_current_branch_or_unknown_is_refused() {
    let dir = history("rw-other");
    git(dir.path(), &["checkout", "-q", "-b", "side", "HEAD~2"]);
    fs::write(dir.path().join("s"), "side\n").unwrap();
    git(dir.path(), &["add", "s"]);
    git(dir.path(), &["commit", "-qm", "side"]);
    let side = hash_of(&dir, "HEAD");
    git(dir.path(), &["checkout", "-q", "main"]);

    let repo = Repo::open(dir.path()).unwrap();
    assert_rebase_failed(
        repo.rebase_edit(&side, &RebaseEdit::Drop),
        "not on the current branch",
    );
    assert!(matches!(
        repo.rebase_edit(&"0".repeat(40), &RebaseEdit::Drop),
        Err(GitError::NoSuchCommit(_))
    ));
}

#[test]
fn autosquash_folds_a_fixup_commit_into_its_target() {
    let dir = history("rw-autosquash");
    let target = hash_of(&dir, "HEAD~2"); // `one`
    fs::write(dir.path().join("g"), "fixed\n").unwrap();
    git(dir.path(), &["add", "g"]);
    git(dir.path(), &["commit", "-q", &format!("--fixup={target}")]);
    assert_eq!(subjects(&dir).len(), 5);

    let outcome = Repo::open(dir.path()).unwrap().autosquash(&target).unwrap();
    assert_eq!(outcome, OperationOutcome::Done);
    assert_eq!(subjects(&dir), ["three", "two", "one", "base"]);
    assert!(dir.path().join("g").exists(), "the fix lives in `one` now");
    assert!(git(dir.path(), &["show", "--stat", "--format=", "HEAD~2"]).contains('g'));
}

#[test]
fn autosquash_with_nothing_to_fold_changes_nothing() {
    let dir = history("rw-autosquash-none");
    let head = hash_of(&dir, "HEAD");
    let target = hash_of(&dir, "HEAD~2");
    let outcome = Repo::open(dir.path()).unwrap().autosquash(&target).unwrap();
    assert_eq!(outcome, OperationOutcome::Done);
    assert_eq!(subjects(&dir).len(), 4);
    assert_eq!(hash_of(&dir, "HEAD"), head, "nothing was rewritten");
}

#[test]
fn commit_message_returns_subject_and_body() {
    let dir = history("rw-message");
    git(
        dir.path(),
        &[
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "subject\n\nbody line",
        ],
    );
    let repo = Repo::open(dir.path()).unwrap();
    assert_eq!(
        repo.commit_message(&hash_of(&dir, "HEAD")).unwrap(),
        "subject\n\nbody line"
    );
    assert!(matches!(
        repo.commit_message("nope"),
        Err(GitError::NoSuchCommit(_))
    ));
}
