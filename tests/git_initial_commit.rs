#![allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "integration test scaffolding: a failed setup is the assertion"
)]
//! `Repo::initial_commit` (`docs/PLAN_15_CREATE_REMOTE.md`): the first commit of a
//! repository with none, an empty `README.md`, and nothing else.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use ferrit::infra::git::Repo;

struct TempDir(PathBuf);

impl TempDir {
    /// A repository with a local identity, so no test depends on the user's.
    fn repo(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = fs::canonicalize(std::env::temp_dir())
            .unwrap()
            .join(format!("ferrit-{tag}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        let dir = Self(path);
        dir.git(&["init", "-q", "."]);
        dir.git(&["config", "user.name", "Local Name"]);
        dir.git(&["config", "user.email", "local@example.com"]);
        dir
    }

    fn git(&self, args: &[&str]) -> String {
        run(&self.0, args)
    }

    fn open(&self) -> Repo {
        Repo::open(&self.0).unwrap()
    }
}

fn run(dir: &Path, args: &[&str]) -> String {
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
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn an_empty_repository_gets_one_commit_holding_an_empty_readme() {
    let dir = TempDir::repo("initial-ok");
    assert!(dir.open().initial_commit(None).unwrap());

    assert_eq!(dir.git(&["rev-list", "--count", "HEAD"]), "1");
    assert_eq!(dir.git(&["log", "-1", "--format=%s"]), "Initial commit");
    assert_eq!(
        dir.git(&["log", "-1", "--format=%b"]),
        "This initial commit and the remote repository were created by Ferrit.",
        "the body says who made it"
    );
    assert_eq!(
        dir.git(&["log", "-1", "--format=%B"]).trim_end(),
        ferrit::domain::git::commit::INITIAL_MESSAGE.trim_end(),
        "always the same message"
    );
    assert_eq!(
        dir.git(&["ls-tree", "-r", "--name-only", "HEAD"]),
        "README.md"
    );
    assert_eq!(
        dir.git(&["cat-file", "-s", "HEAD:README.md"]),
        "0",
        "the file is empty"
    );
    assert_eq!(dir.git(&["status", "--porcelain"]), "", "nothing left over");
}

#[test]
fn the_other_files_of_the_folder_stay_out_of_it() {
    let dir = TempDir::repo("initial-others");
    fs::write(dir.0.join("notes.txt"), "mine").unwrap();
    fs::write(dir.0.join("staged.txt"), "staged on purpose").unwrap();
    dir.git(&["add", "staged.txt"]);
    assert!(dir.open().initial_commit(None).unwrap());

    assert_eq!(
        dir.git(&["ls-tree", "-r", "--name-only", "HEAD"]),
        "README.md"
    );
    let status = dir.git(&["status", "--porcelain"]);
    assert!(status.contains("A  staged.txt"), "still staged: {status}");
    assert!(status.contains("?? notes.txt"), "still untracked: {status}");
}

#[test]
fn an_existing_readme_is_committed_as_it_is() {
    let dir = TempDir::repo("initial-readme");
    fs::write(dir.0.join("README.md"), "# my project\n").unwrap();
    assert!(dir.open().initial_commit(None).unwrap());
    assert_eq!(dir.git(&["show", "HEAD:README.md"]), "# my project");
    assert_eq!(
        fs::read_to_string(dir.0.join("README.md")).unwrap(),
        "# my project\n"
    );
}

#[test]
fn a_repository_that_already_has_a_commit_is_left_alone() {
    let dir = TempDir::repo("initial-has");
    fs::write(dir.0.join("a.txt"), "a").unwrap();
    dir.git(&["add", "a.txt"]);
    dir.git(&["commit", "-q", "-m", "first"]);
    assert!(!dir.open().initial_commit(None).unwrap());
    assert_eq!(dir.git(&["rev-list", "--count", "HEAD"]), "1");
    assert!(!dir.0.join("README.md").exists(), "no file is added");
}

#[test]
fn asking_twice_commits_once() {
    let dir = TempDir::repo("initial-twice");
    let repo = dir.open();
    assert!(repo.initial_commit(None).unwrap());
    assert!(!repo.initial_commit(None).unwrap());
    assert_eq!(dir.git(&["rev-list", "--count", "HEAD"]), "1");
}

#[test]
fn the_chosen_author_signs_it_and_the_log_shows_the_commit() {
    let dir = TempDir::repo("initial-author");
    dir.open()
        .initial_commit(Some("Chosen One <chosen@example.com>".to_owned()))
        .unwrap();
    assert_eq!(
        dir.git(&["log", "-1", "--format=%an <%ae>"]),
        "Chosen One <chosen@example.com>"
    );

    let logged = ferrit::domain::git::command_log::recent(usize::MAX, true)
        .iter()
        .any(|r| r.argv.contains("--only -- README.md") && r.exit == Some(0));
    assert!(logged, "the commit is in the command log");
}
