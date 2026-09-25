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
//! Subprocess coverage for `Repo::stash_push` / `stash_apply` / `stash_pop` /
//! `stash_drop` / `stash_diff`: throwaway fixture repos, then real `git`
//! state. See `docs/PLAN_10_STASH.md` milestone S0.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use ferrit::domain::git::Repo;
use ferrit::domain::git::diff::DiffOpts;
use ferrit::domain::git::error::GitError;
use ferrit::domain::git::stash::StashOutcome;
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

/// One commit with `a.txt`.
fn fixture(tag: &str) -> TempDir {
    let dir = TempDir::new(tag);
    let repo = Repository::init(dir.path()).unwrap();
    configure_identity(dir.path());
    fs::write(dir.path().join("a.txt"), "one\n").unwrap();
    commit_all(&repo, "init");
    dir
}

fn dirty(dir: &TempDir) {
    fs::write(dir.path().join("a.txt"), "one\nlocal\n").unwrap();
    fs::write(dir.path().join("new.txt"), "untracked\n").unwrap();
}

fn oids(repo: &mut Repo) -> Vec<String> {
    repo.snapshot()
        .unwrap()
        .stashes
        .into_iter()
        .map(|e| e.oid)
        .collect()
}

#[test]
fn push_stashes_tracked_and_untracked_with_the_message() {
    let dir = fixture("stash-push");
    dirty(&dir);
    let mut repo = Repo::open(dir.path()).unwrap();
    repo.stash_push("parked").unwrap();

    let stashes = repo.snapshot().unwrap().stashes;
    assert_eq!(stashes.len(), 1);
    assert!(stashes[0].message.contains("parked"), "{:?}", stashes[0]);
    assert_eq!(git(dir.path(), &["status", "--porcelain"]), "");
    assert!(!dir.path().join("new.txt").exists());
}

#[test]
fn push_without_a_message_uses_gits_wip_message() {
    let dir = fixture("stash-wip");
    dirty(&dir);
    let mut repo = Repo::open(dir.path()).unwrap();
    repo.stash_push("").unwrap();
    let stashes = repo.snapshot().unwrap().stashes;
    assert!(stashes[0].message.starts_with("WIP on"), "{:?}", stashes[0]);
}

#[test]
fn push_on_a_clean_tree_is_nothing_to_stash() {
    let dir = fixture("stash-clean");
    let repo = Repo::open(dir.path()).unwrap();
    let err = repo.stash_push("x").unwrap_err();
    assert!(matches!(err, GitError::NothingToStash), "got {err:?}");
}

#[test]
fn apply_keeps_the_entry_and_restores_the_content() {
    let dir = fixture("stash-apply");
    dirty(&dir);
    let mut repo = Repo::open(dir.path()).unwrap();
    repo.stash_push("").unwrap();
    let oid = oids(&mut repo).remove(0);

    assert_eq!(repo.stash_apply(&oid).unwrap(), StashOutcome::Done);
    assert_eq!(oids(&mut repo), vec![oid]);
    assert_eq!(
        fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "one\nlocal\n"
    );
    assert!(dir.path().join("new.txt").exists());
}

#[test]
fn pop_removes_the_entry_and_restores_the_content() {
    let dir = fixture("stash-pop");
    dirty(&dir);
    let mut repo = Repo::open(dir.path()).unwrap();
    repo.stash_push("").unwrap();
    let oid = oids(&mut repo).remove(0);

    assert_eq!(repo.stash_pop(&oid).unwrap(), StashOutcome::Done);
    assert!(oids(&mut repo).is_empty());
    assert!(dir.path().join("new.txt").exists());
}

#[test]
fn pop_into_a_conflicting_worktree_is_conflicted_and_keeps_the_entry() {
    let dir = fixture("stash-conflict");
    fs::write(dir.path().join("a.txt"), "one\nstashed\n").unwrap();
    let mut repo = Repo::open(dir.path()).unwrap();
    repo.stash_push("").unwrap();
    let oid = oids(&mut repo).remove(0);
    // Diverge committed history so applying the stash conflicts.
    fs::write(dir.path().join("a.txt"), "one\ncommitted\n").unwrap();
    commit_all(&Repository::open(dir.path()).unwrap(), "diverge");

    assert_eq!(repo.stash_pop(&oid).unwrap(), StashOutcome::Conflicted);
    assert_eq!(oids(&mut repo), vec![oid]);
    assert!(git(dir.path(), &["status", "--porcelain"]).contains("UU a.txt"));
}

#[test]
fn apply_onto_an_overlapping_dirty_file_fails() {
    let dir = fixture("stash-overlap");
    fs::write(dir.path().join("a.txt"), "one\nstashed\n").unwrap();
    let mut repo = Repo::open(dir.path()).unwrap();
    repo.stash_push("").unwrap();
    let oid = oids(&mut repo).remove(0);
    fs::write(dir.path().join("a.txt"), "one\nother\n").unwrap();

    let err = repo.stash_apply(&oid).unwrap_err();
    assert!(matches!(err, GitError::StashFailed(_)), "got {err:?}");
    assert_eq!(oids(&mut repo), vec![oid]);
}

#[test]
fn drop_removes_exactly_the_entry_even_after_indices_shifted() {
    let dir = fixture("stash-drop");
    let mut repo = Repo::open(dir.path()).unwrap();
    fs::write(dir.path().join("a.txt"), "one\nfirst\n").unwrap();
    repo.stash_push("first").unwrap();
    let first = oids(&mut repo).remove(0);
    fs::write(dir.path().join("a.txt"), "one\nsecond\n").unwrap();
    repo.stash_push("second").unwrap();
    // `first` moved from stash@{0} to stash@{1}.

    repo.stash_drop(&first).unwrap();
    let left = repo.snapshot().unwrap().stashes;
    assert_eq!(left.len(), 1);
    assert!(left[0].message.contains("second"));
}

#[test]
fn an_unknown_oid_is_a_vanished_entry() {
    let dir = fixture("stash-gone");
    let mut repo = Repo::open(dir.path()).unwrap();
    let err = repo.stash_drop(&"0".repeat(40)).unwrap_err();
    assert!(
        matches!(&err, GitError::StashFailed(m) if m.contains("no longer exists")),
        "got {err:?}"
    );
}

#[test]
fn stash_diff_shows_the_tracked_hunk_and_the_untracked_file() {
    let dir = fixture("stash-diff");
    dirty(&dir);
    let mut repo = Repo::open(dir.path()).unwrap();
    repo.stash_push("").unwrap();
    let oid = oids(&mut repo).remove(0);

    let diff = repo.stash_diff(&oid, DiffOpts::default()).unwrap();
    assert!(diff.text.contains("+local"), "{}", diff.text);
    assert!(diff.text.contains("new.txt"), "{}", diff.text);
}

#[test]
fn stash_round_trips_on_a_detached_head() {
    let dir = fixture("stash-detached");
    git(dir.path(), &["checkout", "-q", "--detach"]);
    dirty(&dir);
    let mut repo = Repo::open(dir.path()).unwrap();

    repo.stash_push("detached").unwrap();
    let oid = oids(&mut repo).remove(0);
    assert_eq!(git(dir.path(), &["status", "--porcelain"]), "");

    assert_eq!(repo.stash_pop(&oid).unwrap(), StashOutcome::Done);
    assert!(oids(&mut repo).is_empty());
    assert!(dir.path().join("new.txt").exists());
}

#[test]
fn push_on_an_unborn_branch_fails_with_gits_own_message() {
    let dir = TempDir::new("stash-unborn");
    Repository::init(dir.path()).unwrap();
    configure_identity(dir.path());
    fs::write(dir.path().join("a.txt"), "one\n").unwrap();

    let repo = Repo::open(dir.path()).unwrap();
    let err = repo.stash_push("x").unwrap_err();
    assert!(
        matches!(&err, GitError::StashFailed(m) if m.contains("initial commit")),
        "got {err:?}"
    );
    assert!(dir.path().join("a.txt").exists(), "nothing was touched");
}

// ------------------------------------------------------- P4: keep-index, rename

#[test]
fn push_keeping_the_index_leaves_staged_changes_staged() {
    let dir = fixture("p4-keep-index");
    fs::write(dir.path().join("a.txt"), "one\nstaged\n").unwrap();
    git(dir.path(), &["add", "a.txt"]);
    fs::write(dir.path().join("a.txt"), "one\nstaged\nunstaged\n").unwrap();
    let mut repo = Repo::open(dir.path()).unwrap();
    repo.stash_push_keeping_index("kept").unwrap();

    assert_eq!(repo.snapshot().unwrap().stashes.len(), 1);
    let status = git(dir.path(), &["status", "--porcelain"]);
    assert_eq!(
        status, "M  a.txt",
        "the staged change is still staged, the rest went: {status}"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("a.txt")).unwrap(),
        "one\nstaged\n"
    );
}

#[test]
fn renaming_a_stash_keeps_its_commit_and_moves_it_to_the_top() {
    let dir = fixture("p4-rename");
    let mut repo = Repo::open(dir.path()).unwrap();
    fs::write(dir.path().join("a.txt"), "one\nfirst\n").unwrap();
    repo.stash_push("first").unwrap();
    fs::write(dir.path().join("a.txt"), "one\nsecond\n").unwrap();
    repo.stash_push("second").unwrap();
    let before = repo.snapshot().unwrap().stashes;
    assert_eq!(before.len(), 2);
    let first = before
        .iter()
        .find(|e| e.message.contains("first"))
        .unwrap()
        .clone();

    repo.stash_rename(&first.oid, "renamed").unwrap();

    let after = repo.snapshot().unwrap().stashes;
    assert_eq!(after.len(), 2, "one entry replaced, not added");
    assert!(after[0].message.contains("renamed"), "{after:?}");
    assert_eq!(after[0].oid, first.oid, "the same commit");
    assert!(after[1].message.contains("second"), "{after:?}");
    assert!(after.iter().all(|e| !e.message.contains("first")));
}

#[test]
fn renaming_the_top_stash_works_alone_and_above_another() {
    // `git stash store` of the commit already on top writes no reflog entry, so
    // the top entry has to be dropped before it is stored again.
    let dir = fixture("p4-rename-top");
    let mut repo = Repo::open(dir.path()).unwrap();
    fs::write(dir.path().join("a.txt"), "one\nfirst\n").unwrap();
    repo.stash_push("first").unwrap();
    let only = repo.snapshot().unwrap().stashes;
    repo.stash_rename(&only[0].oid, "renamed").unwrap();
    let after = repo.snapshot().unwrap().stashes;
    assert_eq!(after.len(), 1, "{after:?}");
    assert!(after[0].message.contains("renamed"), "{after:?}");
    assert_eq!(after[0].oid, only[0].oid, "the same commit");

    fs::write(dir.path().join("a.txt"), "one\nsecond\n").unwrap();
    repo.stash_push("second").unwrap();
    let top = repo.snapshot().unwrap().stashes[0].clone();
    repo.stash_rename(&top.oid, "again").unwrap();
    let after = repo.snapshot().unwrap().stashes;
    assert_eq!(after.len(), 2, "{after:?}");
    assert!(after[0].message.contains("again"), "{after:?}");
    assert!(after[1].message.contains("renamed"), "{after:?}");
}

#[test]
fn renaming_a_vanished_stash_is_an_error_and_changes_nothing() {
    let dir = fixture("p4-rename-gone");
    fs::write(dir.path().join("a.txt"), "one\nx\n").unwrap();
    let mut repo = Repo::open(dir.path()).unwrap();
    repo.stash_push("only").unwrap();
    let err = repo.stash_rename(&"0".repeat(40), "nope").unwrap_err();
    assert!(
        matches!(&err, GitError::StashFailed(m) if m.contains("no longer exists")),
        "{err:?}"
    );
    assert_eq!(repo.snapshot().unwrap().stashes.len(), 1);
}
