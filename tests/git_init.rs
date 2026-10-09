#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "integration test scaffolding: a failed setup is the assertion"
)]
//! `Repo::init` (`docs/PLAN_16_START_WITHOUT_REPO.md`, W0): a folder with no
//! repository becomes one, and a folder that cannot be one says why.

use std::fs;
use std::path::PathBuf;

use ferrit::git::command_log::{CommandKind, recent};
use ferrit::git::error::GitError;
use ferrit::git::repo::Repo;

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = fs::canonicalize(std::env::temp_dir())
            .unwrap()
            .join(format!("ferrit-{tag}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn a_folder_with_no_repository_becomes_one_with_no_commits() {
    let dir = TempDir::new("init-ok");
    assert!(Repo::open(&dir.0).is_err(), "no repository yet");

    let mut repo = Repo::init(&dir.0).unwrap();
    assert!(dir.0.join(".git").is_dir());
    let snapshot = repo.snapshot().unwrap();
    assert!(snapshot.commits.is_empty(), "nothing committed");
    assert!(snapshot.files.is_empty());
    assert!(Repo::open(&dir.0).is_ok(), "it opens like any repository");
}

#[test]
fn it_is_recorded_in_the_command_log_as_a_write() {
    let dir = TempDir::new("init-log");
    Repo::init(&dir.0).unwrap();
    let found: Vec<_> = recent(usize::MAX, true)
        .into_iter()
        .filter(|r| r.argv == "git init" && r.exit == Some(0))
        .collect();
    assert!(!found.is_empty(), "git init is logged");
    assert!(found.iter().all(|r| r.kind == CommandKind::Write));
}

#[test]
fn a_folder_that_does_not_exist_says_why_and_creates_nothing() {
    let dir = TempDir::new("init-missing");
    let missing = dir.0.join("not-there");
    let err = Repo::init(&missing).err().unwrap();
    assert!(
        matches!(&err, GitError::InitFailed(message) if !message.is_empty()),
        "{err:?}"
    );
    assert!(!missing.exists());
}

#[test]
fn a_file_in_the_folder_is_left_alone() {
    let dir = TempDir::new("init-files");
    fs::write(dir.0.join("notes.txt"), "keep me").unwrap();
    let mut repo = Repo::init(&dir.0).unwrap();
    assert_eq!(
        fs::read_to_string(dir.0.join("notes.txt")).unwrap(),
        "keep me"
    );
    let snapshot = repo.snapshot().unwrap();
    assert_eq!(
        snapshot.files.len(),
        1,
        "it shows as untracked, nothing staged"
    );
}
