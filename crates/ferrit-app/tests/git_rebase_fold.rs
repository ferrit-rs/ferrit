//! When an autosquash would change anything (`git::rebase::has_foldable_fixup`).

use ferrit_domain::model::{CommitEntry, PushState};
use ferrit_domain::rebase::has_foldable_fixup;

fn commit(summary: &str) -> CommitEntry {
    CommitEntry {
        full_hash: summary.to_owned(),
        short_hash: String::new(),
        author: String::new(),
        author_email: String::new(),
        summary: summary.to_owned(),
        body: String::new(),
        time: 0,
        refs: Vec::new(),
        push_state: PushState::default(),
    }
}

/// Newest first, like the Commits pane.
fn log(summaries: &[&str]) -> Vec<CommitEntry> {
    summaries.iter().map(|s| commit(s)).collect()
}

#[test]
fn a_fixup_above_its_target_is_foldable() {
    let commits = log(&["fixup! add parser", "add parser", "init"]);
    assert!(has_foldable_fixup(&commits, 2));
    assert!(has_foldable_fixup(&commits, 1));
}

#[test]
fn a_selection_that_stops_above_the_target_folds_nothing() {
    let commits = log(&["fixup! add parser", "add parser", "init"]);
    assert!(!has_foldable_fixup(&commits, 0));
}

#[test]
fn a_squash_prefix_counts_too() {
    let commits = log(&["squash! add parser", "add parser"]);
    assert!(has_foldable_fixup(&commits, 1));
}

#[test]
fn a_fixup_without_a_target_folds_nothing() {
    let commits = log(&["fixup! missing", "add parser"]);
    assert!(!has_foldable_fixup(&commits, 1));
}

#[test]
fn a_selection_past_the_end_folds_nothing() {
    assert!(!has_foldable_fixup(&log(&["a"]), 3));
}
