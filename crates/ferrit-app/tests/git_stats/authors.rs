//! Authors.

use crate::support::{
    MAX_NEW, MAX_OLD, OLA, RICHARD, RICHARD_OLD, RICHARD_WORK, TempDir, ago, commit, init, project,
    stats_at,
};
use ferrit_domain::stats::Window;

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
