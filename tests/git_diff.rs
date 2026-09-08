//! Subprocess coverage for `Repo::file_diff` / `Repo::commit_diff`: build a
//! throwaway repo with `git2`, then check the plain-text `git diff` / `git show`
//! output comes back parsed. Needs `git` on `PATH` (the same assumption lazygit
//! makes).

use std::fs;
use std::path::{Path, PathBuf};

use ferrit::git::{DiffOpts, DiffSide, GitError, Repo};
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
        TempDir(path)
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

fn commit_all(repo: &Repository, message: &str) -> git2::Oid {
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
        .unwrap()
}

fn stage_all(repo: &Repository) {
    let mut index = repo.index().unwrap();
    index
        .add_all(["*"].iter(), IndexAddOption::DEFAULT, None)
        .unwrap();
    index.write().unwrap();
}

#[test]
fn modified_tracked_file_diffs_against_the_index() {
    let dir = TempDir::new("diff-mod");
    let repo = Repository::init(dir.path()).unwrap();
    fs::write(dir.path().join("a.txt"), "one\ntwo\nthree\n").unwrap();
    commit_all(&repo, "init");
    fs::write(dir.path().join("a.txt"), "one\nTWO\nthree\n").unwrap();

    let backend = Repo::open(dir.path()).unwrap();
    let diff = backend
        .file_diff(Path::new("a.txt"), DiffSide::Worktree, &DiffOpts::default())
        .unwrap();

    assert_eq!(diff.files.len(), 1);
    assert!(diff.text.contains("-two"));
    assert!(diff.text.contains("+TWO"));
    assert_eq!(diff.files[0].hunks.len(), 1);
}

#[test]
fn untracked_file_comes_back_as_all_additions() {
    let dir = TempDir::new("diff-untracked");
    let repo = Repository::init(dir.path()).unwrap();
    fs::write(dir.path().join("seed.txt"), "seed\n").unwrap();
    commit_all(&repo, "init");
    fs::write(dir.path().join("new.txt"), "fresh line\nsecond\n").unwrap();

    let backend = Repo::open(dir.path()).unwrap();
    let diff = backend
        .file_diff(Path::new("new.txt"), DiffSide::Worktree, &DiffOpts::default())
        .unwrap();

    assert_eq!(diff.files.len(), 1, "--no-index fallback produced a section");
    assert!(diff.text.contains("+fresh line"));
    assert!(diff.text.contains("+second"));
}

#[test]
fn staged_side_diffs_the_index_against_head() {
    let dir = TempDir::new("diff-staged");
    let repo = Repository::init(dir.path()).unwrap();
    fs::write(dir.path().join("s.txt"), "base\n").unwrap();
    commit_all(&repo, "init");
    fs::write(dir.path().join("s.txt"), "base\nstaged addition\n").unwrap();
    stage_all(&repo);

    let backend = Repo::open(dir.path()).unwrap();
    let diff = backend
        .file_diff(Path::new("s.txt"), DiffSide::Staged, &DiffOpts::default())
        .unwrap();

    assert!(diff.text.contains("+staged addition"));

    // Worktree side is clean now (everything staged): no file section.
    let worktree = backend
        .file_diff(Path::new("s.txt"), DiffSide::Worktree, &DiffOpts::default())
        .unwrap();
    assert!(worktree.files.is_empty());
}

#[test]
fn binary_file_is_flagged_not_dumped() {
    let dir = TempDir::new("diff-binary");
    let repo = Repository::init(dir.path()).unwrap();
    fs::write(dir.path().join("keep.txt"), "keep\n").unwrap();
    commit_all(&repo, "init");
    fs::write(dir.path().join("blob.bin"), [0u8, 1, 2, 0, 255, 0, 10]).unwrap();

    let backend = Repo::open(dir.path()).unwrap();
    let diff = backend
        .file_diff(Path::new("blob.bin"), DiffSide::Worktree, &DiffOpts::default())
        .unwrap();

    assert_eq!(diff.files.len(), 1);
    assert!(diff.files[0].binary, "NUL byte marks it binary");
}

#[test]
fn commit_diff_shows_the_root_commit_against_the_empty_tree() {
    let dir = TempDir::new("show-root");
    let repo = Repository::init(dir.path()).unwrap();
    fs::write(dir.path().join("r.txt"), "root content\n").unwrap();
    let oid = commit_all(&repo, "root");

    let backend = Repo::open(dir.path()).unwrap();
    let diff = backend
        .commit_diff(&oid.to_string(), &DiffOpts::default())
        .unwrap();

    assert!(diff.text.contains("+root content"));
    assert!(diff.text.contains("new file mode"), "root commit adds the file");
    assert_eq!(diff.files.len(), 1);
}

#[test]
fn commit_diff_on_a_bad_hash_is_no_such_commit() {
    let dir = TempDir::new("show-bad");
    let repo = Repository::init(dir.path()).unwrap();
    fs::write(dir.path().join("x.txt"), "x\n").unwrap();
    commit_all(&repo, "init");

    let backend = Repo::open(dir.path()).unwrap();
    let err = backend
        .commit_diff("0000000000000000000000000000000000000000", &DiffOpts::default())
        .unwrap_err();

    assert!(matches!(err, GitError::NoSuchCommit(_)), "got {err:?}");
}
