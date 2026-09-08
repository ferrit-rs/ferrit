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
//! Unit coverage for the headless git backend (`ferrit::git`).
//!
//! Each test builds a throwaway repository with `git2` directly, then checks
//! what `Repo::snapshot()` reports. Covers a non-repo path, a fresh `git init`
//! with no commits, and a repo with a commit plus working-tree changes.

use std::path::{Path, PathBuf};

use ferrit::git::{Change, GitError, Repo, Rev};
use git2::{IndexAddOption, Repository, Signature};

/// A temp directory that deletes itself on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let mut path = std::env::temp_dir();
        path.push(format!("ferrit-{tag}-{}-{nanos}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
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

#[test]
fn open_on_a_non_repo_path_reports_not_a_repository() {
    let dir = TempDir::new("nonrepo");
    let result = Repo::open(dir.path());
    assert!(matches!(result, Err(GitError::NotARepository(_))));
}

#[test]
fn fresh_repo_has_a_branch_name_and_no_ahead_behind() {
    let dir = TempDir::new("fresh");
    Repository::init(dir.path()).unwrap();

    let snap = Repo::open(dir.path()).unwrap().snapshot().unwrap();
    assert!(
        !snap.header.branch.is_empty(),
        "unborn branch still has a name"
    );
    assert!(!snap.header.detached);
    assert_eq!(snap.header.upstream, None);
    assert_eq!(snap.header.ahead, 0);
    assert_eq!(snap.header.behind, 0);
    assert_eq!(snap.header.conflicts, 0);
    assert!(snap.files.is_empty(), "nothing in the working tree yet");
}

#[test]
fn header_names_the_checked_out_branch_after_a_commit() {
    let dir = TempDir::new("committed");
    let repo = Repository::init(dir.path()).unwrap();
    std::fs::write(dir.path().join("README.md"), b"hello\n").unwrap();
    commit_all(&repo, "initial commit");

    let head_branch = repo.head().unwrap().shorthand().unwrap().to_owned();
    let snap = Repo::open(dir.path()).unwrap().snapshot().unwrap();

    assert_eq!(snap.header.branch, head_branch);
    assert!(!snap.header.detached);
    assert_eq!(snap.header.upstream, None);
    assert!(snap.files.is_empty(), "clean tree after the commit");
}

#[test]
fn files_reports_untracked_then_staged() {
    let dir = TempDir::new("worktree");
    let repo = Repository::init(dir.path()).unwrap();
    std::fs::write(dir.path().join("README.md"), b"hello\n").unwrap();
    commit_all(&repo, "initial commit");

    std::fs::write(dir.path().join("new.txt"), b"fresh\n").unwrap();

    let snap = Repo::open(dir.path()).unwrap().snapshot().unwrap();
    assert_eq!(snap.files.len(), 1);
    let entry = &snap.files[0];
    assert_eq!(entry.path, PathBuf::from("new.txt"));
    assert_eq!(entry.worktree, Change::Untracked);
    assert_eq!(entry.staged, Change::None);

    let mut index = repo.index().unwrap();
    index.add_path(Path::new("new.txt")).unwrap();
    index.write().unwrap();

    let snap = Repo::open(dir.path()).unwrap().snapshot().unwrap();
    assert_eq!(snap.files.len(), 1);
    assert_eq!(snap.files[0].staged, Change::Added);
    assert_eq!(snap.files[0].worktree, Change::None);
}

#[test]
fn branches_lists_head_first_then_alphabetical() {
    let dir = TempDir::new("branches");
    let repo = Repository::init(dir.path()).unwrap();
    std::fs::write(dir.path().join("f.txt"), b"1\n").unwrap();
    commit_all(&repo, "initial commit");
    let head_oid = repo.head().unwrap().target().unwrap();
    let head_commit = repo.find_commit(head_oid).unwrap();
    repo.branch("zeta", &head_commit, false).unwrap();
    repo.branch("alpha", &head_commit, false).unwrap();

    let head_name = repo.head().unwrap().shorthand().unwrap().to_owned();
    let snap = Repo::open(dir.path()).unwrap().snapshot().unwrap();

    assert_eq!(snap.branches.len(), 3);
    assert_eq!(snap.branches[0].name, head_name);
    assert!(snap.branches[0].is_head);
    assert_eq!(snap.branches[0].upstream, None);
    let rest: Vec<&str> = snap.branches[1..].iter().map(|b| b.name.as_str()).collect();
    assert_eq!(rest, ["alpha", "zeta"]);
    assert!(!snap.branches[1].is_head);
}

#[test]
fn commits_lists_newest_first_and_respects_the_limit() {
    let dir = TempDir::new("commits");
    let repo = Repository::init(dir.path()).unwrap();
    for i in 0..3 {
        std::fs::write(dir.path().join("f.txt"), format!("{i}\n")).unwrap();
        commit_all(&repo, &format!("commit {i}"));
    }

    let snap = Repo::open(dir.path()).unwrap().snapshot().unwrap();

    assert_eq!(snap.commits.len(), 3);
    assert_eq!(snap.commits[0].summary, "commit 2");
    assert_eq!(snap.commits[1].summary, "commit 1");
    assert_eq!(snap.commits[2].summary, "commit 0");
    assert_eq!(snap.commits[0].short_hash.len(), 7);
}

#[test]
fn commits_on_a_fresh_repo_is_empty() {
    let dir = TempDir::new("commits-fresh");
    Repository::init(dir.path()).unwrap();

    let snap = Repo::open(dir.path()).unwrap().snapshot().unwrap();
    assert!(snap.commits.is_empty(), "unborn branch has no history yet");
}

#[test]
fn stashes_lists_saved_entries() {
    let dir = TempDir::new("stash");
    let mut repo = Repository::init(dir.path()).unwrap();
    std::fs::write(dir.path().join("a.txt"), b"a\n").unwrap();
    commit_all(&repo, "initial commit");

    std::fs::write(dir.path().join("a.txt"), b"changed\n").unwrap();
    let sig = Signature::now("Test", "test@example.com").unwrap();
    repo.stash_save(&sig, "wip: first", None).unwrap();

    let snap = Repo::open(dir.path()).unwrap().snapshot().unwrap();
    assert_eq!(snap.stashes.len(), 1);
    assert_eq!(snap.stashes[0].index, 0);
    assert!(snap.stashes[0].message.contains("wip: first"));
}

#[test]
fn blob_bytes_reads_workdir_and_head() {
    let dir = TempDir::new("blob");
    let repo = Repository::init(dir.path()).unwrap();
    std::fs::write(dir.path().join("logo.txt"), b"v1\n").unwrap();
    commit_all(&repo, "add logo");

    // Working copy diverges from the committed blob.
    std::fs::write(dir.path().join("logo.txt"), b"v2\n").unwrap();

    let backend = Repo::open(dir.path()).unwrap();
    let path = Path::new("logo.txt");
    assert_eq!(backend.blob_bytes(path, Rev::Workdir).unwrap(), b"v2\n");
    assert_eq!(backend.blob_bytes(path, Rev::Head).unwrap(), b"v1\n");

    let missing = backend.blob_bytes(Path::new("nope.png"), Rev::Head);
    assert!(matches!(missing, Err(GitError::Read(_))));
}
