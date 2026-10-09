//! Churn.

use std::sync::atomic::AtomicBool;

use crate::support::{NOW, TempDir, git, init, project, stats_at};
use ferrit::domain::git::command_log;
use ferrit::domain::git::stats::{StatsOptions, Window};
use ferrit::infra::git::Repo;

#[test]
fn lines_are_summed_in_total_and_per_author() {
    let tmp = project();
    let stats = stats_at(tmp.path(), Window::Days90);
    let lines = stats.totals.lines.unwrap();
    assert_eq!(
        (lines.added, lines.removed),
        (10, 0),
        "lockfile lines count as git counts them; the stash and the other branches are not main"
    );
    let per_author: Vec<_> = stats
        .authors
        .iter()
        .map(|a| (a.name.as_str(), a.added.unwrap(), a.removed.unwrap()))
        .collect();
    assert_eq!(per_author, [("Richard", 4, 0), ("Max Wells", 6, 0)]);
}

#[test]
fn the_churn_reads_the_window_only() {
    let tmp = project();
    let stats = stats_at(tmp.path(), Window::Days7);
    let hot = stats.hot_files.unwrap();
    assert_eq!(hot.commits, 2);
    let paths: Vec<_> = hot.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, ["README.md", "e.txt"], "f.txt is not in HEAD");
    let all = stats_at(tmp.path(), Window::All).totals.lines.unwrap();
    assert!(
        all.added > stats.totals.lines.unwrap().added,
        "the whole history has more lines"
    );
}

#[test]
fn the_numstat_cap_keeps_the_newest_commits_and_says_sampled() {
    let tmp = project();
    let repo = Repo::open(tmp.path()).unwrap();
    let opts = StatsOptions {
        now: NOW,
        numstat_cap: 3,
        ..StatsOptions::default()
    };
    let stats = repo
        .stats_with(Window::All, &opts, &AtomicBool::new(false))
        .unwrap();
    assert!(stats.sampled);
    assert_eq!(stats.totals.commits, 7, "the walk has its own cap");
    let hot = stats.hot_files.unwrap();
    assert_eq!(hot.commits, 3, "c5, c4 and d1");
    let lines = stats.totals.lines.unwrap();
    assert_eq!((lines.added, lines.removed), (4, 0));

    let exact = StatsOptions {
        now: NOW,
        numstat_cap: 9,
        ..StatsOptions::default()
    };
    let stats = repo
        .stats_with(Window::All, &exact, &AtomicBool::new(false))
        .unwrap();
    assert!(!stats.sampled, "9 non-merge commits fit a cap of 9");
}

#[test]
fn a_failing_git_log_leaves_the_churn_absent_and_the_rest_filled() {
    let tmp = project();
    git(tmp.path(), &["config", "log.date", "bogus"]);
    let stats = stats_at(tmp.path(), Window::Days90);
    assert!(stats.hot_files.is_none());
    assert!(stats.totals.lines.is_none());
    assert!(
        stats
            .authors
            .iter()
            .all(|a| a.added.is_none() && a.removed.is_none())
    );
    assert_eq!(stats.totals.commits, 6);
    assert_eq!(stats.authors.len(), 2);
}

#[test]
fn the_numstat_command_lands_in_the_command_log() {
    let tmp = project();
    let _ = stats_at(tmp.path(), Window::Days90);
    let logged = command_log::recent(usize::MAX, true)
        .into_iter()
        .any(|entry| {
            entry.argv.contains("--numstat")
                && entry.argv.contains("--no-merges")
                && entry.exit == Some(0)
        });
    assert!(logged);
}

#[test]
fn an_empty_repository_has_empty_churn() {
    let tmp = TempDir::new("stats-empty-churn");
    init(tmp.path(), "main");
    let stats = stats_at(tmp.path(), Window::All);
    let hot = stats.hot_files.unwrap();
    assert!(hot.files.is_empty() && hot.hidden.is_empty());
    assert_eq!(hot.commits, 0);
    assert_eq!(stats.totals.lines.map(|l| l.added), Some(0));
}
