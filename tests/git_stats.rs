#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::pathbuf_init_then_push,
    elided_lifetimes_in_paths,
    reason = "integration test scaffolding: a failed setup is the assertion, helper ergonomics beat lint-cleanliness here"
)]
//! `Repo::stats` on fixture repositories with fixed dates, several authors, a
//! `.mailmap`, conventional prefixes, a merge commit, branches ahead / behind /
//! merged / stale and a tag. See `docs/PLAN_13_DASHBOARD.md`, milestones D0
//! and D1. "Now" is fixed at 2026-09-29 12:00 UTC (a Tuesday) so every bucket
//! is predictable.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::AtomicBool;

use ferrit::domain::git::Repo;
use ferrit::domain::git::command_log;
use ferrit::domain::git::error::GitError;
use ferrit::domain::git::stats::kind::Kind;
use ferrit::domain::git::stats::series::Granularity;
use ferrit::domain::git::stats::{RepoStats, StatsOptions, Window};

const NOW: i64 = 1_790_683_200;
const DAY: i64 = 86_400;

fn ago(days: i64) -> i64 {
    NOW - days * DAY
}

static NEXT_DIR: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let mut path = std::env::temp_dir();
        // The clock ticks in microseconds on macOS: tests starting together need a counter too.
        let n = NEXT_DIR.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        path.push(format!("ferrit-{tag}-{}-{nanos}-{n}", std::process::id()));
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

fn git(dir: &Path, args: &[&str]) -> String {
    git_as(dir, ("Fixture", "fixture@example.com", NOW), args)
}

fn git_as(dir: &Path, who: (&str, &str, i64), args: &[&str]) -> String {
    let date = format!("{} +0000", who.2);
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", who.0)
        .env("GIT_AUTHOR_EMAIL", who.1)
        .env("GIT_AUTHOR_DATE", &date)
        .env("GIT_COMMITTER_NAME", who.0)
        .env("GIT_COMMITTER_EMAIL", who.1)
        .env("GIT_COMMITTER_DATE", &date)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

fn init(dir: &Path, branch: &str) {
    git(dir, &["init", "-q", "-b", branch]);
}

/// Write `files`, stage everything and commit as `who` at `when`; returns the hash.
fn commit(
    dir: &Path,
    who: (&str, &str),
    when: i64,
    message: &str,
    files: &[(&str, &str)],
) -> String {
    for (name, content) in files {
        let path = dir.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }
    let who = (who.0, who.1, when);
    git_as(dir, who, &["add", "-A"]);
    git_as(dir, who, &["commit", "-q", "-m", message]);
    git(dir, &["rev-parse", "HEAD"])
}

const RICHARD: (&str, &str) = ("Richard", "richard@example.com");
const MAX_OLD: (&str, &str) = ("Max", "max@old.example.com");
const MAX_NEW: (&str, &str) = ("Max W.", "max@example.com");
const OLA: (&str, &str) = ("Ola", "ola@example.com");

fn stats_at(dir: &Path, window: Window) -> RepoStats {
    let repo = Repo::open(dir).unwrap();
    let opts = StatsOptions {
        now: NOW,
        ..StatsOptions::default()
    };
    repo.stats_with(window, &opts, &AtomicBool::new(false))
        .unwrap()
}

/// The reference project, on `main` (see the history in the comments):
///
/// ```text
/// c1 100d Richard chore: init      (.mailmap, a.txt, Cargo.lock, CHANGELOG.md)
/// c2  45d Max     feat: add b      <- tag v1
/// c3  10d Max W.  fix(core): bug
/// M    8d Richard Merge branch 'done'   (d1 9d Richard feat: done work)
/// c4   3d Richard docs: readme     (empties a.txt: one line removed)
/// c5   2d Richard feat!: breaking       <- main, `same`
/// old:     o1 80d Ola     wip: stuff        (off c2)
/// feature: f1 5d Max W.   feat: f1          (off c3)
///          f2 4d Richard  test: f2
/// ```
///
/// plus a stash, and a dirty tree: `a.txt` changed, `b.txt` staged, `u.txt` new.
fn project() -> TempDir {
    let tmp = TempDir::new("stats-project");
    let dir = tmp.path();
    init(dir, "main");
    let mailmap =
        "Max Wells <max@example.com> <max@old.example.com>\nMax Wells <max@example.com>\n";
    commit(
        dir,
        RICHARD,
        ago(100),
        "chore: init",
        &[
            (".mailmap", mailmap),
            ("a.txt", "a\n"),
            ("Cargo.lock", "l1\n"),
            ("CHANGELOG.md", "c1\n"),
        ],
    );
    let c2 = commit(
        dir,
        MAX_OLD,
        ago(45),
        "feat: add b",
        &[
            ("b.txt", "b\n"),
            ("src/app.rs", "x\n"),
            ("Cargo.lock", "l1\nl2\n"),
            ("CHANGELOG.md", "c1\nc2\n"),
        ],
    );
    git(dir, &["tag", "v1"]);
    git(dir, &["checkout", "-q", "-b", "old", &c2]);
    commit(dir, OLA, ago(80), "wip: stuff", &[("o.txt", "o\n")]);
    git(dir, &["checkout", "-q", "main"]);
    let c3 = commit(
        dir,
        MAX_NEW,
        ago(10),
        "fix(core): bug",
        &[("c.txt", "c\n"), ("src/app.rs", "x\ny\n")],
    );
    git(dir, &["checkout", "-q", "-b", "done"]);
    commit(
        dir,
        RICHARD,
        ago(9),
        "feat: done work",
        &[("d.txt", "d\n"), ("src/app.rs", "x\ny\nz\n")],
    );
    git(dir, &["checkout", "-q", "main"]);
    git_as(
        dir,
        ("Richard", "richard@example.com", ago(8)),
        &[
            "merge",
            "-q",
            "--no-ff",
            "-m",
            "Merge branch 'done'",
            "done",
        ],
    );
    git(dir, &["checkout", "-q", "-b", "feature", &c3]);
    commit(dir, MAX_NEW, ago(5), "feat: f1", &[("f.txt", "f\n")]);
    commit(dir, RICHARD, ago(4), "test: f2", &[("f.txt", "f\nf\n")]);
    git(dir, &["checkout", "-q", "main"]);
    commit(
        dir,
        RICHARD,
        ago(3),
        "docs: readme",
        &[("README.md", "r\n")],
    );
    commit(dir, RICHARD, ago(2), "feat!: breaking", &[("e.txt", "e\n")]);
    git(dir, &["branch", "same"]);
    fs::write(dir.join("a.txt"), "stashed\n").unwrap();
    git(dir, &["stash", "push", "-q"]);
    fs::write(dir.join("a.txt"), "changed\n").unwrap();
    fs::write(dir.join("b.txt"), "b staged\n").unwrap();
    git(dir, &["add", "b.txt"]);
    fs::write(dir.join("u.txt"), "u\n").unwrap();
    tmp
}

fn buckets(stats: &RepoStats) -> Vec<usize> {
    stats.series.iter().map(|b| b.commits).collect()
}

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

#[test]
fn authors_are_grouped_through_mailmap_and_ranked() {
    let tmp = project();
    let stats = stats_at(tmp.path(), Window::Days90);
    let names: Vec<_> = stats
        .authors
        .iter()
        .map(|a| (a.name.as_str(), a.email.as_str(), a.commits))
        .collect();
    assert_eq!(
        names,
        [
            ("Richard", "richard@example.com", 4),
            ("Max Wells", "max@example.com", 2),
        ]
    );
    let max = &stats.authors[1];
    assert_eq!(max.last_commit, ago(10), "c2 and c3 under one identity");
}

#[test]
fn without_a_mailmap_the_same_person_stays_two_authors() {
    let tmp = TempDir::new("stats-nomailmap");
    init(tmp.path(), "main");
    commit(
        tmp.path(),
        MAX_OLD,
        ago(3),
        "feat: one",
        &[("a.txt", "a\n")],
    );
    commit(
        tmp.path(),
        MAX_NEW,
        ago(2),
        "feat: two",
        &[("a.txt", "b\n")],
    );
    let stats = stats_at(tmp.path(), Window::All);
    assert_eq!(stats.totals.authors, 2);
}

const RICHARD_WORK: (&str, &str) = ("Richard", "richard@work.example.com");
const RICHARD_OLD: (&str, &str) = ("richard ", "Richard@Old.example.com");

#[test]
fn a_name_with_several_emails_is_one_author() {
    let tmp = TempDir::new("stats-samename");
    let dir = tmp.path();
    init(dir, "main");
    commit(dir, RICHARD, ago(9), "feat: one", &[("a.txt", "1\n2\n")]);
    commit(dir, RICHARD_WORK, ago(8), "feat: two", &[("b.txt", "1\n")]);
    commit(
        dir,
        RICHARD_WORK,
        ago(7),
        "feat: three",
        &[("c.txt", "1\n")],
    );
    commit(
        dir,
        RICHARD_OLD,
        ago(3),
        "feat: four",
        &[("d.txt", "1\n2\n3\n")],
    );
    commit(dir, OLA, ago(1), "feat: five", &[("e.txt", "1\n")]);
    commit(dir, OLA, ago(1), "feat: six", &[("f.txt", "1\n")]);
    commit(dir, OLA, ago(1), "feat: seven", &[("g.txt", "1\n")]);
    let stats = stats_at(dir, Window::All);
    assert_eq!(stats.totals.authors, 2);
    let rows: Vec<_> = stats
        .authors
        .iter()
        .map(|a| (a.name.as_str(), a.email.as_str(), a.commits))
        .collect();
    assert_eq!(
        rows,
        [
            ("Richard", "richard@work.example.com", 4),
            ("Ola", "ola@example.com", 3),
        ],
        "most commits first; the most frequent email names the row"
    );
    let richard = &stats.authors[0];
    assert_eq!(
        richard.emails,
        [
            "richard@work.example.com",
            "Richard@Old.example.com",
            "richard@example.com"
        ],
        "most commits first, ties alphabetical (the case of the first sighting is kept)"
    );
    assert_eq!(richard.last_commit, ago(3));
    assert_eq!((richard.added, richard.removed), (Some(7), Some(0)));
    assert_eq!(stats.authors[1].emails, ["ola@example.com"]);
}

#[test]
fn an_email_tie_is_broken_alphabetically() {
    let tmp = TempDir::new("stats-tie");
    let dir = tmp.path();
    init(dir, "main");
    commit(dir, RICHARD_WORK, ago(3), "feat: one", &[("a.txt", "1\n")]);
    commit(dir, RICHARD, ago(2), "feat: two", &[("b.txt", "1\n")]);
    let stats = stats_at(dir, Window::All);
    assert_eq!(stats.authors.len(), 1);
    assert_eq!(stats.authors[0].email, "richard@example.com");
    assert_eq!(
        stats.authors[0].emails,
        ["richard@example.com", "richard@work.example.com"]
    );
}

#[test]
fn a_mailmap_groups_before_the_name_merge() {
    let tmp = TempDir::new("stats-mailmap-first");
    let dir = tmp.path();
    init(dir, "main");
    let mailmap = "Max Wells <max@example.com> <max@old.example.com>\n";
    commit(
        dir,
        RICHARD,
        ago(4),
        "chore: init",
        &[(".mailmap", mailmap)],
    );
    commit(dir, MAX_OLD, ago(3), "feat: one", &[("a.txt", "1\n")]);
    let wells = ("Max Wells", "max@example.com");
    commit(dir, wells, ago(2), "feat: two", &[("b.txt", "1\n")]);
    let stats = stats_at(dir, Window::All);
    let max = stats
        .authors
        .iter()
        .find(|a| a.name == "Max Wells")
        .unwrap();
    assert_eq!(max.commits, 2);
    assert_eq!(max.emails, ["max@example.com"], "the mailmap joined them");
    assert_eq!(stats.totals.authors, 2);
}

#[test]
fn two_different_names_stay_apart() {
    let tmp = TempDir::new("stats-names");
    let dir = tmp.path();
    init(dir, "main");
    commit(dir, RICHARD, ago(3), "feat: one", &[("a.txt", "1\n")]);
    commit(dir, OLA, ago(2), "feat: two", &[("b.txt", "1\n")]);
    let stats = stats_at(dir, Window::All);
    assert_eq!(stats.totals.authors, 2);
    assert!(stats.authors.iter().all(|a| a.emails.len() == 1));
}

#[test]
fn kinds_come_from_the_prefix_and_leave_merges_out() {
    let tmp = project();
    let stats = stats_at(tmp.path(), Window::Days90);
    let kinds: Vec<_> = stats.kinds.iter().map(|k| (k.kind, k.commits)).collect();
    assert_eq!(kinds, [(Kind::Feat, 3), (Kind::Fix, 1), (Kind::Docs, 1),]);
    assert_eq!(
        stats.kinds.iter().map(|k| k.commits).sum::<usize>(),
        stats.totals.commits - 1,
        "the merge commit is a commit but not a kind"
    );
}

#[test]
fn branches_are_ordered_and_measured_against_main() {
    let tmp = project();
    let stats = stats_at(tmp.path(), Window::Days90);
    assert_eq!(stats.main_branch.as_deref(), Some("main"));
    let rows: Vec<_> = stats
        .branches
        .iter()
        .map(|b| {
            let v = b.vs_main.unwrap();
            (
                b.name.as_str(),
                b.current,
                v.ahead,
                v.behind,
                v.merged,
                b.stale,
            )
        })
        .collect();
    assert_eq!(
        rows,
        [
            ("main", true, 0, 0, false, false),
            ("feature", false, 2, 4, false, false),
            ("old", false, 1, 5, false, true),
            ("same", false, 0, 0, true, false),
            ("done", false, 0, 3, true, false),
        ]
    );
    assert_eq!(stats.branches[2].tip_time, ago(80));
}

#[test]
fn the_current_branch_leads_even_when_it_is_not_main_and_is_never_stale() {
    let tmp = project();
    git(tmp.path(), &["stash", "push", "-q", "-u"]);
    git(tmp.path(), &["checkout", "-q", "old"]);
    let stats = stats_at(tmp.path(), Window::Days90);
    let first = &stats.branches[0];
    assert_eq!(
        (first.name.as_str(), first.current, first.stale),
        ("old", true, false)
    );
    assert_eq!(
        stats.branches[1].name, "feature",
        "then the branches with work not in main"
    );
}

#[test]
fn since_tag_counts_the_commits_after_the_newest_reachable_tag() {
    let tmp = project();
    let stats = stats_at(tmp.path(), Window::Days90);
    let tag = stats.since_tag.unwrap();
    assert_eq!((tag.name.as_str(), tag.commits), ("v1", 5));
    git(tmp.path(), &["tag", "-a", "-m", "second", "v2", "HEAD"]);
    let stats = stats_at(tmp.path(), Window::Days90);
    let tag = stats.since_tag.unwrap();
    assert_eq!((tag.name.as_str(), tag.commits), ("v2", 0));
    assert_eq!(stats.totals.tags, 2);
}

#[test]
fn a_tag_on_a_branch_that_is_not_reachable_from_head_is_ignored() {
    let tmp = project();
    git(tmp.path(), &["tag", "on-old", "old"]);
    let stats = stats_at(tmp.path(), Window::Days90);
    assert_eq!(stats.since_tag.unwrap().name, "v1");
}

#[test]
fn work_state_counts_the_tree_and_the_stash() {
    let tmp = project();
    let work = stats_at(tmp.path(), Window::Days90).work;
    assert_eq!(
        (
            work.changed,
            work.staged,
            work.untracked,
            work.conflicted,
            work.stashes
        ),
        (1, 1, 1, 0, 1)
    );
    assert_eq!(work.upstream, None);
    assert_eq!((work.ahead, work.behind), (0, 0));
}

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

#[test]
fn hot_files_rank_by_commits_touching_them_and_hide_lockfiles_and_changelogs() {
    let tmp = project();
    let stats = stats_at(tmp.path(), Window::Days90);
    let hot = stats.hot_files.unwrap();
    assert_eq!(hot.commits, 5, "the merge commit is not read");
    let rows: Vec<_> = hot
        .files
        .iter()
        .map(|f| {
            (
                f.path.as_str(),
                f.share.count,
                f.share.percent,
                f.added,
                f.removed,
            )
        })
        .collect();
    assert_eq!(
        rows,
        [
            ("src/app.rs", 3, Some(60), 3, 0),
            ("README.md", 1, Some(20), 1, 0),
            ("b.txt", 1, Some(20), 1, 0),
            ("c.txt", 1, Some(20), 1, 0),
            ("d.txt", 1, Some(20), 1, 0),
            ("e.txt", 1, Some(20), 1, 0),
        ]
    );
    assert_eq!(hot.hidden, ["CHANGELOG.md", "Cargo.lock"]);
    assert_eq!(
        hot.gone, 0,
        "f.txt and o.txt are on branches other than main, which is not walked"
    );
}

#[test]
fn only_the_ten_hottest_files_are_listed() {
    let tmp = TempDir::new("stats-many");
    init(tmp.path(), "main");
    let names: Vec<String> = (0..12).map(|i| format!("f{i:02}.txt")).collect();
    let files: Vec<(&str, &str)> = names.iter().map(|n| (n.as_str(), "x\n")).collect();
    commit(tmp.path(), RICHARD, ago(2), "feat: many", &files);
    let hot = stats_at(tmp.path(), Window::All).hot_files.unwrap();
    assert_eq!(hot.files.len(), 10);
    assert_eq!(hot.files[0].share.percent, Some(100));
    assert!(hot.hidden.is_empty());
    assert_eq!(hot.gone, 0);
}

#[test]
fn hot_files_drop_a_deleted_file_and_a_renamed_away_path() {
    let tmp = TempDir::new("stats-gone");
    let dir = tmp.path();
    init(dir, "main");
    commit(
        dir,
        RICHARD,
        ago(9),
        "feat: one",
        &[("old.rs", "1\n"), ("dead.rs", "1\n"), ("keep.rs", "1\n")],
    );
    commit(
        dir,
        RICHARD,
        ago(8),
        "fix: two",
        &[
            ("old.rs", "1\n2\n"),
            ("dead.rs", "1\n2\n"),
            ("keep.rs", "1\n2\n"),
        ],
    );
    git(dir, &["rm", "-q", "dead.rs"]);
    git(dir, &["mv", "old.rs", "new.rs"]);
    git(dir, &["commit", "-q", "-m", "refactor: move"]);
    let hot = stats_at(dir, Window::All).hot_files.unwrap();
    let rows: Vec<_> = hot
        .files
        .iter()
        .map(|f| (f.path.as_str(), f.share.count))
        .collect();
    assert_eq!(
        rows,
        [("keep.rs", 2), ("new.rs", 1)],
        "only the post-rename commit counts for the new path"
    );
    assert_eq!(hot.gone, 2, "dead.rs and old.rs");
    assert_eq!(hot.commits, 3, "the shares stay a share of the window");
}

#[test]
fn an_ignored_path_that_is_gone_counts_in_hidden_only() {
    let tmp = TempDir::new("stats-gone-hidden");
    let dir = tmp.path();
    init(dir, "main");
    commit(
        dir,
        RICHARD,
        ago(3),
        "feat: one",
        &[("a.rs", "1\n"), ("CHANGELOG.md", "c\n")],
    );
    git(dir, &["rm", "-q", "CHANGELOG.md"]);
    git(dir, &["commit", "-q", "-m", "chore: drop it"]);
    let hot = stats_at(dir, Window::All).hot_files.unwrap();
    assert_eq!(hot.hidden, ["CHANGELOG.md"]);
    assert_eq!(hot.gone, 0);
    assert_eq!(hot.files.len(), 1);
}

#[test]
fn an_unborn_head_has_no_hot_files_and_nothing_gone() {
    let tmp = TempDir::new("stats-unborn");
    init(tmp.path(), "main");
    let hot = stats_at(tmp.path(), Window::All).hot_files;
    assert!(hot.is_none_or(|h| h.files.is_empty() && h.gone == 0));
}

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
