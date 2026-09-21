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
//! Subprocess coverage for `Repo::commit` / `head_message` / `staged_count`:
//! build a throwaway repo with `git2`, configure a commit identity (`git
//! commit` needs one; earlier fixtures never called it), then drive the
//! backend and check `git log` / `git rev-parse`. See
//! `docs/PLAN_7_COMMIT.md` milestone C0.

use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use ferrit::domain::git::Repo;
use ferrit::domain::git::commit::{CommitKind, CommitOpts};
use ferrit::domain::git::error::GitError;
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

/// `git commit` (unlike `add`/`restore`/`apply`) needs a configured
/// identity; the `git2::Signature` earlier fixtures use only covers `git2`'s
/// own in-process commits, not the `git` subprocess this module drives.
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

#[test]
fn commit_normal_creates_a_commit_on_top_of_head() {
    let dir = TempDir::new("commit-normal");
    let repo = Repository::init(dir.path()).unwrap();
    configure_identity(dir.path());
    fs::write(dir.path().join("a.txt"), "one\n").unwrap();
    commit_all(&repo, "init");
    let old_head = git(dir.path(), &["rev-parse", "HEAD"]);
    fs::write(dir.path().join("a.txt"), "one\ntwo\n").unwrap();
    git(dir.path(), &["add", "a.txt"]);

    let backend = Repo::open(dir.path()).unwrap();
    let hash = backend
        .commit(
            &CommitKind::Normal,
            "feat: add a line",
            CommitOpts::default(),
        )
        .unwrap();

    assert_eq!(hash, git(dir.path(), &["rev-parse", "HEAD"]));
    assert_eq!(
        git(dir.path(), &["log", "-1", "--format=%s"]),
        "feat: add a line"
    );
    assert_eq!(git(dir.path(), &["rev-parse", "HEAD^"]), old_head);
}

#[test]
fn commit_normal_with_nothing_staged_is_nothing_staged() {
    let dir = TempDir::new("commit-empty");
    let repo = Repository::init(dir.path()).unwrap();
    configure_identity(dir.path());
    fs::write(dir.path().join("a.txt"), "one\n").unwrap();
    commit_all(&repo, "init");

    let backend = Repo::open(dir.path()).unwrap();
    let err = backend
        .commit(&CommitKind::Normal, "nothing here", CommitOpts::default())
        .unwrap_err();
    assert!(matches!(err, GitError::NothingStaged), "got {err:?}");
}

#[test]
fn commit_amend_changes_the_subject_and_keeps_the_parent() {
    let dir = TempDir::new("commit-amend");
    let repo = Repository::init(dir.path()).unwrap();
    configure_identity(dir.path());
    fs::write(dir.path().join("a.txt"), "one\n").unwrap();
    commit_all(&repo, "base");
    fs::write(dir.path().join("b.txt"), "two\n").unwrap();
    commit_all(&repo, "wip");
    let base_hash = git(dir.path(), &["rev-parse", "HEAD^"]);

    let backend = Repo::open(dir.path()).unwrap();
    backend
        .commit(
            &CommitKind::Amend,
            "feat: the real message",
            CommitOpts::default(),
        )
        .unwrap();

    assert_eq!(
        git(dir.path(), &["log", "-1", "--format=%s"]),
        "feat: the real message"
    );
    assert_eq!(git(dir.path(), &["rev-parse", "HEAD^"]), base_hash);
}

#[test]
fn commit_reword_ignores_a_dirty_index() {
    let dir = TempDir::new("commit-reword");
    let repo = Repository::init(dir.path()).unwrap();
    configure_identity(dir.path());
    fs::write(dir.path().join("a.txt"), "one\n").unwrap();
    commit_all(&repo, "base");
    fs::write(dir.path().join("b.txt"), "two\n").unwrap();
    commit_all(&repo, "wip");

    // A staged change that reword must not fold in.
    fs::write(dir.path().join("c.txt"), "three\n").unwrap();
    git(dir.path(), &["add", "c.txt"]);

    let backend = Repo::open(dir.path()).unwrap();
    backend
        .commit(&CommitKind::Reword, "wip: reworded", CommitOpts::default())
        .unwrap();

    assert_eq!(
        git(dir.path(), &["log", "-1", "--format=%s"]),
        "wip: reworded"
    );
    assert!(
        !git(dir.path(), &["show", "--name-only", "--format="]).contains("c.txt"),
        "the staged file did not get folded into the amended commit"
    );
    assert!(
        git(dir.path(), &["diff", "--cached", "--name-only"]).contains("c.txt"),
        "the staged change is still staged, untouched"
    );
}

#[test]
fn commit_fixup_writes_its_own_message() {
    let dir = TempDir::new("commit-fixup");
    let repo = Repository::init(dir.path()).unwrap();
    configure_identity(dir.path());
    fs::write(dir.path().join("a.txt"), "one\n").unwrap();
    commit_all(&repo, "feat: the target commit");
    let target = git(dir.path(), &["rev-parse", "HEAD"]);
    fs::write(dir.path().join("b.txt"), "two\n").unwrap();
    git(dir.path(), &["add", "b.txt"]);

    let backend = Repo::open(dir.path()).unwrap();
    backend
        .commit(
            &CommitKind::Fixup { target },
            "ignored",
            CommitOpts::default(),
        )
        .unwrap();

    assert_eq!(
        git(dir.path(), &["log", "-1", "--format=%s"]),
        "fixup! feat: the target commit"
    );
}

#[test]
fn sign_off_appends_a_trailer() {
    let dir = TempDir::new("commit-signoff");
    let repo = Repository::init(dir.path()).unwrap();
    configure_identity(dir.path());
    fs::write(dir.path().join("a.txt"), "one\n").unwrap();
    git(dir.path(), &["add", "a.txt"]);
    let _ = repo; // identity only needed via `git config`, not git2 here

    let backend = Repo::open(dir.path()).unwrap();
    backend
        .commit(
            &CommitKind::Normal,
            "feat: signed",
            CommitOpts {
                sign_off: true,
                no_verify: false,
                author: None,
            },
        )
        .unwrap();

    let body = git(dir.path(), &["log", "-1", "--format=%B"]);
    assert!(
        body.contains("Signed-off-by: Test <test@example.com>"),
        "got: {body}"
    );
}

#[test]
fn selected_author_applies_to_commit_without_changing_git_config() {
    let dir = TempDir::new("commit-selected-author");
    let repo = Repository::init(dir.path()).unwrap();
    configure_identity(dir.path());
    fs::write(dir.path().join("a.txt"), "one\n").unwrap();
    git(dir.path(), &["add", "a.txt"]);
    let _ = repo;

    let backend = Repo::open(dir.path()).unwrap();
    backend
        .commit(
            &CommitKind::Normal,
            "feat: selected author",
            CommitOpts {
                author: Some("Chosen User <chosen@example.com>".to_owned()),
                ..CommitOpts::default()
            },
        )
        .unwrap();

    assert_eq!(
        git(dir.path(), &["log", "-1", "--format=%an <%ae>"]).trim(),
        "Chosen User <chosen@example.com>"
    );
    assert_eq!(
        git(dir.path(), &["log", "-1", "--format=%cn <%ce>"]).trim(),
        "Test <test@example.com>"
    );
    assert_eq!(git(dir.path(), &["config", "user.name"]).trim(), "Test");
    assert_eq!(
        git(dir.path(), &["config", "user.email"]).trim(),
        "test@example.com"
    );
}

#[test]
fn pre_commit_hook_rejection_leaves_head_untouched() {
    let dir = TempDir::new("commit-hook");
    let repo = Repository::init(dir.path()).unwrap();
    configure_identity(dir.path());
    fs::write(dir.path().join("a.txt"), "one\n").unwrap();
    commit_all(&repo, "init");

    let hooks_dir = dir.path().join(".git/hooks");
    fs::create_dir_all(&hooks_dir).unwrap();
    let hook_path = hooks_dir.join("pre-commit");
    fs::write(&hook_path, "#!/bin/sh\nexit 1\n").unwrap();
    fs::set_permissions(&hook_path, fs::Permissions::from_mode(0o755)).unwrap();

    fs::write(dir.path().join("a.txt"), "one\ntwo\n").unwrap();
    git(dir.path(), &["add", "a.txt"]);
    let before = git(dir.path(), &["rev-parse", "HEAD"]);

    let backend = Repo::open(dir.path()).unwrap();
    let err = backend
        .commit(
            &CommitKind::Normal,
            "should be rejected",
            CommitOpts::default(),
        )
        .unwrap_err();

    assert!(matches!(err, GitError::CommitFailed(_)), "got {err:?}");
    assert_eq!(
        git(dir.path(), &["rev-parse", "HEAD"]),
        before,
        "no commit was made"
    );
}

#[test]
fn root_commit_on_an_unborn_branch() {
    let dir = TempDir::new("commit-root");
    let _repo = Repository::init(dir.path()).unwrap();
    configure_identity(dir.path());
    fs::write(dir.path().join("a.txt"), "one\n").unwrap();
    git(dir.path(), &["add", "a.txt"]);

    let backend = Repo::open(dir.path()).unwrap();
    backend
        .commit(&CommitKind::Normal, "root", CommitOpts::default())
        .unwrap();

    assert_eq!(git(dir.path(), &["log", "-1", "--format=%s"]), "root");
}

#[test]
fn head_message_and_staged_count() {
    let dir = TempDir::new("commit-introspect");
    let repo = Repository::init(dir.path()).unwrap();
    configure_identity(dir.path());

    let backend = Repo::open(dir.path()).unwrap();
    assert_eq!(backend.head_message().unwrap(), None, "unborn branch");
    assert_eq!(backend.staged_count().unwrap(), 0);

    fs::write(dir.path().join("a.txt"), "one\n").unwrap();
    commit_all(&repo, "feat: first");
    assert_eq!(
        backend.head_message().unwrap().as_deref(),
        Some("feat: first")
    );

    fs::write(dir.path().join("b.txt"), "two\n").unwrap();
    git(dir.path(), &["add", "b.txt"]);
    assert_eq!(backend.staged_count().unwrap(), 1);
}
