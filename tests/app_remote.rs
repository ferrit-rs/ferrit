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
//! milestones S1/S2): `start_remote_op` on a real two-repo fixture, drained
//! through a channel this test owns and fed back into `on_remote_done` —
//! no full `App::run()` loop, no real terminal, the one place ferrit's
//! tests wait on a real background thread. `set_event_sender` lets the
//! S2 tests drive the same channel through the real `f`/`p`/`P` keys
//! (`feed_key`) instead of calling `start_remote_op` directly.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use std::time::Duration;

use ferrit::app::App;
use ferrit::events::{AppEvent, RemoteOp};
use git2::{IndexAddOption, Repository, Signature};
use ratatui::crossterm::event::{KeyCode, KeyEvent};

fn char_key(c: char) -> KeyEvent {
    KeyEvent::from(KeyCode::Char(c))
}

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

    app.start_remote_op(RemoteOp::Fetch, None, tx);
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

    app.start_remote_op(RemoteOp::Fetch, None, tx.clone());
    assert_eq!(app.remote_busy_label(), Some("Fetching\u{2026}"));

    // A second, distinct op while the first is in flight: ignored
    // outright, `remote_busy` unchanged, no second thread's result ever
    // arrives on the channel.
    app.start_remote_op(RemoteOp::Pull, None, tx);
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
    app.start_remote_op(RemoteOp::Push, None, tx);
    wait_for_remote_done(&mut app, &rx);

    assert!(app.remote_busy_label().is_none());
    let lines = app.status_lines();
    assert!(
        lines.iter().any(|l| l.to_string().contains("no upstream")),
        "got: {lines:?}"
    );
}

#[test]
fn capital_p_with_no_remotes_opens_editable_upstream_prompt() {
    let dir = TempDir::new("app-remote-p-no-remotes");
    let repo = Repository::init(dir.path()).unwrap();
    configure_identity(dir.path());
    fs::write(dir.path().join("a.txt"), "one\n").unwrap();
    commit_all(&repo, "init");

    let mut app = App::open(dir.path()).unwrap();
    app.feed_key(char_key('P'));

    assert!(
        app.upstream_value()
            .is_some_and(|value| value.starts_with("origin ")),
        "prompt suggests origin even before remote exists"
    );
}

#[test]
fn capital_p_with_one_remote_and_no_upstream_pushes_with_dash_u() {
    let (origin, work) = two_repo_fixture("app-remote-p-one-remote");
    git(work.path(), &["checkout", "-q", "-b", "feature"]);
    fs::write(work.path().join("e.txt"), "feature\n").unwrap();
    let work_repo = Repository::open(work.path()).unwrap();
    commit_all(&work_repo, "feature work");

    let mut app = App::open(work.path()).unwrap();
    let (tx, rx) = mpsc::channel();
    app.set_event_sender(tx);
    app.feed_key(char_key('P'));
    assert!(app.upstream_value().is_some(), "prompt asks for upstream");
    app.feed_key(KeyEvent::from(KeyCode::Enter));
    assert_eq!(
        app.remote_busy_label(),
        Some("Pushing\u{2026}"),
        "confirming suggested upstream starts push"
    );

    wait_for_remote_done(&mut app, &rx);
    assert!(app.remote_busy_label().is_none());
    assert_eq!(
        git(work.path(), &["rev-parse", "--abbrev-ref", "feature@{u}"]),
        "origin/feature"
    );
    assert_eq!(
        git(origin.path(), &["rev-parse", "refs/heads/feature"]),
        git(work.path(), &["rev-parse", "HEAD"])
    );
}

#[test]
fn push_default_current_skips_remote_picker_and_sets_upstream() {
    let (origin, work) = two_repo_fixture("app-remote-push-default-current");
    git(
        work.path(),
        &["remote", "add", "backup", origin.path().to_str().unwrap()],
    );
    git(work.path(), &["config", "push.default", "current"]);
    git(work.path(), &["checkout", "-q", "-b", "feature"]);
    fs::write(work.path().join("feature.txt"), "feature\n").unwrap();
    let work_repo = Repository::open(work.path()).unwrap();
    commit_all(&work_repo, "feature work");

    let mut app = App::open(work.path()).unwrap();
    let (tx, rx) = mpsc::channel();
    app.set_event_sender(tx);
    app.feed_key(char_key('P'));

    assert_eq!(app.remote_busy_label(), Some("Pushing\u{2026}"));
    assert!(
        app.upstream_value().is_none(),
        "configured current push skips picker"
    );
    wait_for_remote_done(&mut app, &rx);
    assert_eq!(
        git(work.path(), &["rev-parse", "--abbrev-ref", "feature@{u}"]),
        "origin/feature"
    );
    assert_eq!(
        git(origin.path(), &["rev-parse", "refs/heads/feature"]),
        git(work.path(), &["rev-parse", "HEAD"])
    );
}

#[test]
fn push_of_behind_branch_requires_confirm_and_uses_force_with_lease() {
    let (origin, work) = two_repo_fixture("app-remote-force-lease");
    fs::write(work.path().join("local.txt"), "local\n").unwrap();
    let work_repo = Repository::open(work.path()).unwrap();
    commit_all(&work_repo, "local commit");

    let remote_work = TempDir::new("app-remote-force-lease-peer");
    git(
        Path::new("."),
        &[
            "clone",
            "-q",
            origin.path().to_str().unwrap(),
            remote_work.path().to_str().unwrap(),
        ],
    );
    configure_identity(remote_work.path());
    fs::write(remote_work.path().join("remote.txt"), "remote\n").unwrap();
    let remote_repo = Repository::open(remote_work.path()).unwrap();
    commit_all(&remote_repo, "remote commit");
    git(remote_work.path(), &["push", "origin", "base"]);
    git(work.path(), &["fetch", "origin"]);

    let mut app = App::open(work.path()).unwrap();
    let (tx, rx) = mpsc::channel();
    app.set_event_sender(tx);
    app.feed_key(char_key('P'));
    let prompt = app.confirm_message().expect("behind branch asks first");
    assert!(prompt.contains("--force-with-lease"), "{prompt}");
    assert!(
        app.remote_busy_label().is_none(),
        "no push before confirmation"
    );

    // Simulate a remote update after confirmation appeared. Lease must reject
    // overwrite because the remote-tracking ref no longer matches.
    fs::write(remote_work.path().join("remote.txt"), "remote changed\n").unwrap();
    commit_all(&remote_repo, "remote advances again");
    git(remote_work.path(), &["push", "origin", "base"]);
    let protected_tip = git(origin.path(), &["rev-parse", "refs/heads/base"]);

    app.feed_key(char_key('y'));
    assert_eq!(app.remote_busy_label(), Some("Pushing\u{2026}"));
    wait_for_remote_done(&mut app, &rx);
    let lines = app.status_lines();
    assert!(
        lines.iter().any(|line| {
            let text = line.to_string().to_lowercase();
            text.contains("stale info") || text.contains("rejected")
        }),
        "stale lease reports rejected push: {lines:?}"
    );
    assert_eq!(
        git(origin.path(), &["rev-parse", "refs/heads/base"]),
        protected_tip,
        "new remote commit stays intact"
    );
}

#[test]
fn push_progress_is_visible_inline_even_when_status_has_old_error() {
    let (_origin, work) = two_repo_fixture("app-remote-p-visible");
    let mut app = App::open(work.path()).unwrap();
    app.on_remote_done(RemoteOp::Push, Err("previous push failed".to_owned()));
    let (tx, rx) = mpsc::channel();
    app.set_event_sender(tx.clone());
    app.start_remote_op(RemoteOp::Push, None, tx);

    assert!(
        app.status_lines()
            .iter()
            .any(|line| line.to_string().contains("Pushing")),
        "status keeps push progress visible alongside old error"
    );
    assert!(
        app.branch_lines()
            .iter()
            .any(|line| line.to_string().contains("Pushing")),
        "checked-out branch shows LazyGit-style inline push status"
    );

    wait_for_remote_done(&mut app, &rx);
    assert!(
        !app.branch_lines()
            .iter()
            .any(|line| line.to_string().contains("Pushing")),
        "inline operation status clears on completion"
    );
}

#[test]
fn capital_p_with_two_remotes_prefills_editable_upstream() {
    let dir = TempDir::new("app-remote-p-two-remotes");
    let repo = Repository::init(dir.path()).unwrap();
    configure_identity(dir.path());
    fs::write(dir.path().join("a.txt"), "one\n").unwrap();
    commit_all(&repo, "init");
    git(
        dir.path(),
        &["remote", "add", "origin", "https://example.com/o.git"],
    );
    git(
        dir.path(),
        &["remote", "add", "upstream", "https://example.com/u.git"],
    );

    let mut app = App::open(dir.path()).unwrap();
    app.feed_key(char_key('P'));

    assert_eq!(app.upstream_value().as_deref(), Some("origin master"));

    app.feed_key(KeyEvent::from(KeyCode::Esc));
    assert!(app.upstream_value().is_none(), "Esc cancelled, no push");
}

#[test]
fn invalid_upstream_entry_keeps_prompt_open_and_shows_error() {
    let (_origin, work) = two_repo_fixture("app-remote-invalid-upstream");
    git(work.path(), &["checkout", "-q", "-b", "feature"]);
    let mut app = App::open(work.path()).unwrap();
    app.feed_key(char_key('P'));
    let initial = app.upstream_value().unwrap();
    for _ in initial.chars() {
        app.feed_key(KeyEvent::from(KeyCode::Backspace));
    }
    app.feed_key(KeyEvent::from(KeyCode::Enter));

    assert_eq!(app.upstream_value().as_deref(), Some(""));
    assert!(
        app.status_lines()
            .iter()
            .any(|line| line.to_string().contains("upstream must be")),
        "invalid input is reported while prompt stays open"
    );
}

#[test]
fn enter_on_upstream_prompt_pushes_to_the_suggested_remote() {
    let (origin, work) = two_repo_fixture("app-remote-p-pick-enter");
    // A second remote that does not exist on disk: prompt suggests origin.
    git(
        work.path(),
        &[
            "remote",
            "add",
            "zzz-unreachable",
            "https://example.invalid/x.git",
        ],
    );
    git(work.path(), &["checkout", "-q", "-b", "feature"]);
    fs::write(work.path().join("e.txt"), "feature\n").unwrap();
    let work_repo = Repository::open(work.path()).unwrap();
    commit_all(&work_repo, "feature work");

    let mut app = App::open(work.path()).unwrap();
    let (tx, rx) = mpsc::channel();
    app.set_event_sender(tx);
    app.feed_key(char_key('P'));
    assert!(app.upstream_value().is_some(), "upstream prompt opens");

    app.feed_key(KeyEvent::from(KeyCode::Enter));
    assert!(app.upstream_value().is_none(), "submitting closes prompt");
    assert_eq!(app.remote_busy_label(), Some("Pushing\u{2026}"));

    wait_for_remote_done(&mut app, &rx);
    assert_eq!(
        git(origin.path(), &["rev-parse", "refs/heads/feature"]),
        git(work.path(), &["rev-parse", "HEAD"]),
        "pushed to the suggested remote"
    );
}

#[test]
fn upstream_prompt_can_map_local_branch_to_another_remote_branch() {
    let (origin, work) = two_repo_fixture("app-remote-upstream-map");
    git(work.path(), &["checkout", "-q", "-b", "feature"]);
    fs::write(work.path().join("e.txt"), "feature\n").unwrap();
    let work_repo = Repository::open(work.path()).unwrap();
    commit_all(&work_repo, "feature work");

    let mut app = App::open(work.path()).unwrap();
    let (tx, rx) = mpsc::channel();
    app.set_event_sender(tx);
    app.feed_key(char_key('P'));
    for _ in 0.."feature".len() {
        app.feed_key(KeyEvent::from(KeyCode::Backspace));
    }
    for c in "review/one".chars() {
        app.feed_key(char_key(c));
    }
    assert_eq!(app.upstream_value().as_deref(), Some("origin review/one"));
    app.feed_key(KeyEvent::from(KeyCode::Enter));
    wait_for_remote_done(&mut app, &rx);

    assert_eq!(
        git(work.path(), &["rev-parse", "--abbrev-ref", "feature@{u}"]),
        "origin/review/one"
    );
    assert_eq!(
        git(origin.path(), &["rev-parse", "refs/heads/review/one"]),
        git(work.path(), &["rev-parse", "HEAD"])
    );
}

#[test]
fn app_mock_ignores_fetch_pull_push_with_no_thread_spawned() {
    let mut app = App::mock();
    app.feed_key(char_key('f'));
    app.feed_key(char_key('p'));
    app.feed_key(char_key('P'));
    assert!(
        app.remote_busy_label().is_none(),
        "no event_sender, no repo to reopen: nothing to be busy about"
    );
    assert!(app.upstream_value().is_none());
}
