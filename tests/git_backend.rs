//! Unit coverage for the headless git backend (`ferrit::git`).
//!
//! Each test builds a throwaway repository with `git2` directly, then checks
//! what `Repo::snapshot()` reports. Covers a non-repo path, a fresh `git init`
//! with no commits, and a repo with a commit plus working-tree changes.

use std::path::{Path, PathBuf};

use ferrit::git::{Change, GitError, Repo};
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
        TempDir(path)
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
    assert!(!snap.header.branch.is_empty(), "unborn branch still has a name");
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

    let head_branch = repo.head().unwrap().shorthand().unwrap().to_string();
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
