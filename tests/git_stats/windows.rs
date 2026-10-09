//! Windows and the main branch.

use std::sync::atomic::AtomicBool;

use crate::support::{NOW, RICHARD, TempDir, ago, commit, git, init, project, stats_at};
use ferrit::domain::git::error::GitError;
use ferrit::domain::git::stats::{StatsOptions, Window};
use ferrit::infra::git::Repo;

#[test]
fn an_empty_repository_reads_zero_without_failing() {
    let tmp = TempDir::new("stats-empty");
    init(tmp.path(), "main");
    let stats = stats_at(tmp.path(), Window::All);
    assert_eq!(stats.totals.commits, 0);
    assert_eq!(stats.totals.first_commit, None);
    assert!(stats.series.is_empty() && stats.authors.is_empty() && stats.kinds.is_empty());
    assert!(stats.branches.is_empty());
    assert_eq!(stats.main_branch, None);
    assert_eq!(stats.since_tag, None);
}

#[test]
fn a_window_without_commits_is_empty_but_the_totals_keep_the_history() {
    let tmp = TempDir::new("stats-quiet");
    init(tmp.path(), "main");
    commit(
        tmp.path(),
        RICHARD,
        ago(400),
        "feat: old",
        &[("a.txt", "a\n")],
    );
    let stats = stats_at(tmp.path(), Window::Days30);
    assert_eq!(stats.totals.commits, 0);
    assert!(stats.series.is_empty() && stats.authors.is_empty());
    assert_eq!(stats.totals.first_commit, Some(ago(400)));
}

#[test]
fn a_detached_head_changes_nothing_about_what_is_counted_and_no_branch_is_current() {
    let tmp = TempDir::new("stats-detached");
    let dir = tmp.path();
    init(dir, "main");
    commit(dir, RICHARD, ago(5), "feat: one", &[("a.txt", "a\n")]);
    commit(dir, RICHARD, ago(4), "feat: two", &[("a.txt", "b\n")]);
    git(dir, &["checkout", "-q", "--detach", "HEAD~1"]);
    commit(dir, RICHARD, ago(1), "fix: loose", &[("b.txt", "b\n")]);
    let stats = stats_at(dir, Window::All);
    assert_eq!(
        stats.totals.commits, 2,
        "the commits on main; the loose commit on the detached HEAD is not on it"
    );
    assert_eq!(stats.main_branch.as_deref(), Some("main"));
    assert!(stats.branches.iter().all(|b| !b.current));
}

#[test]
fn master_is_the_main_branch_when_there_is_no_main() {
    let tmp = TempDir::new("stats-master");
    init(tmp.path(), "master");
    commit(
        tmp.path(),
        RICHARD,
        ago(2),
        "feat: one",
        &[("a.txt", "a\n")],
    );
    assert_eq!(
        stats_at(tmp.path(), Window::All).main_branch.as_deref(),
        Some("master")
    );
}

#[test]
fn init_default_branch_wins_over_main_and_master() {
    let tmp = TempDir::new("stats-default");
    let dir = tmp.path();
    init(dir, "main");
    commit(dir, RICHARD, ago(3), "feat: one", &[("a.txt", "a\n")]);
    git(dir, &["branch", "trunk"]);
    git(dir, &["config", "init.defaultBranch", "trunk"]);
    assert_eq!(
        stats_at(dir, Window::All).main_branch.as_deref(),
        Some("trunk")
    );
}

#[test]
fn with_only_the_head_branch_there_is_no_main_and_nothing_is_stale_or_merged() {
    let tmp = TempDir::new("stats-trunk");
    let dir = tmp.path();
    init(dir, "work");
    commit(dir, RICHARD, ago(300), "feat: one", &[("a.txt", "a\n")]);
    git(dir, &["branch", "other"]);
    let stats = stats_at(dir, Window::All);
    assert_eq!(stats.main_branch, None);
    assert!(
        stats
            .branches
            .iter()
            .all(|b| b.vs_main.is_none() && !b.stale)
    );
    assert_eq!(stats.branches[0].name, "work");
}

#[test]
fn a_shallow_clone_says_so_and_finds_the_main_branch_through_origin_head() {
    let tmp = project();
    let clone = TempDir::new("stats-shallow");
    let url = format!("file://{}", tmp.path().display());
    git(
        clone.path(),
        &[
            "clone",
            "-q",
            "--depth",
            "1",
            "--no-single-branch",
            &url,
            "c",
        ],
    );
    let dir = clone.path().join("c");
    let stats = stats_at(&dir, Window::All);
    assert!(stats.shallow);
    assert_eq!(stats.totals.remotes, 1);
    assert!(stats.totals.remote_branches >= 1);
    assert_eq!(stats.main_branch.as_deref(), Some("main"));
    assert!(
        stats.totals.first_commit.unwrap() > ago(100),
        "the cut hides the first commit"
    );
    assert!(
        stats.totals.commits < 10,
        "history before the cut is missing"
    );
}

#[test]
fn the_walk_cap_keeps_the_newest_commits_and_says_sampled() {
    let tmp = project();
    let repo = Repo::open(tmp.path()).unwrap();
    let opts = StatsOptions {
        now: NOW,
        walk_cap: 3,
        ..StatsOptions::default()
    };
    let stats = repo
        .stats_with(Window::All, &opts, &AtomicBool::new(false))
        .unwrap();
    assert!(stats.sampled);
    assert_eq!(stats.totals.commits, 3);
    let uncapped = stats_at(tmp.path(), Window::All);
    assert!(!uncapped.sampled);
}

#[test]
fn a_set_cancel_flag_stops_the_walk() {
    let tmp = project();
    let repo = Repo::open(tmp.path()).unwrap();
    let result = repo.stats(Window::All, &AtomicBool::new(true));
    assert!(matches!(result, Err(GitError::Cancelled)), "{result:?}");
}
