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
//! Subprocess coverage for `Repo::remotes` / `fetch` / `pull` / `push`: two
//! `TempDir` fixture repos, one pointing at the other with a plain path
//! remote URL — no network, no real GitHub, the same trick `git`'s own
//! test suite uses. See `docs/PLAN_9_REMOTE.md` milestone S0.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use ferrit::git::{GitError, Repo};
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

/// `origin`, a normal (non-bare) repo with `receive.denyCurrentBranch =
/// ignore` — git's own test-suite trick for a same-machine "remote" that
/// still accepts a push to its checked-out branch, no bare repo or network
/// needed — cloned into `work`, whose `base` tracks `origin/base`. `base`
/// rather than trusting `init.defaultBranch`, same reasoning as
/// `tests/git_branch.rs`'s fixtures. Returns `(origin, work)`.
fn two_repo_fixture(tag: &str) -> (TempDir, TempDir) {
    let origin = TempDir::new(&format!("{tag}-origin"));
    let origin_repo = Repository::init(origin.path()).unwrap();
    configure_identity(origin.path());
    fs::write(origin.path().join("a.txt"), "one\n").unwrap();
    commit_all(&origin_repo, "init");
    git(origin.path(), &["branch", "-m", "base"]);
    git(
        origin.path(),
        &["config", "receive.denyCurrentBranch", "ignore"],
    );

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
    (origin, work)
}

#[test]
fn remotes_lists_every_configured_remote() {
    let dir = TempDir::new("remote-list");
    let repo = Repository::init(dir.path()).unwrap();
    configure_identity(dir.path());
    fs::write(dir.path().join("a.txt"), "one\n").unwrap();
    commit_all(&repo, "init");
    git(
        dir.path(),
        &["remote", "add", "origin", "https://example.com/origin.git"],
    );
    git(
        dir.path(),
        &[
            "remote",
            "add",
            "upstream",
            "https://example.com/upstream.git",
        ],
    );

    let backend = Repo::open(dir.path()).unwrap();
    let remotes = backend.remotes().unwrap();
    assert_eq!(remotes.len(), 2);
    assert_eq!(remotes[0].name, "origin");
    assert_eq!(remotes[0].fetch_url, "https://example.com/origin.git");
    assert_eq!(remotes[0].push_url, "https://example.com/origin.git");
    assert_eq!(remotes[1].name, "upstream");
}

#[test]
fn fetch_with_no_remote_is_a_silent_no_op_pull_is_an_error() {
    let dir = TempDir::new("remote-none");
    let repo = Repository::init(dir.path()).unwrap();
    configure_identity(dir.path());
    fs::write(dir.path().join("a.txt"), "one\n").unwrap();
    commit_all(&repo, "init");

    let backend = Repo::open(dir.path()).unwrap();
    assert!(backend.remotes().unwrap().is_empty());
    // Checked empirically rather than assumed (the plan's own edge-case
    // table guessed a "no remote repository specified" error here): a
    // bare `git fetch` with zero remotes configured has nothing to do and
    // exits 0. `git pull` still needs *something* to merge from and does
    // fail ("no tracking information for the current branch").
    assert_eq!(backend.fetch(None).unwrap(), String::new());
    assert!(matches!(
        backend.pull().unwrap_err(),
        GitError::PullFailed(_)
    ));
}

#[test]
fn fetch_updates_the_remote_tracking_ref_without_touching_local() {
    let (origin, work) = two_repo_fixture("fetch");
    fs::write(origin.path().join("a.txt"), "one\ntwo\n").unwrap();
    let origin_repo = Repository::open(origin.path()).unwrap();
    commit_all(&origin_repo, "origin advances");

    let local_before = git(work.path(), &["rev-parse", "HEAD"]);
    let worktree_before = fs::read_to_string(work.path().join("a.txt")).unwrap();

    let backend = Repo::open(work.path()).unwrap();
    backend.fetch(None).unwrap();

    assert_eq!(
        git(work.path(), &["rev-parse", "refs/remotes/origin/base"]),
        git(origin.path(), &["rev-parse", "HEAD"]),
        "the remote-tracking ref moved"
    );
    assert_eq!(
        git(work.path(), &["rev-parse", "HEAD"]),
        local_before,
        "the local branch did not move"
    );
    assert_eq!(
        fs::read_to_string(work.path().join("a.txt")).unwrap(),
        worktree_before,
        "the worktree is untouched"
    );
}

#[test]
fn pull_fast_forward_moves_the_branch_and_worktree() {
    let (origin, work) = two_repo_fixture("pull-ff");
    fs::write(origin.path().join("a.txt"), "one\ntwo\n").unwrap();
    let origin_repo = Repository::open(origin.path()).unwrap();
    commit_all(&origin_repo, "origin advances");

    let backend = Repo::open(work.path()).unwrap();
    backend.pull().unwrap();

    assert_eq!(
        git(work.path(), &["rev-parse", "HEAD"]),
        git(origin.path(), &["rev-parse", "HEAD"])
    );
    assert_eq!(
        fs::read_to_string(work.path().join("a.txt")).unwrap(),
        "one\ntwo\n"
    );
}

#[test]
fn pull_diverged_with_rebase_false_makes_a_merge_commit() {
    let (origin, work) = two_repo_fixture("pull-diverge-merge");
    // Modern git refuses an ambiguous pull outright ("Need to specify how
    // to reconcile divergent branches") rather than picking a default, so
    // "the merge shape" needs this spelled out same as the rebase case
    // right below does — `pull.rebase = false` is still nothing ferrit
    // itself passes on the command line, just config `git pull` already
    // reads on its own.
    git(work.path(), &["config", "pull.rebase", "false"]);
    fs::write(origin.path().join("a.txt"), "one\ntwo\n").unwrap();
    let origin_repo = Repository::open(origin.path()).unwrap();
    commit_all(&origin_repo, "origin advances");

    fs::write(work.path().join("b.txt"), "local\n").unwrap();
    let work_repo = Repository::open(work.path()).unwrap();
    commit_all(&work_repo, "local work");

    let backend = Repo::open(work.path()).unwrap();
    backend.pull().unwrap();

    assert!(
        !git(work.path(), &["log", "--merges", "-1", "--format=%H"]).is_empty(),
        "a merge commit landed"
    );
}

#[test]
fn pull_diverged_with_rebase_config_replays_the_local_commit() {
    let (origin, work) = two_repo_fixture("pull-diverge-rebase");
    git(work.path(), &["config", "pull.rebase", "true"]);
    fs::write(origin.path().join("a.txt"), "one\ntwo\n").unwrap();
    let origin_repo = Repository::open(origin.path()).unwrap();
    commit_all(&origin_repo, "origin advances");

    fs::write(work.path().join("b.txt"), "local\n").unwrap();
    let work_repo = Repository::open(work.path()).unwrap();
    commit_all(&work_repo, "local work");

    let backend = Repo::open(work.path()).unwrap();
    backend.pull().unwrap();

    assert!(
        git(work.path(), &["log", "--merges", "-1", "--format=%H"]).is_empty(),
        "rebase, not a merge commit"
    );
    assert_eq!(
        git(work.path(), &["log", "-1", "--format=%s"]),
        "local work",
        "the local commit is replayed on top"
    );
    assert_eq!(
        git(work.path(), &["rev-parse", "HEAD~1"]),
        git(origin.path(), &["rev-parse", "HEAD"])
    );
}

#[test]
fn pull_with_a_dirty_worktree_that_would_be_overwritten_is_an_error() {
    let (origin, work) = two_repo_fixture("pull-dirty");
    fs::write(origin.path().join("a.txt"), "one\ntwo\n").unwrap();
    let origin_repo = Repository::open(origin.path()).unwrap();
    commit_all(&origin_repo, "origin advances");

    // Uncommitted, conflicting with the incoming change.
    fs::write(work.path().join("a.txt"), "one\nlocal-dirty\n").unwrap();

    let backend = Repo::open(work.path()).unwrap();
    let err = backend.pull().unwrap_err();
    assert!(matches!(err, GitError::PullFailed(_)), "got {err:?}");
    assert_eq!(
        fs::read_to_string(work.path().join("a.txt")).unwrap(),
        "one\nlocal-dirty\n",
        "worktree untouched"
    );
    assert_eq!(
        git(work.path(), &["rev-parse", "HEAD"]),
        git(work.path(), &["rev-parse", "refs/heads/base"]),
        "local branch untouched"
    );
}

#[test]
fn pull_that_conflicts_is_an_error_and_leaves_the_conflict_visible() {
    let (origin, work) = two_repo_fixture("pull-conflict");
    // Same "modern git refuses an ambiguous pull outright" reason as
    // `pull_diverged_with_rebase_false_makes_a_merge_commit`.
    git(work.path(), &["config", "pull.rebase", "false"]);
    fs::write(origin.path().join("a.txt"), "one\norigin-change\n").unwrap();
    let origin_repo = Repository::open(origin.path()).unwrap();
    commit_all(&origin_repo, "origin edits a.txt");

    fs::write(work.path().join("a.txt"), "one\nlocal-change\n").unwrap();
    let work_repo = Repository::open(work.path()).unwrap();
    commit_all(&work_repo, "local edits a.txt too");

    let backend = Repo::open(work.path()).unwrap();
    let err = backend.pull().unwrap_err();
    // Not a dedicated "conflicted" outcome, unlike phase 8's
    // `MergeOutcome::Conflicted` — a conflicting pull is still reported as
    // a real failure (git's own conflict message, shown verbatim), same
    // "not resolved, not pretended-resolved" promise, one variant simpler.
    assert!(matches!(err, GitError::PullFailed(_)), "got {err:?}");
    let status = git(work.path(), &["status", "--porcelain=v2"]);
    assert!(status.contains("u "), "a conflict entry shows: {status}");
}

#[test]
fn push_with_an_existing_upstream_moves_the_remotes_ref() {
    let (origin, work) = two_repo_fixture("push-basic");
    fs::write(work.path().join("b.txt"), "local\n").unwrap();
    let work_repo = Repository::open(work.path()).unwrap();
    commit_all(&work_repo, "local work");

    let backend = Repo::open(work.path()).unwrap();
    backend.push(None).unwrap();

    assert_eq!(
        git(origin.path(), &["rev-parse", "refs/heads/base"]),
        git(work.path(), &["rev-parse", "HEAD"])
    );
}

#[test]
fn push_with_no_upstream_and_a_chosen_remote_sets_one() {
    let (origin, work) = two_repo_fixture("push-set-upstream");
    git(work.path(), &["checkout", "-q", "-b", "feature"]);
    fs::write(work.path().join("c.txt"), "feature\n").unwrap();
    let work_repo = Repository::open(work.path()).unwrap();
    commit_all(&work_repo, "feature work");

    let backend = Repo::open(work.path()).unwrap();
    backend.push(Some("origin")).unwrap();

    assert_eq!(
        git(work.path(), &["rev-parse", "--abbrev-ref", "feature@{u}"]),
        "origin/feature"
    );
    assert_eq!(
        git(origin.path(), &["rev-parse", "refs/heads/feature"]),
        git(work.path(), &["rev-parse", "HEAD"])
    );
}

#[test]
fn push_with_no_upstream_and_no_chosen_remote_is_no_upstream_error() {
    let (_origin, work) = two_repo_fixture("push-no-upstream");
    git(work.path(), &["checkout", "-q", "-b", "orphan"]);
    fs::write(work.path().join("d.txt"), "orphan\n").unwrap();
    let work_repo = Repository::open(work.path()).unwrap();
    commit_all(&work_repo, "orphan work");

    let backend = Repo::open(work.path()).unwrap();
    let err = backend.push(None).unwrap_err();
    assert!(matches!(err, GitError::NoUpstream), "got {err:?}");
}

#[test]
fn push_rejected_as_non_fast_forward_leaves_the_remote_at_the_accepted_commit() {
    let (origin, work_a) = two_repo_fixture("push-reject");
    let work_b = TempDir::new("push-reject-work-b");
    git(
        Path::new("."),
        &[
            "clone",
            "-q",
            origin.path().to_str().unwrap(),
            work_b.path().to_str().unwrap(),
        ],
    );
    configure_identity(work_b.path());

    fs::write(work_a.path().join("a.txt"), "one\nfrom-a\n").unwrap();
    let repo_a = Repository::open(work_a.path()).unwrap();
    commit_all(&repo_a, "from A");
    Repo::open(work_a.path()).unwrap().push(None).unwrap();
    let accepted_tip = git(origin.path(), &["rev-parse", "refs/heads/base"]);

    fs::write(work_b.path().join("a.txt"), "one\nfrom-b\n").unwrap();
    let repo_b = Repository::open(work_b.path()).unwrap();
    commit_all(&repo_b, "from B, conflicting");
    let err = Repo::open(work_b.path()).unwrap().push(None).unwrap_err();
    assert!(matches!(err, GitError::PushFailed(_)), "got {err:?}");
    assert_eq!(
        git(origin.path(), &["rev-parse", "refs/heads/base"]),
        accepted_tip,
        "the rejected push did not move the remote"
    );
}
