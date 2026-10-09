#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "integration test scaffolding: a failed setup is the assertion"
)]
//! The same scenarios run against the real adapter (`Repo`, a throwaway
//! repository on disk) and against `FakeGit`, so the fake cannot drift from
//! what git really does. See `docs/PLAN_21_GIT_PORT.md`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use ferrit::domain::git::apply::ApplyDir;
use ferrit::domain::git::commit::{CommitKind, CommitOpts};
use ferrit::domain::git::error::GitError;
use ferrit::domain::git::fake::FakeGit;
use ferrit::domain::git::model::Change;
use ferrit::domain::git::port::GitPort;
use ferrit::infra::git::Repo;

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("ferrit-{tag}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn git(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

/// A repository on branch `main` holding an untracked `a.txt`, and, when
/// `with_commit`, one commit before it.
fn real(tag: &str, with_commit: bool) -> (TempDir, Box<dyn GitPort>) {
    let dir = TempDir::new(tag);
    git(&dir.0, &["init", "-q", "-b", "main"]);
    git(&dir.0, &["config", "user.name", "Test"]);
    git(&dir.0, &["config", "user.email", "test@example.com"]);
    git(&dir.0, &["config", "commit.gpgsign", "false"]);
    if with_commit {
        fs::write(dir.0.join("seed.txt"), "seed\n").unwrap();
        git(&dir.0, &["add", "-A"]);
        git(&dir.0, &["commit", "-q", "-m", "seed"]);
    }
    fs::write(dir.0.join("a.txt"), "hello\n").unwrap();
    let repo = Repo::open(&dir.0).unwrap();
    (dir, Box::new(repo))
}

fn fake(with_commit: bool) -> Box<dyn GitPort> {
    let fake = FakeGit::new("contract").with_file("a.txt", Change::None, Change::Untracked);
    Box::new(if with_commit {
        fake.with_commit("seed")
    } else {
        fake
    })
}

fn state_of(git: &mut dyn GitPort, path: &str) -> (Change, Change) {
    let snapshot = git.snapshot().unwrap();
    let file = snapshot
        .files
        .iter()
        .find(|file| file.path == Path::new(path))
        .unwrap_or_else(|| panic!("{path} is not listed"));
    (file.staged, file.worktree)
}

// Unstaging needs a `HEAD`: on a repository with no commit yet `Repo` reports
// `could not resolve HEAD`, which the fake does not model (a known gap, found
// by this suite), so both start from one commit.
fn stage_unstage_and_commit(git: &mut dyn GitPort) {
    assert_eq!(state_of(git, "a.txt"), (Change::None, Change::Untracked));

    git.stage_file(Path::new("a.txt"), ApplyDir::Forward)
        .unwrap();
    assert_eq!(state_of(git, "a.txt"), (Change::Added, Change::None));

    git.stage_file(Path::new("a.txt"), ApplyDir::Reverse)
        .unwrap();
    assert_eq!(state_of(git, "a.txt"), (Change::None, Change::Untracked));

    git.stage_all(ApplyDir::Forward).unwrap();
    git.commit(&CommitKind::Normal, "add a", CommitOpts::default())
        .unwrap();
    let snapshot = git.snapshot().unwrap();
    assert!(
        snapshot
            .files
            .iter()
            .all(|file| file.path != Path::new("a.txt"))
    );
    assert_eq!(
        snapshot.commits.first().map(|c| c.summary.as_str()),
        Some("add a")
    );
    assert!(git.has_commits());

    let nothing = git.commit(&CommitKind::Normal, "again", CommitOpts::default());
    assert!(
        matches!(nothing, Err(GitError::NothingStaged)),
        "{nothing:?}"
    );
}

fn branch_lifecycle(git: &mut dyn GitPort) {
    git.create_branch("feat").unwrap();
    assert_eq!(git.snapshot().unwrap().header.branch, "feat");

    let twice = git.create_branch("feat");
    assert!(matches!(twice, Err(GitError::BranchFailed(_))), "{twice:?}");

    git.checkout("main").unwrap();
    let snapshot = git.snapshot().unwrap();
    assert_eq!(snapshot.header.branch, "main");
    assert!(
        snapshot
            .branches
            .iter()
            .any(|b| b.name == "feat" && !b.is_head)
    );

    let head = git.delete_branch("main", false);
    assert!(matches!(head, Err(GitError::BranchFailed(_))), "{head:?}");

    git.delete_branch("feat", false).unwrap();
    assert!(
        git.snapshot()
            .unwrap()
            .branches
            .iter()
            .all(|b| b.name != "feat")
    );
}

#[test]
fn stage_unstage_and_commit_on_a_real_repository() {
    let (_dir, mut git) = real("contract-stage", true);
    stage_unstage_and_commit(git.as_mut());
}

#[test]
fn stage_unstage_and_commit_on_the_fake() {
    stage_unstage_and_commit(fake(true).as_mut());
}

#[test]
fn branch_lifecycle_on_a_real_repository() {
    let (_dir, mut git) = real("contract-branch", true);
    branch_lifecycle(git.as_mut());
}

#[test]
fn branch_lifecycle_on_the_fake() {
    branch_lifecycle(fake(true).as_mut());
}

#[test]
fn a_queued_failure_hits_the_next_call_only_and_calls_are_recorded() {
    let fake = FakeGit::new("failing")
        .with_file("a.txt", Change::None, Change::Untracked)
        .fail_next("stage_file", GitError::ApplyFailed("boom".to_owned()));
    let git: &dyn GitPort = &fake;
    let first = git.stage_file(Path::new("a.txt"), ApplyDir::Forward);
    assert!(matches!(first, Err(GitError::ApplyFailed(_))), "{first:?}");
    git.stage_file(Path::new("a.txt"), ApplyDir::Forward)
        .unwrap();
    assert_eq!(fake.calls(), ["stage_file", "stage_file"]);
}

#[test]
fn a_reopened_handle_shares_the_fake_state() {
    let fake = FakeGit::new("shared").with_file("a.txt", Change::None, Change::Untracked);
    let other = fake.reopen().unwrap();
    other.stage_all(ApplyDir::Forward).unwrap();
    assert_eq!(
        fake.current().files.first().map(|f| f.staged),
        Some(Change::Added)
    );
}
