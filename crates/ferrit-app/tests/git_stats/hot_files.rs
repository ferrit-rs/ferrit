//! Hot files.

use crate::support::{RICHARD, TempDir, ago, commit, git, init, project, stats_at};
use ferrit_domain::stats::Window;

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
