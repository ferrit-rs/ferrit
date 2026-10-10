//! Calendar and other branches.

use crate::support::{RICHARD, TempDir, ago, commit, git, init, project, stats_at};
use ferrit_domain::stats::Window;

#[test]
fn the_daily_counts_cover_26_weeks_whatever_the_window() {
    let tmp = project();
    let week = stats_at(tmp.path(), Window::Days7);
    let all = stats_at(tmp.path(), Window::All);
    assert_eq!(
        week.daily.len(),
        182,
        "one bucket per day, empty ones included"
    );
    assert_eq!(week.daily, all.daily, "the heat map ignores the window");
    let recent: usize = week.daily.iter().map(|b| b.commits).sum();
    assert_eq!(
        recent, 7,
        "every commit of main in the last 26 weeks, the 100-day-old one included"
    );
    assert!(
        week.daily
            .windows(2)
            .all(|w| w[1].start - w[0].start == 86_400),
        "consecutive days"
    );
}

#[test]
fn commits_only_on_other_branches_are_not_counted_but_their_branches_still_show() {
    let tmp = project();
    let stats = stats_at(tmp.path(), Window::All);
    assert_eq!(
        stats.totals.commits, 7,
        "c1 to c5, d1 and the merge: not o1, f1, f2"
    );
    assert!(
        stats.authors.iter().all(|a| a.name != "Ola"),
        "Ola only has a commit on `old`"
    );
    let names: Vec<_> = stats.branches.iter().map(|b| b.name.as_str()).collect();
    assert!(
        names.contains(&"old") && names.contains(&"feature"),
        "the branch health still lists every branch: {names:?}"
    );
}

#[test]
fn working_on_another_branch_still_counts_main() {
    let tmp = project();
    git(tmp.path(), &["checkout", "-q", "feature"]);
    let stats = stats_at(tmp.path(), Window::All);
    assert_eq!(stats.totals.commits, 7, "main's commits, wherever HEAD is");
    assert_eq!(stats.main_branch.as_deref(), Some("main"));
}

#[test]
fn with_no_main_branch_the_history_of_head_is_what_is_counted() {
    let tmp = TempDir::new("stats-no-main");
    let dir = tmp.path();
    init(dir, "dev");
    commit(dir, RICHARD, ago(5), "feat: one", &[("a.txt", "a\n")]);
    commit(dir, RICHARD, ago(4), "feat: two", &[("a.txt", "b\n")]);
    git(dir, &["checkout", "-q", "-b", "other"]);
    commit(dir, RICHARD, ago(1), "fix: elsewhere", &[("b.txt", "b\n")]);
    git(dir, &["checkout", "-q", "dev"]);
    let stats = stats_at(dir, Window::All);
    assert_eq!(stats.main_branch, None);
    assert_eq!(stats.totals.commits, 2, "dev's own history, not `other`'s");
}
