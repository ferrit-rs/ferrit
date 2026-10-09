//! Edge cases.

use crate::support::{DAY, NOW, author, render, stats, view};
use ferrit::git::stats::kind::Kind;
use ferrit::git::stats::series::{Bucket, Granularity};
use ferrit::git::stats::share::Share;
use ferrit::git::stats::{FileStat, HotFiles, KindStat, Lines, Totals, Window};

#[test]
fn while_computing_the_screen_says_so() {
    let mut v = view(None);
    v.computing = true;
    let out = render(&v, 120, 30);
    assert!(
        out.contains("computing…") && out.contains("Dashboard"),
        "{out}"
    );
    assert!(!out.contains("Activity"));
    let out = render(&v, 50, 12);
    assert!(out.contains("computing…"));
}

#[test]
fn a_stats_error_is_one_line() {
    let mut v = view(None);
    v.error = Some("could not walk the refs");
    let out = render(&v, 120, 30);
    assert!(
        out.contains("could not read the statistics: could not walk the refs"),
        "{out}"
    );
    // Next to stats, it is a notice under the totals and the charts stay.
    let s = stats();
    let mut v = view(Some(&s));
    v.error = Some("boom");
    let out = render(&v, 120, 42);
    assert!(out.contains("stats error: boom") && out.contains("Contributors"));
}

#[test]
fn churn_columns_compute_then_read_n_a_when_missing() {
    let mut s = stats();
    s.hot_files = None;
    s.totals.lines = None;
    let mut v = view(Some(&s));
    v.churn_pending = true;
    let out = render(&v, 120, 42);
    assert_eq!(
        out.matches("computing…").count(),
        2,
        "hot files and lines: {out}"
    );
    assert!(out.contains("Contributors"), "the rest still renders");
    v.churn_pending = false;
    let out = render(&v, 120, 42);
    assert!(!out.contains("computing…"));
    assert!(out.matches("n/a").count() >= 2, "{out}");
}

#[test]
fn an_empty_repository_says_no_commits_yet() {
    let mut s = stats();
    s.totals = Totals {
        commits: 0,
        authors: 0,
        local_branches: 0,
        remote_branches: 0,
        remotes: 0,
        tags: 0,
        stashes: 0,
        first_commit: None,
        last_commit: None,
        lines: None,
    };
    s.series.clear();
    s.daily.clear();
    s.authors.clear();
    s.kinds.clear();
    s.hot_files = None;
    s.branches.clear();
    s.since_tag = None;
    let out = render(&view(Some(&s)), 120, 30);
    assert!(
        out.contains("no commits yet") && out.contains("commits"),
        "{out}"
    );
    assert!(!out.contains("Activity") && out.contains("2 changed"));
}

#[test]
fn a_window_without_commits_keeps_the_totals_and_empties_the_charts() {
    let mut s = stats();
    s.window = Window::Days7;
    s.totals.commits = 0;
    s.totals.authors = 0;
    s.series.clear();
    s.authors.clear();
    s.kinds.clear();
    s.hot_files = Some(HotFiles {
        files: vec![],
        hidden: vec![],
        gone: 0,
        commits: 0,
    });
    s.totals.lines = Some(Lines::default());
    let out = render(&view(Some(&s)), 120, 42);
    assert!(out.contains("window: 7 days (t)"));
    assert!(
        out.matches("no commits in this window").count() >= 4,
        "{out}"
    );
    assert!(out.contains("13 (+3 remote)"), "totals unchanged");
}

#[test]
fn the_shallow_and_sampled_banners() {
    let mut s = stats();
    s.shallow = true;
    s.sampled = true;
    let out = render(&view(Some(&s)), 120, 44);
    assert!(
        out.contains("shallow: history before 2026-03-13 is not available"),
        "{out}"
    );
    assert!(
        out.contains("sampled: newest 20 000 commits read, lines from the newest 5 000"),
        "{out}"
    );
}

#[test]
fn fewer_than_two_buckets_is_text_not_a_chart() {
    let mut s = stats();
    s.series = vec![Bucket {
        start: NOW - 3 * DAY,
        commits: 3,
    }];
    s.totals.commits = 3;
    let out = render(&view(Some(&s)), 120, 42);
    assert!(out.contains("3 commits this week"), "{out}");
}

#[test]
fn the_activity_caption_follows_the_granularity() {
    let mut s = stats();
    s.granularity = Granularity::Day;
    let out = render(&view(Some(&s)), 120, 42);
    assert!(
        out.contains("Activity  commits per day") && out.contains("13 days of history · peak "),
        "{out}"
    );
    s.granularity = Granularity::Month;
    let out = render(&view(Some(&s)), 120, 42);
    assert!(
        out.contains("commits per month") && out.contains("13 months of history · peak "),
        "{out}"
    );
}

#[test]
fn no_remote_hides_the_remote_count_and_a_small_whole_shows_counts() {
    let mut s = stats();
    s.totals.remote_branches = 0;
    s.totals.remotes = 0;
    s.since_tag = None;
    s.totals.commits = 12;
    s.kinds = vec![
        KindStat {
            kind: Kind::Feat,
            commits: 7,
        },
        KindStat {
            kind: Kind::Fix,
            commits: 5,
        },
    ];
    s.authors = vec![author("Solo Dev", 12)];
    s.hot_files = Some(HotFiles {
        files: vec![FileStat {
            path: "a.rs".to_owned(),
            share: Share::of(6, 12),
            added: 1,
            removed: 0,
        }],
        hidden: vec![],
        gone: 0,
        commits: 12,
    });
    let out = render(&view(Some(&s)), 120, 42);
    assert!(out.contains("branches") && !out.contains("remote"), "{out}");
    assert!(out.contains("Hot files  commits touching"));
    assert!(!out.contains("since"));
    // 12 commits: counts, no percentages, in the kinds legend, the bars and the branches.
    let legend = out.lines().find(|l| l.contains("● feat")).unwrap();
    assert!(legend.contains(" 7") && !legend.contains('%'), "{legend}");
    let solo = out.lines().find(|l| l.contains("Solo Dev")).unwrap();
    assert!(solo.contains("12") && !solo.contains('%'), "{solo}");
}
