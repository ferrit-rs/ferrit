//! What a stage acts on, decided from the files alone (`git::staging`).

use std::path::PathBuf;

use ferrit::git::apply::{ApplyDir, Granule};
use ferrit::git::model::{Change, FileEntry};
use ferrit::git::staging::direction;

fn file(staged: Change, worktree: Change) -> FileEntry {
    FileEntry {
        path: PathBuf::from("a.txt"),
        staged,
        worktree,
        binary: false,
    }
}

#[test]
fn a_worktree_change_is_staged() {
    let files = [file(Change::None, Change::Modified)];
    assert_eq!(direction(&files), Some(ApplyDir::Forward));
}

#[test]
fn a_half_staged_file_is_staged_further() {
    let files = [file(Change::Modified, Change::Modified)];
    assert_eq!(direction(&files), Some(ApplyDir::Forward));
}

#[test]
fn a_fully_staged_file_is_unstaged() {
    let files = [file(Change::Modified, Change::None)];
    assert_eq!(direction(&files), Some(ApplyDir::Reverse));
}

#[test]
fn nothing_to_do_is_no_direction() {
    let files = [file(Change::None, Change::None)];
    assert_eq!(direction(&files), None);
    assert_eq!(direction(&[]), None);
}

#[test]
fn one_unstaged_file_among_staged_ones_decides_for_all() {
    let files = [
        file(Change::Modified, Change::None),
        file(Change::None, Change::Added),
    ];
    assert_eq!(direction(&files), Some(ApplyDir::Forward));
}

#[test]
fn a_granule_is_named_for_the_question() {
    let hunk = Granule::Hunk {
        patch: String::new(),
    };
    let lines = |n| Granule::Lines {
        file_header: String::new(),
        hunk_header: String::new(),
        hunk_body: String::new(),
        lines: (0..n).collect(),
    };
    assert_eq!(hunk.describe(), "this hunk");
    assert_eq!(lines(1).describe(), "1 line");
    assert_eq!(lines(3).describe(), "3 lines");
}
