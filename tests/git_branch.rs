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
//! Subprocess coverage for `Repo::checkout` / `create_branch` /
//! `delete_branch` / `fast_forward` / `merge_branch`: build throwaway
//! fixture repos with `git2`, then drive the backend and check real `git`
//! state. See `docs/PLAN_8_BRANCHES.md` milestones S0/S1.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use ferrit::git::Repo;
use ferrit::git::branch::MergeOutcome;
use ferrit::git::error::GitError;
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

/// A repo with `base` at one commit and `feat` branched off it at a
/// second, `base` checked out. `base` rather than relying on whatever
/// name `git2::Repository::init` happens to default to (`init
/// .defaultBranch`-dependent, `main` here but not guaranteed elsewhere):
/// renamed right after the first commit, so every other test in this file
/// can name it without caring what the ambient git config picked. The
/// common starting point for most of this file's tests.
fn two_branch_fixture(tag: &str) -> TempDir {
    let dir = TempDir::new(tag);
    let repo = Repository::init(dir.path()).unwrap();
    configure_identity(dir.path());
    fs::write(dir.path().join("a.txt"), "one\n").unwrap();
    commit_all(&repo, "init");
    git(dir.path(), &["branch", "-m", "base"]);
    git(dir.path(), &["checkout", "-q", "-b", "feat"]);
    fs::write(dir.path().join("a.txt"), "one\nfeat-change\n").unwrap();
    fs::write(dir.path().join("b.txt"), "two\n").unwrap();
    commit_all(&repo, "feat work");
    git(dir.path(), &["checkout", "-q", "base"]);
    dir
}

#[test]
fn checkout_switches_head() {
    let dir = two_branch_fixture("checkout-basic");
    let backend = Repo::open(dir.path()).unwrap();
    backend.checkout("feat").unwrap();
    assert_eq!(
        git(dir.path(), &["symbolic-ref", "--short", "HEAD"]),
        "feat"
    );
}

#[test]
fn checkout_with_a_conflicting_dirty_worktree_fails_and_leaves_head_untouched() {
    let dir = two_branch_fixture("checkout-dirty");
    fs::write(dir.path().join("a.txt"), "one\nlocal-dirty\n").unwrap();

    let backend = Repo::open(dir.path()).unwrap();
    let err = backend.checkout("feat").unwrap_err();
    assert!(matches!(err, GitError::CheckoutFailed(_)), "got {err:?}");
    assert_eq!(
        git(dir.path(), &["symbolic-ref", "--short", "HEAD"]),
        "base"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "one\nlocal-dirty\n",
        "worktree untouched"
    );
}

#[test]
fn create_branch_checks_out_from_head_with_the_same_tip() {
    let dir = two_branch_fixture("create-branch");
    let parent_tip = git(dir.path(), &["rev-parse", "HEAD"]);

    let backend = Repo::open(dir.path()).unwrap();
    backend.create_branch("new-work").unwrap();

    assert_eq!(
        git(dir.path(), &["symbolic-ref", "--short", "HEAD"]),
        "new-work"
    );
    assert_eq!(git(dir.path(), &["rev-parse", "HEAD"]), parent_tip);
}

#[test]
fn create_branch_with_a_name_already_taken_is_an_error_and_leaves_head_alone() {
    let dir = two_branch_fixture("create-branch-taken");
    let head_before = git(dir.path(), &["symbolic-ref", "--short", "HEAD"]);

    let backend = Repo::open(dir.path()).unwrap();
    let err = backend.create_branch("feat").unwrap_err();
    assert!(matches!(err, GitError::BranchFailed(_)), "got {err:?}");
    assert_eq!(
        git(dir.path(), &["symbolic-ref", "--short", "HEAD"]),
        head_before
    );
}

#[test]
fn delete_branch_merged_succeeds() {
    let dir = TempDir::new("delete-merged");
    let repo = Repository::init(dir.path()).unwrap();
    configure_identity(dir.path());
    fs::write(dir.path().join("a.txt"), "one\n").unwrap();
    commit_all(&repo, "init");
    git(dir.path(), &["branch", "merged-branch"]);

    let backend = Repo::open(dir.path()).unwrap();
    backend.delete_branch("merged-branch", false).unwrap();
    assert!(!git(dir.path(), &["branch", "--list"]).contains("merged-branch"));
}

#[test]
fn delete_branch_unmerged_needs_force() {
    let dir = two_branch_fixture("delete-unmerged");

    let backend = Repo::open(dir.path()).unwrap();
    let err = backend.delete_branch("feat", false).unwrap_err();
    let GitError::BranchFailed(msg) = err else {
        panic!("expected BranchFailed, got {err:?}");
    };
    assert!(msg.contains("not fully merged"), "got: {msg}");
    assert!(git(dir.path(), &["branch", "--list"]).contains("feat"));

    backend.delete_branch("feat", true).unwrap();
    assert!(!git(dir.path(), &["branch", "--list"]).contains("feat"));
}

#[test]
fn delete_branch_checked_out_is_an_error_and_leaves_it_in_place() {
    let dir = two_branch_fixture("delete-checked-out");
    let backend = Repo::open(dir.path()).unwrap();
    let err = backend.delete_branch("base", false).unwrap_err();
    assert!(matches!(err, GitError::BranchFailed(_)), "got {err:?}");
    assert!(git(dir.path(), &["branch", "--list"]).contains("base"));
}

/// `origin`, cloned into `work`; `feat` in `work` tracks `origin/base`.
/// `base` for the same "don't trust the ambient `init.defaultBranch`"
/// reason as `two_branch_fixture`. Returns `(origin, work)`.
fn clone_fixture(tag: &str) -> (TempDir, TempDir) {
    let origin = TempDir::new(&format!("{tag}-origin"));
    let repo = Repository::init(origin.path()).unwrap();
    configure_identity(origin.path());
    fs::write(origin.path().join("a.txt"), "one\n").unwrap();
    commit_all(&repo, "init");
    git(origin.path(), &["branch", "-m", "base"]);

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
    git(work.path(), &["checkout", "-q", "-b", "feat"]);
    git(
        work.path(),
        &["branch", "--set-upstream-to=origin/base", "feat"],
    );
    (origin, work)
}

#[test]
fn fast_forward_not_checked_out_moves_only_that_branch() {
    let (origin, work) = clone_fixture("ff-other");
    fs::write(origin.path().join("a.txt"), "one\ntwo\n").unwrap();
    let origin_repo = Repository::open(origin.path()).unwrap();
    commit_all(&origin_repo, "origin advances");
    git(work.path(), &["fetch", "-q", "origin"]);

    let base_before = git(work.path(), &["rev-parse", "base"]);
    let upstream_tip = git(work.path(), &["rev-parse", "origin/base"]);
    assert_ne!(git(work.path(), &["rev-parse", "feat"]), upstream_tip);

    let backend = Repo::open(work.path()).unwrap();
    backend.fast_forward("feat").unwrap();

    assert_eq!(git(work.path(), &["rev-parse", "feat"]), upstream_tip);
    assert_eq!(
        git(work.path(), &["rev-parse", "base"]),
        base_before,
        "the other branch's HEAD never moved"
    );
    assert_eq!(
        git(work.path(), &["symbolic-ref", "--short", "HEAD"]),
        "feat",
        "still on feat, fast_forward did not touch HEAD"
    );
}

#[test]
fn fast_forward_checked_out_branch_moves_it_too() {
    let (origin, work) = clone_fixture("ff-checked-out");
    fs::write(origin.path().join("a.txt"), "one\ntwo\n").unwrap();
    let origin_repo = Repository::open(origin.path()).unwrap();
    commit_all(&origin_repo, "origin advances");
    git(work.path(), &["fetch", "-q", "origin"]);
    git(work.path(), &["checkout", "-q", "feat"]);
    let upstream_tip = git(work.path(), &["rev-parse", "origin/base"]);

    let backend = Repo::open(work.path()).unwrap();
    backend.fast_forward("feat").unwrap();

    assert_eq!(git(work.path(), &["rev-parse", "feat"]), upstream_tip);
    assert_eq!(git(work.path(), &["rev-parse", "HEAD"]), upstream_tip);
}

#[test]
fn fast_forward_diverged_branches_is_an_error() {
    let (origin, work) = clone_fixture("ff-diverged");
    fs::write(origin.path().join("a.txt"), "one\ntwo\n").unwrap();
    let origin_repo = Repository::open(origin.path()).unwrap();
    commit_all(&origin_repo, "origin advances");
    git(work.path(), &["fetch", "-q", "origin"]);
    fs::write(work.path().join("b.txt"), "local\n").unwrap();
    let work_repo = Repository::open(work.path()).unwrap();
    git(work.path(), &["checkout", "-q", "feat"]);
    commit_all(&work_repo, "local-only work");
    git(work.path(), &["checkout", "-q", "base"]);
    let feat_before = git(work.path(), &["rev-parse", "feat"]);

    let backend = Repo::open(work.path()).unwrap();
    let err = backend.fast_forward("feat").unwrap_err();
    assert!(matches!(err, GitError::BranchFailed(_)), "got {err:?}");
    assert_eq!(git(work.path(), &["rev-parse", "feat"]), feat_before);
}

#[test]
fn fast_forward_with_no_upstream_is_an_error() {
    let dir = two_branch_fixture("ff-no-upstream");
    let backend = Repo::open(dir.path()).unwrap();
    let err = backend.fast_forward("feat").unwrap_err();
    assert!(matches!(err, GitError::BranchFailed(_)), "got {err:?}");
}

#[test]
fn merge_branch_fast_forwardable_lands_clean() {
    let dir = two_branch_fixture("merge-ff");
    let backend = Repo::open(dir.path()).unwrap();
    let feat_tip = git(dir.path(), &["rev-parse", "feat"]);

    let outcome = backend.merge_branch("feat").unwrap();
    assert_eq!(outcome, MergeOutcome::Merged);
    assert_eq!(git(dir.path(), &["rev-parse", "HEAD"]), feat_tip);
}

#[test]
fn merge_branch_needing_a_real_merge_commit_lands_clean() {
    let dir = two_branch_fixture("merge-real");
    let repo = Repository::open(dir.path()).unwrap();
    fs::write(dir.path().join("c.txt"), "main-only\n").unwrap();
    commit_all(&repo, "main advances too");

    let backend = Repo::open(dir.path()).unwrap();
    let outcome = backend.merge_branch("feat").unwrap();
    assert_eq!(outcome, MergeOutcome::Merged);
    assert!(
        !git(dir.path(), &["log", "--merges", "-1", "--format=%H"]).is_empty(),
        "a merge commit landed"
    );
}

#[test]
fn merge_branch_with_nothing_to_merge_is_still_merged() {
    let dir = two_branch_fixture("merge-noop");
    let backend = Repo::open(dir.path()).unwrap();
    backend.merge_branch("feat").unwrap();
    let outcome = backend.merge_branch("feat").unwrap();
    assert_eq!(outcome, MergeOutcome::Merged, "already up to date");
}

#[test]
fn merge_branch_conflicting_reports_conflicted_and_leaves_the_conflict_visible() {
    let dir = TempDir::new("merge-conflict");
    let repo = Repository::init(dir.path()).unwrap();
    configure_identity(dir.path());
    fs::write(dir.path().join("a.txt"), "one\n").unwrap();
    commit_all(&repo, "init");
    git(dir.path(), &["checkout", "-q", "-b", "feat"]);
    fs::write(dir.path().join("a.txt"), "one\nfeat-change\n").unwrap();
    commit_all(&repo, "feat edits a.txt");
    git(dir.path(), &["checkout", "-q", "-"]); // back to whatever init picked
    fs::write(dir.path().join("a.txt"), "one\nbase-change\n").unwrap();
    commit_all(&repo, "base edits a.txt too");

    let backend = Repo::open(dir.path()).unwrap();
    let outcome = backend.merge_branch("feat").unwrap();
    assert_eq!(outcome, MergeOutcome::Conflicted);

    let status = git(dir.path(), &["status", "--porcelain=v2"]);
    assert!(status.contains("u "), "a conflict entry shows: {status}");
}
