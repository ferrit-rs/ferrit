#![allow(
    clippy::unwrap_used,
    reason = "integration test scaffolding: a failed setup is the assertion"
)]
//! What opening the commit editor comes to, and what it starts from
//! (`git::commit::plan_open`, `prefill`).

use std::path::PathBuf;

use ferrit_domain::commit::{CommitKind, OpenPlan, plan_open, prefill};
use ferrit_domain::model::{Change, FileEntry};
use ferrit_git::fake::FakeGit;

fn file(staged: Change, worktree: Change) -> FileEntry {
    FileEntry {
        path: PathBuf::from("a.txt"),
        staged,
        worktree,
        binary: false,
    }
}

#[test]
fn a_new_commit_with_nothing_staged_asks_to_stage_everything_first() {
    let files = [file(Change::None, Change::Modified)];
    assert_eq!(
        plan_open(&CommitKind::Normal, &files, 3),
        OpenPlan::StageAllFirst
    );
    assert_eq!(
        plan_open(&CommitKind::Normal, &[], 3),
        OpenPlan::StageAllFirst
    );
}

#[test]
fn a_new_commit_with_something_staged_opens_the_editor() {
    let files = [file(Change::Modified, Change::None)];
    assert_eq!(plan_open(&CommitKind::Normal, &files, 0), OpenPlan::Edit);
}

#[test]
fn amend_and_reword_need_a_commit() {
    assert_eq!(plan_open(&CommitKind::Amend, &[], 0), OpenPlan::NoCommit);
    assert_eq!(plan_open(&CommitKind::Reword, &[], 0), OpenPlan::NoCommit);
    assert_eq!(plan_open(&CommitKind::Amend, &[], 2), OpenPlan::Edit);
}

#[test]
fn a_fixup_does_not_ask_for_a_message() {
    let fixup = CommitKind::Fixup {
        target: "abc".to_owned(),
    };
    assert!(!fixup.needs_summary());
    assert!(CommitKind::Normal.needs_summary());
}

#[test]
fn amend_starts_from_the_head_message_and_leaves_the_saved_draft() {
    let repo = FakeGit::new("r").with_commit("fix the parser");
    let mut saved = Some("half written".to_owned());
    assert_eq!(
        prefill(&CommitKind::Amend, &repo, &mut saved).as_deref(),
        Some("fix the parser")
    );
    assert_eq!(saved.as_deref(), Some("half written"));
}

#[test]
fn a_new_commit_restores_the_draft_a_cancelled_editor_kept() {
    let repo = FakeGit::new("r");
    let mut saved = Some("half written".to_owned());
    assert_eq!(
        prefill(&CommitKind::Normal, &repo, &mut saved).as_deref(),
        Some("half written")
    );
    assert_eq!(saved, None, "the draft is taken, not copied");
    assert_eq!(prefill(&CommitKind::Normal, &repo, &mut saved), None);
}
