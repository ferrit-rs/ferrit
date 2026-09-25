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
//! The command log (`docs/PLAN_12_POLISH.md` P0): every `git` subprocess is
//! recorded through `exec`, reads are hidden by default, credentials are
//! redacted, and the ring keeps the last 200.
//!
//! The log is process-wide and these tests run in parallel, so each one looks
//! for entries carrying a name only it uses, never for "the last entry".

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use ferrit::domain::git::Repo;
use ferrit::domain::git::command_log::{CommandKind, CommandRecord, recent};
use ferrit::domain::git::diff::{DiffOpts, DiffSide};
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

fn fixture(tag: &str) -> TempDir {
    let dir = TempDir::new(tag);
    let repo = Repository::init(dir.path()).unwrap();
    configure_identity(dir.path());
    fs::write(dir.path().join("a.txt"), "one\n").unwrap();
    commit_all(&repo, "init");
    dir
}

/// Entries whose command line contains `needle`, oldest first.
fn logged(needle: &str) -> Vec<CommandRecord> {
    recent(usize::MAX, true)
        .into_iter()
        .filter(|entry| entry.argv.contains(needle))
        .collect()
}

#[test]
fn a_write_is_recorded_without_the_workdir_flag() {
    let dir = fixture("exec-write");
    let repo = Repo::open(dir.path()).unwrap();
    repo.create_branch("exec-write-branch").unwrap();

    let entries = logged("exec-write-branch");
    assert_eq!(entries.len(), 1, "{entries:?}");
    assert_eq!(entries[0].argv, "git checkout -b exec-write-branch");
    assert_eq!(entries[0].kind, CommandKind::Write);
    assert_eq!(entries[0].exit, Some(0));
    assert!(!entries[0].failed());
}

#[test]
fn a_failing_command_is_recorded_with_its_exit_code() {
    let dir = fixture("exec-fail");
    let repo = Repo::open(dir.path()).unwrap();
    assert!(repo.checkout("exec-fail-no-such-branch").is_err());

    let entries = logged("exec-fail-no-such-branch");
    assert_eq!(entries.len(), 1, "{entries:?}");
    assert!(entries[0].failed());
    assert_ne!(entries[0].exit, Some(0));
    assert!(entries[0].exit.is_some(), "it ran, it just failed");
}

#[test]
fn reads_are_hidden_unless_asked_for() {
    let dir = fixture("exec-read");
    fs::write(dir.path().join("exec-read-file.txt"), "x\n").unwrap();
    let repo = Repo::open(dir.path()).unwrap();
    repo.file_diff(
        Path::new("exec-read-file.txt"),
        DiffSide::Worktree,
        DiffOpts::default(),
    )
    .unwrap();

    let entries = logged("exec-read-file.txt");
    assert!(!entries.is_empty(), "the diff ran through exec");
    assert!(entries.iter().all(|e| e.kind == CommandKind::Read));
    let shown = recent(usize::MAX, false);
    assert!(
        shown.iter().all(|e| !e.argv.contains("exec-read-file.txt")),
        "reads stay out of the default view"
    );
}

#[test]
fn credentials_in_a_url_never_reach_the_log() {
    let dir = fixture("exec-redact");
    let repo = Repo::open(dir.path()).unwrap();
    let _ = repo.checkout("https://me:hunter2@exec-redact.example/o.git");

    let entries = logged("exec-redact.example");
    assert_eq!(entries.len(), 1, "{entries:?}");
    assert!(entries[0].argv.contains("me:***@"), "{}", entries[0].argv);
    assert!(!entries[0].argv.contains("hunter2"), "{}", entries[0].argv);
}

#[test]
fn a_stdin_fed_command_is_recorded_too() {
    // `commit` pipes the message through stdin and manages the child itself.
    let dir = fixture("exec-stdin");
    fs::write(dir.path().join("a.txt"), "one\ntwo\n").unwrap();
    git(dir.path(), &["add", "a.txt"]);
    let repo = Repo::open(dir.path()).unwrap();
    repo.commit(
        &ferrit::domain::git::commit::CommitKind::Normal,
        "exec-stdin message",
        ferrit::domain::git::commit::CommitOpts::default(),
    )
    .unwrap();

    let commits: Vec<_> = recent(usize::MAX, true)
        .into_iter()
        .filter(|e| e.argv.starts_with("git commit"))
        .collect();
    assert!(!commits.is_empty(), "the commit subprocess was recorded");
    assert!(commits.iter().all(|e| e.kind == CommandKind::Write));
    assert!(commits.iter().any(|e| e.exit == Some(0)));
}

#[test]
fn a_network_command_is_recorded_too() {
    // `fetch` is spawned with process-group handling and a polling loop.
    let dir = fixture("exec-fetch");
    let repo = Repo::open(dir.path()).unwrap();
    let _ = repo.fetch(Some("exec-fetch-no-such-remote"));

    let entries = logged("exec-fetch-no-such-remote");
    assert!(!entries.is_empty(), "the fetch subprocess was recorded");
    assert!(entries[0].argv.starts_with("git fetch"), "{entries:?}");
}

#[test]
fn the_log_keeps_only_the_newest_two_hundred() {
    let dir = fixture("exec-ring");
    let repo = Repo::open(dir.path()).unwrap();
    for i in 0..250 {
        let _ = repo.checkout(&format!("exec-ring-{i:03}"));
    }

    let all = recent(usize::MAX, true);
    assert!(all.len() <= 200, "{} entries", all.len());
    assert!(logged("exec-ring-000").is_empty(), "the oldest was dropped");
    assert_eq!(logged("exec-ring-249").len(), 1, "the newest is kept");
}

#[test]
fn recent_returns_the_newest_entries_oldest_first() {
    let dir = fixture("exec-order");
    let repo = Repo::open(dir.path()).unwrap();
    let _ = repo.checkout("exec-order-a");
    let _ = repo.checkout("exec-order-b");

    let ours: Vec<String> = recent(usize::MAX, true)
        .into_iter()
        .filter(|e| e.argv.contains("exec-order-"))
        .map(|e| e.argv)
        .collect();
    assert_eq!(
        ours,
        vec!["git checkout exec-order-a", "git checkout exec-order-b"]
    );
    let two = recent(2, true);
    assert!(two.len() <= 2);
}

/// Nothing may build a `git` command or drive a child without `exec`, or the
/// log would silently miss it.
#[test]
fn every_git_subprocess_goes_through_exec() {
    fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                rust_files(&path, out);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                out.push(path);
            }
        }
    }
    let mut files = Vec::new();
    rust_files(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("src"),
        &mut files,
    );

    for file in files {
        let text = fs::read_to_string(&file).unwrap();
        // The replay harness builds and inspects fixture repositories; it does
        // not operate one for the user, so its `git` calls are not the
        // command log's business (`src/replay/mod.rs`). Nothing else is exempt.
        if file.starts_with(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/replay")) {
            continue;
        }
        let is_exec = file.ends_with("domain/git/exec.rs");
        if !is_exec {
            assert!(
                !text.contains("Command::new(\"git\")"),
                "{} builds a git command outside exec::git",
                file.display()
            );
        }
        if file.starts_with(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/domain/git"))
            && !is_exec
            && !file.ends_with("command_log.rs")
            && (text.contains(".output()") || text.contains(".spawn()"))
        {
            assert!(
                text.contains("exec::"),
                "{} runs a child without exec::output or exec::track",
                file.display()
            );
        }
    }
}
