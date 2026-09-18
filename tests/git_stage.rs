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
//! Subprocess coverage for `Repo::stage_file` / `apply_hunk` / `apply_lines`
//! / `discard_file`: build a throwaway repo with `git2`, drive the backend,
//! then check `git status --porcelain=v2` / `git diff` for the result. See
//! `docs/PLAN_6_STAGING.md` milestones S0..S3.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use ferrit::git::{ApplyDir, ApplyTarget, DiffOpts, DiffSide, GitError, Repo};
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

/// `git <args>` in `dir`, output as trimmed stdout text.
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
fn stage_file_then_reverse_puts_it_back() {
    let dir = TempDir::new("stage-file");
    let repo = Repository::init(dir.path()).unwrap();
    fs::write(dir.path().join("a.txt"), "one\n").unwrap();
    commit_all(&repo, "init");
    fs::write(dir.path().join("a.txt"), "one\ntwo\n").unwrap();

    let backend = Repo::open(dir.path()).unwrap();
    backend
        .stage_file(Path::new("a.txt"), ApplyDir::Forward)
        .unwrap();
    let status = git(dir.path(), &["status", "--porcelain=v2"]);
    assert!(status.starts_with("1 M."), "staged modified: {status}");

    let staged = backend
        .file_diff(Path::new("a.txt"), DiffSide::Staged, DiffOpts::default())
        .unwrap();
    assert!(staged.text.contains("+two"), "staged: {}", staged.text);

    backend
        .stage_file(Path::new("a.txt"), ApplyDir::Reverse)
        .unwrap();
    let staged = backend
        .file_diff(Path::new("a.txt"), DiffSide::Staged, DiffOpts::default())
        .unwrap();
    assert!(staged.files.is_empty(), "back to unstaged: {}", staged.text);
}

#[test]
fn untracked_file_stages_via_add() {
    let dir = TempDir::new("stage-untracked");
    let repo = Repository::init(dir.path()).unwrap();
    fs::write(dir.path().join("seed.txt"), "seed\n").unwrap();
    commit_all(&repo, "init");
    fs::write(dir.path().join("new.txt"), "fresh\n").unwrap();

    let backend = Repo::open(dir.path()).unwrap();
    backend
        .stage_file(Path::new("new.txt"), ApplyDir::Forward)
        .unwrap();
    let status = git(dir.path(), &["status", "--porcelain=v2"]);
    assert!(
        status.contains("new.txt"),
        "still tracked as changed: {status}"
    );
    assert!(
        !status.lines().any(|l| l.starts_with('?')),
        "no longer untracked: {status}"
    );
}

#[test]
fn binary_file_stages_whole() {
    let dir = TempDir::new("stage-binary");
    let repo = Repository::init(dir.path()).unwrap();
    fs::write(dir.path().join("keep.txt"), "keep\n").unwrap();
    commit_all(&repo, "init");
    fs::write(dir.path().join("blob.bin"), [0u8, 1, 2, 0, 255, 0, 10]).unwrap();

    let backend = Repo::open(dir.path()).unwrap();
    backend
        .stage_file(Path::new("blob.bin"), ApplyDir::Forward)
        .unwrap();
    let staged = backend
        .file_diff(Path::new("blob.bin"), DiffSide::Staged, DiffOpts::default())
        .unwrap();
    assert!(staged.files.first().is_some_and(|f| f.binary));
}

#[test]
fn apply_hunk_moves_exactly_one_hunk_of_a_two_hunk_file() {
    let dir = TempDir::new("stage-hunk");
    let repo = Repository::init(dir.path()).unwrap();
    let base: String = (0..40).map(|n| format!("line {n}\n")).collect();
    fs::write(dir.path().join("f.txt"), &base).unwrap();
    commit_all(&repo, "init");
    let edited = base
        .replace("line 5\n", "line 5 CHANGED\n")
        .replace("line 30\n", "line 30 CHANGED\n");
    fs::write(dir.path().join("f.txt"), &edited).unwrap();

    let backend = Repo::open(dir.path()).unwrap();
    let diff = backend
        .file_diff(Path::new("f.txt"), DiffSide::Worktree, DiffOpts::default())
        .unwrap();
    let file = diff.files.first().unwrap();
    assert_eq!(file.hunks.len(), 2, "two separate hunks");
    let first = &file.hunks[0];
    let patch = diff.text.get(file.header.start..first.body.end).unwrap();

    backend
        .apply_hunk(patch, ApplyDir::Forward, ApplyTarget::Index)
        .unwrap();

    let staged = backend
        .file_diff(Path::new("f.txt"), DiffSide::Staged, DiffOpts::default())
        .unwrap();
    assert!(staged.text.contains("+line 5 CHANGED"));
    assert!(!staged.text.contains("+line 30 CHANGED"));

    let worktree = backend
        .file_diff(Path::new("f.txt"), DiffSide::Worktree, DiffOpts::default())
        .unwrap();
    assert!(!worktree.text.contains("+line 5 CHANGED"));
    assert!(worktree.text.contains("+line 30 CHANGED"));
}

#[test]
fn apply_hunk_reverse_unstages_it() {
    let dir = TempDir::new("stage-hunk-reverse");
    let repo = Repository::init(dir.path()).unwrap();
    fs::write(dir.path().join("f.txt"), "a\nb\nc\n").unwrap();
    commit_all(&repo, "init");
    fs::write(dir.path().join("f.txt"), "a\nB\nc\n").unwrap();

    let backend = Repo::open(dir.path()).unwrap();
    let diff = backend
        .file_diff(Path::new("f.txt"), DiffSide::Worktree, DiffOpts::default())
        .unwrap();
    let file = diff.files.first().unwrap();
    let hunk = file.hunks.first().unwrap();
    let patch = diff.text.get(file.header.start..hunk.body.end).unwrap();
    backend
        .apply_hunk(patch, ApplyDir::Forward, ApplyTarget::Index)
        .unwrap();

    let staged_diff = backend
        .file_diff(Path::new("f.txt"), DiffSide::Staged, DiffOpts::default())
        .unwrap();
    let staged_file = staged_diff.files.first().unwrap();
    let staged_hunk = staged_file.hunks.first().unwrap();
    let staged_patch = staged_diff
        .text
        .get(staged_file.header.start..staged_hunk.body.end)
        .unwrap();

    backend
        .apply_hunk(staged_patch, ApplyDir::Reverse, ApplyTarget::Index)
        .unwrap();

    let staged = backend
        .file_diff(Path::new("f.txt"), DiffSide::Staged, DiffOpts::default())
        .unwrap();
    assert!(
        staged.files.is_empty(),
        "unstaged back out: {}",
        staged.text
    );
}

#[test]
fn apply_lines_stages_one_addition_out_of_three() {
    let dir = TempDir::new("stage-lines");
    let repo = Repository::init(dir.path()).unwrap();
    fs::write(dir.path().join("f.txt"), "base\n").unwrap();
    commit_all(&repo, "init");
    fs::write(dir.path().join("f.txt"), "base\none\ntwo\nthree\n").unwrap();

    let backend = Repo::open(dir.path()).unwrap();
    let diff = backend
        .file_diff(Path::new("f.txt"), DiffSide::Worktree, DiffOpts::default())
        .unwrap();
    let file = diff.files.first().unwrap();
    let hunk = file.hunks.first().unwrap();
    let file_header = diff.text.get(file.header.clone()).unwrap();
    let hunk_header = diff.text.get(hunk.header.clone()).unwrap();
    let hunk_body = diff.text.get(hunk.body.clone()).unwrap();
    let two_index = hunk_body
        .split_inclusive('\n')
        .position(|l| l.trim_end() == "+two")
        .unwrap();

    backend
        .apply_lines(
            file_header,
            hunk_header,
            hunk_body,
            &[two_index],
            ApplyDir::Forward,
            ApplyTarget::Index,
        )
        .unwrap();

    let staged = backend
        .file_diff(Path::new("f.txt"), DiffSide::Staged, DiffOpts::default())
        .unwrap();
    assert!(staged.text.contains("+two"));
    assert!(!staged.text.contains("+one"));
    assert!(!staged.text.contains("+three"));

    let worktree = backend
        .file_diff(Path::new("f.txt"), DiffSide::Worktree, DiffOpts::default())
        .unwrap();
    assert!(worktree.text.contains("+one"));
    assert!(worktree.text.contains("+three"));
    assert!(!worktree.text.contains("+two"));
}

#[test]
fn discard_worktree_hunk_restores_the_index_content() {
    let dir = TempDir::new("stage-discard");
    let repo = Repository::init(dir.path()).unwrap();
    fs::write(dir.path().join("f.txt"), "a\nb\nc\n").unwrap();
    commit_all(&repo, "init");
    fs::write(dir.path().join("f.txt"), "a\nB\nc\n").unwrap();

    let backend = Repo::open(dir.path()).unwrap();
    let diff = backend
        .file_diff(Path::new("f.txt"), DiffSide::Worktree, DiffOpts::default())
        .unwrap();
    let file = diff.files.first().unwrap();
    let hunk = file.hunks.first().unwrap();
    let patch = diff.text.get(file.header.start..hunk.body.end).unwrap();

    backend
        .apply_hunk(patch, ApplyDir::Reverse, ApplyTarget::Worktree)
        .unwrap();

    assert_eq!(
        fs::read_to_string(dir.path().join("f.txt")).unwrap(),
        "a\nb\nc\n",
        "worktree reverted to the index content"
    );
    let worktree = backend
        .file_diff(Path::new("f.txt"), DiffSide::Worktree, DiffOpts::default())
        .unwrap();
    assert!(worktree.files.is_empty());
}

#[test]
fn discard_file_removes_an_untracked_file() {
    let dir = TempDir::new("discard-untracked");
    let repo = Repository::init(dir.path()).unwrap();
    fs::write(dir.path().join("seed.txt"), "seed\n").unwrap();
    commit_all(&repo, "init");
    fs::write(dir.path().join("new.txt"), "fresh\n").unwrap();

    let backend = Repo::open(dir.path()).unwrap();
    backend.discard_file(Path::new("new.txt"), true).unwrap();
    assert!(!dir.path().join("new.txt").exists());
}

#[test]
fn discard_file_restores_a_tracked_files_worktree_content() {
    let dir = TempDir::new("discard-tracked");
    let repo = Repository::init(dir.path()).unwrap();
    fs::write(dir.path().join("f.txt"), "a\n").unwrap();
    commit_all(&repo, "init");
    fs::write(dir.path().join("f.txt"), "changed\n").unwrap();

    let backend = Repo::open(dir.path()).unwrap();
    backend.discard_file(Path::new("f.txt"), false).unwrap();
    assert_eq!(fs::read_to_string(dir.path().join("f.txt")).unwrap(), "a\n");
}

#[test]
fn context_drift_leaves_the_index_untouched() {
    let dir = TempDir::new("stage-drift");
    let repo = Repository::init(dir.path()).unwrap();
    fs::write(dir.path().join("f.txt"), "a\nb\nc\n").unwrap();
    commit_all(&repo, "init");
    fs::write(dir.path().join("f.txt"), "a\nB\nc\n").unwrap();

    let backend = Repo::open(dir.path()).unwrap();
    let diff = backend
        .file_diff(Path::new("f.txt"), DiffSide::Worktree, DiffOpts::default())
        .unwrap();
    let file = diff.files.first().unwrap();
    let hunk = file.hunks.first().unwrap();
    let patch = diff.text.get(file.header.start..hunk.body.end).unwrap();

    // Stage it once for real, then try to apply the *same* (now stale) patch
    // again: the index no longer matches its expected "old" side.
    backend
        .apply_hunk(patch, ApplyDir::Forward, ApplyTarget::Index)
        .unwrap();
    let err = backend
        .apply_hunk(patch, ApplyDir::Forward, ApplyTarget::Index)
        .unwrap_err();
    assert!(matches!(err, GitError::ApplyFailed(_)), "got {err:?}");

    let staged = backend
        .file_diff(Path::new("f.txt"), DiffSide::Staged, DiffOpts::default())
        .unwrap();
    assert!(
        staged.text.contains("+B"),
        "the earlier successful stage is still there: {}",
        staged.text
    );
}
