//! Totals and buckets.

use crate::support::{NOW, RICHARD, TempDir, ago, buckets, commit, init, project, stats_at};
use ferrit_domain::stats::Window;
use ferrit_domain::stats::series::Granularity;

#[test]
fn totals_count_the_window_and_the_whole_repository() {
    let tmp = project();
    let stats = stats_at(tmp.path(), Window::Days90);
    let t = &stats.totals;
    assert_eq!(
        t.commits, 6,
        "the commits on main: c1 (100 days) is outside the window, o1, f1 and f2 are not on main"
    );
    assert_eq!(t.authors, 2);
    assert_eq!(t.local_branches, 5);
    assert_eq!((t.remote_branches, t.remotes), (0, 0));
    assert_eq!(t.tags, 1);
    assert_eq!(t.stashes, 1);
    assert_eq!(
        t.first_commit,
        Some(ago(100)),
        "the first commit ignores the window"
    );
    assert_eq!(t.last_commit, Some(ago(2)));
    assert_eq!(stats.window, Window::Days90);
    assert!(!stats.sampled && !stats.shallow);

    assert_eq!(stats_at(tmp.path(), Window::All).totals.commits, 7);
    assert_eq!(stats_at(tmp.path(), Window::Year).totals.commits, 7);
    assert_eq!(stats_at(tmp.path(), Window::Days30).totals.commits, 5);
    assert_eq!(stats_at(tmp.path(), Window::Days7).totals.commits, 2);
}

#[test]
fn a_seven_day_window_is_daily_and_ends_today() {
    let tmp = project();
    let stats = stats_at(tmp.path(), Window::Days7);
    assert_eq!(stats.granularity, Granularity::Day);
    // 09-22 .. 09-29: c4 (26th), c5 (27th). f1 and f2 are not on main.
    assert_eq!(buckets(&stats), [0, 0, 0, 0, 1, 1, 0, 0]);
    assert_eq!(stats.series.first().unwrap().start, ago(7) - 12 * 3600);
    assert_eq!(stats.series.last().unwrap().start, NOW - 12 * 3600);
}

#[test]
fn thirty_days_are_daily_and_ninety_are_weekly_iso_weeks() {
    let tmp = project();
    let month = stats_at(tmp.path(), Window::Days30);
    assert_eq!(month.granularity, Granularity::Day);
    assert_eq!(month.series.len(), 31);
    assert_eq!(month.series.iter().map(|b| b.commits).sum::<usize>(), 5);

    let quarter = stats_at(tmp.path(), Window::Days90);
    assert_eq!(quarter.granularity, Granularity::Week);
    // Weeks start on Monday: 06-29 .. 09-28, 14 of them.
    assert_eq!(quarter.series.len(), 14);
    assert_eq!(quarter.series.first().unwrap().start, ago(92) - 12 * 3600);
    assert_eq!(quarter.series.last().unwrap().start, ago(1) - 12 * 3600);
    let all = buckets(&quarter);
    assert_eq!(all.iter().sum::<usize>(), 6);
    // Week of 09-14: c3, d1. Week of 09-21: M, c4, c5. This week: none yet.
    assert_eq!(&all[11..], [2, 3, 0]);
}

#[test]
fn the_axis_is_clipped_to_the_age_of_the_repository() {
    let tmp = TempDir::new("stats-young");
    init(tmp.path(), "main");
    commit(
        tmp.path(),
        RICHARD,
        ago(3),
        "feat: one",
        &[("a.txt", "a\n")],
    );
    commit(
        tmp.path(),
        RICHARD,
        ago(1),
        "feat: two",
        &[("a.txt", "b\n")],
    );
    let stats = stats_at(tmp.path(), Window::Days90);
    assert_eq!(stats.granularity, Granularity::Day);
    assert_eq!(buckets(&stats), [1, 0, 1, 0], "4 days, not 90");
}

#[test]
fn a_very_old_history_is_monthly() {
    let tmp = TempDir::new("stats-old");
    init(tmp.path(), "main");
    commit(
        tmp.path(),
        RICHARD,
        ago(5 * 365),
        "feat: long ago",
        &[("a.txt", "a\n")],
    );
    commit(
        tmp.path(),
        RICHARD,
        ago(1),
        "feat: now",
        &[("a.txt", "b\n")],
    );
    let stats = stats_at(tmp.path(), Window::All);
    assert_eq!(stats.granularity, Granularity::Month);
    assert_eq!(stats.series.len(), 61);
    assert_eq!(stats.series.iter().map(|b| b.commits).sum::<usize>(), 2);
    assert_eq!(stats.series.first().unwrap().commits, 1);
    assert_eq!(stats.series.last().unwrap().commits, 1);
}
