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
//! `Repo::has_conflict_markers` and `Repo::stage_all_except`
//! (`docs/PLAN_11_REBASE.md` R0): `git add` would mark an unmerged path
//! resolved whatever the file holds, so ferrit checks for markers first.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use ferrit::domain::git::Repo;
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

/// `main` and `side` both rewrite `f`, then `side` is merged into `main`:
/// `f` is `UU` and holds conflict markers. `n` is a clean, tracked file.
fn conflict_repo(tag: &str) -> TempDir {
    let dir = TempDir::new(tag);
    let repo = Repository::init(dir.path()).unwrap();
    configure_identity(dir.path());
    git(dir.path(), &["checkout", "-q", "-b", "main"]);
    fs::write(dir.path().join("f"), "base\n").unwrap();
    fs::write(dir.path().join("n"), "n\n").unwrap();
    commit_all(&repo, "base");
    git(dir.path(), &["checkout", "-q", "-b", "side"]);
    fs::write(dir.path().join("f"), "side\n").unwrap();
    commit_all(&repo, "side");
    git(dir.path(), &["checkout", "-q", "main"]);
    fs::write(dir.path().join("f"), "main\n").unwrap();
    commit_all(&repo, "main");
    // A conflicting merge exits non-zero by design.
    let _ = Command::new("git")
        .arg("-C")
        .arg(dir.path())
        .args(["merge", "side"])
        .output()
        .unwrap();
    dir
}

#[test]
fn a_freshly_conflicted_file_has_markers() {
    let dir = conflict_repo("markers-fresh");
    let repo = Repo::open(dir.path()).unwrap();
    assert!(repo.has_conflict_markers(Path::new("f")).unwrap());
    assert!(!repo.has_conflict_markers(Path::new("n")).unwrap());
}

#[test]
fn a_resolved_file_has_no_markers() {
    let dir = conflict_repo("markers-resolved");
    fs::write(dir.path().join("f"), "resolved\n").unwrap();
    let repo = Repo::open(dir.path()).unwrap();
    assert!(!repo.has_conflict_markers(Path::new("f")).unwrap());
}

#[test]
fn a_lone_equals_line_is_not_a_marker() {
    // `=======` is also a Markdown heading underline.
    let dir = conflict_repo("markers-setext");
    fs::write(dir.path().join("f"), "Title\n=======\nbody\n").unwrap();
    let repo = Repo::open(dir.path()).unwrap();
    assert!(!repo.has_conflict_markers(Path::new("f")).unwrap());
}

#[test]
fn a_half_removed_marker_block_still_counts() {
    let dir = conflict_repo("markers-half");
    fs::write(
        dir.path().join("f"),
        "<<<<<<< HEAD\nmain\n=======\nside\n>>>>>>> side\n",
    )
    .unwrap();
    let repo = Repo::open(dir.path()).unwrap();
    assert!(repo.has_conflict_markers(Path::new("f")).unwrap());
}

#[test]
fn a_missing_file_has_no_markers() {
    let dir = conflict_repo("markers-missing");
    let repo = Repo::open(dir.path()).unwrap();
    assert!(!repo.has_conflict_markers(Path::new("gone")).unwrap());
}

#[test]
fn stage_all_except_leaves_the_excluded_path_unmerged() {
    let dir = conflict_repo("except-basic");
    fs::write(dir.path().join("n"), "n\nchanged\n").unwrap();
    fs::write(dir.path().join("new.txt"), "new\n").unwrap();
    let repo = Repo::open(dir.path()).unwrap();

    repo.stage_all_except(&[PathBuf::from("f")]).unwrap();

    let status = git(dir.path(), &["status", "--porcelain"]);
    assert!(status.contains("UU f"), "{status}");
    assert!(status.contains("M  n"), "{status}");
    assert!(status.contains("A  new.txt"), "{status}");
}

#[test]
fn stage_all_except_takes_paths_literally() {
    let dir = conflict_repo("except-literal");
    fs::write(dir.path().join("we ird [1].txt"), "x\n").unwrap();
    let repo = Repo::open(dir.path()).unwrap();

    repo.stage_all_except(&[PathBuf::from("f")]).unwrap();

    let status = git(dir.path(), &["status", "--porcelain"]);
    assert!(status.contains("we ird [1].txt"), "{status}");
    assert!(status.contains("UU f"), "{status}");
}

// ------------------------------------------------------- P4: take ours / theirs

#[test]
fn taking_ours_or_theirs_replaces_the_file_and_clears_the_markers() {
    for (ours, expected) in [(true, "main\n"), (false, "side\n")] {
        let dir = conflict_repo(&format!("side-{ours}"));
        let repo = Repo::open(dir.path()).unwrap();
        assert!(repo.has_conflict_markers(Path::new("f")).unwrap());

        repo.take_side(Path::new("f"), ours).unwrap();

        assert_eq!(fs::read_to_string(dir.path().join("f")).unwrap(), expected);
        assert!(!repo.has_conflict_markers(Path::new("f")).unwrap());
        assert!(
            git(dir.path(), &["status", "--porcelain"]).contains("UU f"),
            "still unmerged until it is staged"
        );
        git(dir.path(), &["add", "f"]);
        let status = git(dir.path(), &["status", "--porcelain"]);
        assert!(!status.contains("UU"), "staging resolved it: {status}");
        // Ours is what `HEAD` already has, so nothing is left to commit for it.
        assert_eq!(status.contains("M  f"), !ours, "{status}");
    }
}

#[test]
fn taking_a_side_of_a_path_git_does_not_know_fails_and_a_merged_file_is_untouched() {
    let dir = conflict_repo("side-unknown");
    let repo = Repo::open(dir.path()).unwrap();
    let err = repo.take_side(Path::new("no-such-file"), true).unwrap_err();
    assert!(
        matches!(err, ferrit::domain::git::error::GitError::ApplyFailed(_)),
        "got {err:?}"
    );

    // A file that is not conflicted is left as it is: git treats it as a no-op.
    repo.take_side(Path::new("n"), true).unwrap();
    assert_eq!(fs::read_to_string(dir.path().join("n")).unwrap(), "n\n");
}
