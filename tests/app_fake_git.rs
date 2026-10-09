#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "integration test scaffolding: a failed setup is the assertion"
)]
//! App logic driven through `FakeGit`: no repository on disk, no `git`
//! subprocess, and failures a real git rarely produces on demand. The same
//! flows run against a real repository in `app_commit.rs` and `app_stage.rs`.
//! See `docs/PLAN_21_GIT_PORT.md`.

use ferrit::git::error::GitError;
use ferrit::git::fake::FakeGit;
use ferrit::git::model::Change;
use ferrit::tui::App;
use ferrit::tui::components::panes::nav::Pane;
use ratatui::crossterm::event::{KeyCode, KeyEvent};

fn char_key(c: char) -> KeyEvent {
    KeyEvent::from(KeyCode::Char(c))
}

fn type_text(app: &mut App, text: &str) {
    for c in text.chars() {
        app.feed_key(char_key(c));
    }
}

fn staged_fake() -> FakeGit {
    FakeGit::new("fake")
        .with_commit("init")
        .with_file("a.txt", Change::Modified, Change::None)
}

#[test]
fn the_panes_show_what_the_port_reports() {
    let app = App::with_git(Box::new(staged_fake()));
    assert_eq!(app.row_count(Pane::Commits), 1);
    assert!(app.status_lines()[0].to_string().contains("fake"));
    assert!(app.status_lines()[0].to_string().contains("main"));
}

#[test]
fn c_types_and_enter_commits_through_the_port() {
    let fake = staged_fake();
    let mut app = App::with_git(Box::new(fake.clone()));

    app.feed_key(char_key('c'));
    assert!(app.commit_popup().is_some());
    type_text(&mut app, "feat: a new line");
    app.feed_key(KeyEvent::from(KeyCode::Enter));

    assert!(app.commit_popup().is_none(), "popup closed on success");
    assert_eq!(app.row_count(Pane::Commits), 2);
    assert_eq!(
        fake.current().commits.first().map(|c| c.summary.as_str()),
        Some("feat: a new line")
    );
    assert!(fake.calls().contains(&"commit"));
}

#[test]
fn a_rejected_commit_keeps_the_popup_and_shows_why() {
    let fake = staged_fake().fail_next(
        "commit",
        GitError::CommitFailed("pre-commit hook said no".to_owned()),
    );
    let mut app = App::with_git(Box::new(fake));

    app.feed_key(char_key('c'));
    type_text(&mut app, "feat: nope");
    app.feed_key(KeyEvent::from(KeyCode::Enter));

    assert!(
        app.commit_popup().is_some(),
        "the popup stays open to retry"
    );
    assert_eq!(app.row_count(Pane::Commits), 1, "no commit was made");
    let status = app.status_lines()[0].to_string();
    assert!(status.contains("pre-commit hook said no"), "{status}");
}
