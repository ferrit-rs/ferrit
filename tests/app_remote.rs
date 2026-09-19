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
//! `App`-level threading coverage for `f`/`p`/`P` (`docs/PLAN_9_REMOTE.md`
//! milestone S1): `start_remote_op` on a real two-repo fixture, drained
//! through a channel this test owns and fed back into `on_remote_done` —
//! no full `App::run()` loop, no real terminal, the one place ferrit's
//! tests wait on a real background thread.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use std::time::Duration;

use ferrit::app::App;
use ferrit::events::{AppEvent, RemoteOp};
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

/// Same shape as `tests/git_remote.rs`'s fixture: `origin` (non-bare,
/// `receive.denyCurrentBranch=ignore`) cloned into `work`, `base` tracking
/// `origin/base`.
fn two_repo_fixture(tag: &str) -> (TempDir, TempDir) {
    let origin = TempDir::new(&format!("{tag}-origin"));
    let origin_repo = Repository::init(origin.path()).unwrap();
    configure_identity(origin.path());
    fs::write(origin.path().join("a.txt"), "one\n").unwrap();
    commit_all(&origin_repo, "init");
    git(origin.path(), &["branch", "-m", "base"]);
    git(
        origin.path(),
        &["config", "receive.denyCurrentBranch", "ignore"],
    );

    let work = TempDir::new(&format!("{tag}-work"));
    git(
        Path::new("."),
        &[
            "clone",
            "-q",
            origin.path().to_str().unwrap(),
            work.path().to_str().unwrap(),
        ],
    );
    configure_identity(work.path());
    (origin, work)
}

/// Block for `AppEvent::RemoteDone` on `rx` (10s: generous for a same-
/// machine path-remote subprocess, never expected to actually wait that
/// long) and feed it into `app.on_remote_done`, the same hop
/// `App::run`'s own match arm makes.
fn wait_for_remote_done(app: &mut App, rx: &mpsc::Receiver<AppEvent>) {
    match rx.recv_timeout(Duration::from_secs(10)) {
        Ok(AppEvent::RemoteDone { op, message }) => app.on_remote_done(op, message),
        other => panic!("expected RemoteDone, got {other:?}"),
    }
}

#[test]
fn fetch_clears_busy_and_refreshes_ahead_behind() {
    let (origin, work) = two_repo_fixture("app-remote-fetch");
    fs::write(origin.path().join("a.txt"), "one\ntwo\n").unwrap();
    let origin_repo = Repository::open(origin.path()).unwrap();
    commit_all(&origin_repo, "origin advances");

    let mut app = App::open(work.path()).unwrap();
    let (tx, rx) = mpsc::channel();

    app.start_remote_op(RemoteOp::Fetch, tx);
    assert_eq!(app.remote_busy_label(), Some("Fetching\u{2026}"));

    wait_for_remote_done(&mut app, &rx);
    assert!(app.remote_busy_label().is_none());
    assert!(
        app.status_lines()
            .iter()
            .any(|l| l.to_string().to_lowercase().contains("fetch")),
        "a success line shows: {:?}",
        app.status_lines()
    );

    // `git fetch` alone never moves the local branch, only the
    // remote-tracking ref (and the behind count `refresh()` recomputes
    // from it) — the point of the test is that `refresh()` actually ran.
    let status = git(work.path(), &["status", "-sb"]);
    assert!(status.contains("behind 1"), "got: {status}");
}

#[test]
fn a_second_start_while_one_is_running_is_ignored() {
    let (origin, work) = two_repo_fixture("app-remote-single-flight");
    fs::write(origin.path().join("a.txt"), "one\ntwo\n").unwrap();
    let origin_repo = Repository::open(origin.path()).unwrap();
    commit_all(&origin_repo, "origin advances");

    let mut app = App::open(work.path()).unwrap();
    let (tx, rx) = mpsc::channel();

    app.start_remote_op(RemoteOp::Fetch, tx.clone());
    assert_eq!(app.remote_busy_label(), Some("Fetching\u{2026}"));

    // A second, distinct op while the first is in flight: ignored
    // outright, `remote_busy` unchanged, no second thread's result ever
    // arrives on the channel.
    app.start_remote_op(RemoteOp::Pull, tx);
    assert_eq!(
        app.remote_busy_label(),
        Some("Fetching\u{2026}"),
        "still the first op, not silently swapped for the second"
    );

    wait_for_remote_done(&mut app, &rx);
    assert!(app.remote_busy_label().is_none());
    assert!(rx.try_recv().is_err(), "no second RemoteDone ever arrived");
}

#[test]
fn a_failure_sets_last_error_not_a_status_note() {
    let (_origin, work) = two_repo_fixture("app-remote-fail");
    git(work.path(), &["checkout", "-q", "-b", "orphan"]);
    fs::write(work.path().join("d.txt"), "orphan\n").unwrap();
    let work_repo = Repository::open(work.path()).unwrap();
    commit_all(&work_repo, "orphan work");

    let mut app = App::open(work.path()).unwrap();
    let (tx, rx) = mpsc::channel();
    app.start_remote_op(RemoteOp::Push, tx);
    wait_for_remote_done(&mut app, &rx);

    assert!(app.remote_busy_label().is_none());
    let lines = app.status_lines();
    assert!(
        lines.iter().any(|l| l.to_string().contains("no upstream")),
        "got: {lines:?}"
    );
}
