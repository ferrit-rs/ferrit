//! Kinds branches and work state.

use crate::support::{ago, git, project, stats_at};
use ferrit::domain::git::stats::Window;
use ferrit::domain::git::stats::kind::Kind;

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
