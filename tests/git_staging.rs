#![allow(
    clippy::unwrap_used,
    clippy::panic,
    reason = "integration test scaffolding: a failed setup is the assertion"
)]
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

mod plans {
    use std::path::Path;

    use ferrit::git::apply::{ApplyTarget, Granule};
    use ferrit::git::diff::DiffSide;
    use ferrit::git::fake::FakeGit;
    use ferrit::git::model::{Change, FileEntry};
    use ferrit::git::port::GitRead;
    use ferrit::git::staging::{
        Plan, Refusal, StageAction, plan_all, plan_directory, plan_discard_file, plan_file,
        plan_granule, run,
    };

    use crate::{ApplyDir, file};

    fn entry(path: &str, staged: Change, worktree: Change) -> FileEntry {
        FileEntry {
            path: path.into(),
            ..file(staged, worktree)
        }
    }

    #[test]
    fn a_modified_file_is_staged_forward() {
        let repo = FakeGit::new("r");
        let plan = plan_file(&repo, &entry("a.txt", Change::None, Change::Modified));
        assert_eq!(
            plan,
            Plan::Do(StageAction::File {
                path: "a.txt".into(),
                dir: ApplyDir::Forward
            })
        );
    }

    #[test]
    fn a_clean_file_has_nothing_to_plan() {
        let repo = FakeGit::new("r");
        assert_eq!(
            plan_file(&repo, &entry("a.txt", Change::None, Change::None)),
            Plan::Nothing
        );
    }

    #[test]
    fn a_conflicted_file_is_refused_while_markers_may_remain() {
        let repo = FakeGit::new("r");
        let plan = plan_file(
            &repo,
            &entry("a.txt", Change::Conflicted, Change::Conflicted),
        );
        assert_eq!(plan, Plan::Refuse(Refusal::ConflictMarkers("a.txt".into())));
    }

    #[test]
    fn unstaging_a_conflicted_file_is_not_refused() {
        let repo = FakeGit::new("r");
        let plan = plan_file(&repo, &entry("a.txt", Change::Conflicted, Change::None));
        assert!(matches!(plan, Plan::Do(_)), "{plan:?}");
    }

    #[test]
    fn the_root_row_stages_everything_and_a_directory_only_itself() {
        let repo = FakeGit::new("r");
        let files = [
            entry("src/a.rs", Change::None, Change::Modified),
            entry("docs/b.md", Change::None, Change::Modified),
        ];
        assert_eq!(
            plan_directory(&repo, &files, Path::new("")),
            Plan::Do(StageAction::All {
                dir: ApplyDir::Forward
            })
        );
        assert_eq!(
            plan_directory(&repo, &files, Path::new("src")),
            Plan::Do(StageAction::File {
                path: "src".into(),
                dir: ApplyDir::Forward
            })
        );
        assert_eq!(
            plan_directory(&repo, &files, Path::new("lib")),
            Plan::Nothing
        );
    }

    #[test]
    fn staging_all_leaves_out_the_conflicted_files() {
        let repo = FakeGit::new("r");
        let files = [
            entry("a.rs", Change::None, Change::Modified),
            entry("c.rs", Change::Conflicted, Change::Conflicted),
        ];
        let plan = plan_all(&repo, &files);
        let Plan::Do(action) = plan else {
            panic!("expected a call, got {plan:?}");
        };
        assert_eq!(action.left_out(), Some(&["c.rs".into()][..]));
    }

    #[test]
    fn a_granule_goes_to_the_index_forward_from_the_worktree_side() {
        let hunk = Granule::Hunk { patch: "p".into() };
        let StageAction::Granule { dir, target, .. } =
            plan_granule(hunk.clone(), DiffSide::Worktree)
        else {
            panic!("a granule action");
        };
        assert_eq!((dir, target), (ApplyDir::Forward, ApplyTarget::Index));
        let StageAction::Granule { dir, .. } = plan_granule(hunk, DiffSide::Staged) else {
            panic!("a granule action");
        };
        assert_eq!(dir, ApplyDir::Reverse);
    }

    #[test]
    fn discarding_an_untracked_file_deletes_it() {
        let files = [entry("new.txt", Change::None, Change::Untracked)];
        assert_eq!(
            plan_discard_file(&files, Path::new("new.txt")),
            StageAction::Discard {
                path: "new.txt".into(),
                untracked: true
            }
        );
    }

    #[test]
    fn running_a_stage_moves_the_file_to_the_index() {
        let mut repo = FakeGit::new("r").with_file("a.txt", Change::None, Change::Modified);
        let action = StageAction::File {
            path: "a.txt".into(),
            dir: ApplyDir::Forward,
        };
        run(&repo, &action).unwrap();
        let files = repo.snapshot().unwrap().files;
        let file = files.first().unwrap();
        assert_eq!(file.staged, Change::Modified);
        assert_eq!(file.worktree, Change::None);
    }
}
