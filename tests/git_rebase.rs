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
use ferrit::domain::git::error::GitError;
use ferrit::domain::git::model::Operation;
use ferrit::domain::git::operation::{OperationOutcome, Step};
use ferrit::domain::git::rebase::{RebaseEdit, build_todo};
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

fn step(dir: &TempDir, step: Step) -> Result<OperationOutcome, GitError> {
    Repo::open(dir.path())?.operation_step(step)
}

fn conflicted_merge(dir: &TempDir) {
    git(dir.path(), &["checkout", "-q", "-b", "side", "HEAD~2"]);
    fs::write(dir.path().join("f"), "side\n").unwrap();
    git(dir.path(), &["commit", "-qam", "side"]);
    git(dir.path(), &["checkout", "-q", "main"]);
    assert!(!try_git(dir.path(), &["merge", "side"], &[]));
}

fn resolve(dir: &TempDir, content: &str) {
    fs::write(dir.path().join("f"), format!("{content}\n")).unwrap();
    git(dir.path(), &["add", "f"]);
}

fn assert_failed_with(result: Result<OperationOutcome, GitError>, needle: &str) {
    match result {
        Err(GitError::OperationFailed(message)) => {
            assert!(message.contains(needle), "{message:?} lacks {needle:?}");
        },
        other => panic!("expected OperationFailed containing {needle:?}, got {other:?}"),
    }
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

// ---------------------------------------------------------------- rewriting

fn hash_of(dir: &TempDir, rev: &str) -> String {
    git(dir.path(), &["rev-parse", rev])
}

fn edit(dir: &TempDir, rev: &str, edit: &RebaseEdit) -> Result<OperationOutcome, GitError> {
    let hash = hash_of(dir, rev);
    Repo::open(dir.path())?.rebase_edit(&hash, edit)
}

fn subjects(dir: &TempDir) -> Vec<String> {
    git(dir.path(), &["log", "--format=%s"])
        .lines()
        .map(str::to_owned)
        .collect()
}

fn assert_rebase_failed(result: Result<OperationOutcome, GitError>, needle: &str) {
    match result {
        Err(GitError::RebaseFailed(message)) => {
            assert!(message.contains(needle), "{message:?} lacks {needle:?}");
        },
        other => panic!("expected RebaseFailed containing {needle:?}, got {other:?}"),
    }
}

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
    let dir = TempDir(outer.path().join("sp ace's"));
    fs::create_dir_all(&dir.0).unwrap();
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
