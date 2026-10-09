#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "integration test scaffolding: a failed setup is the assertion"
)]
//! What may be up at the same time, and who gets the key. These pin the
//! behaviour before the modal state of `App` (a popup, a key-bar question, a
//! sheet, a full-screen view) is reshaped; see `docs/PLAN_22_APP_SPLIT.md`.
//!
//! The layers, from the bottom: the panes or a full-screen view (git config,
//! welcome); a sheet over them; a popup or a key-bar question on top (never
//! both); the error toast, which takes `Esc` only when nothing above needs it.

use std::fs;
use std::time::Duration;

use ferrit::config::ConfigLoad;
use ferrit::git::error::GitError;
use ferrit::git::fake::FakeGit;
use ferrit::git::model::Change;
use ferrit::tui::App;
use ferrit::tui::state::full_screens::FullScreen;
use ferrit::tui::state::pane::Pane;
use ratatui::crossterm::event::{KeyCode, KeyEvent};

fn press(app: &mut App, code: KeyCode) {
    app.feed_key(KeyEvent::from(code));
}

fn ch(app: &mut App, c: char) {
    press(app, KeyCode::Char(c));
}

fn settle(app: &mut App) {
    for _ in 0..40 {
        app.advance_clock(Duration::from_millis(16));
    }
}

/// A staged file (so `c` opens the commit popup) and a second branch (so `d`
/// on it asks before deleting).
fn app_with_fake() -> (App, FakeGit) {
    let fake = FakeGit::new("overlay")
        .with_commit("init")
        .with_branch("feat")
        .with_file("a.txt", Change::Modified, Change::None);
    (App::with_git(Box::new(fake.clone())), fake)
}

fn ask_to_delete_feat(app: &mut App) {
    app.nav.focus = Pane::Branches;
    let feat = app
        .branch_lines()
        .iter()
        .position(|line| line.to_string().contains("feat"))
        .unwrap();
    app.select(Pane::Branches, feat);
    ch(app, 'd');
}

#[test]
fn d_on_a_branch_asks_before_deleting() {
    let (mut app, _) = app_with_fake();
    ask_to_delete_feat(&mut app);
    assert!(
        app.confirm_message()
            .unwrap()
            .contains("delete branch feat")
    );
}

#[test]
fn a_question_swallows_the_key_that_would_open_a_popup() {
    let (mut app, _) = app_with_fake();
    ask_to_delete_feat(&mut app);
    ch(&mut app, 'c');
    assert!(app.commit_popup().is_none(), "c did not open the popup");
    assert!(app.confirm_message().is_some(), "the question is still up");
}

#[test]
fn n_and_esc_withdraw_the_question_and_delete_nothing() {
    for answer in [KeyCode::Char('n'), KeyCode::Esc] {
        let (mut app, fake) = app_with_fake();
        ask_to_delete_feat(&mut app);
        press(&mut app, answer);
        assert!(app.confirm_message().is_none(), "{answer:?}");
        assert!(!fake.calls().contains(&"delete_branch"), "{answer:?}");
    }
}

#[test]
fn y_answers_the_question_and_the_branch_goes() {
    let (mut app, fake) = app_with_fake();
    ask_to_delete_feat(&mut app);
    ch(&mut app, 'y');
    assert!(app.confirm_message().is_none());
    assert!(fake.calls().contains(&"delete_branch"));
}

#[test]
fn a_popup_blocks_the_key_that_would_ask_a_question() {
    let (mut app, _) = app_with_fake();
    ch(&mut app, 'c');
    assert!(app.commit_popup().is_some());
    ch(&mut app, 'd'); // typed into the message, not a command
    assert!(app.confirm_message().is_none());
    assert!(app.commit_popup().is_some(), "the popup is still up");
}

#[test]
fn esc_goes_to_the_popup_before_the_toast() {
    let fake = FakeGit::new("overlay")
        .with_commit("init")
        .with_file("a.txt", Change::Modified, Change::None)
        .fail_next("commit", GitError::CommitFailed("hook said no".to_owned()));
    let mut app = App::with_git(Box::new(fake));
    ch(&mut app, 'c');
    for c in "feat: x".chars() {
        ch(&mut app, c);
    }
    press(&mut app, KeyCode::Enter);
    settle(&mut app);
    assert!(
        app.commit_popup().is_some(),
        "the popup stays after a failure"
    );

    // The toast only takes Esc when no popup or question needs it.
    press(&mut app, KeyCode::Esc);
    assert!(
        app.commit_popup().is_none(),
        "the first Esc closed the popup"
    );
}

#[test]
fn a_sheet_owns_the_keys_over_the_panes_until_it_has_closed() {
    let (mut app, _) = app_with_fake();
    ch(&mut app, 'D');
    assert!(app.sheet_is_open());

    ch(&mut app, 'c');
    assert!(app.commit_popup().is_none(), "the sheet took the key");

    press(&mut app, KeyCode::Esc);
    settle(&mut app);
    assert!(!app.sheet_is_open());
    ch(&mut app, 'c');
    assert!(app.commit_popup().is_some(), "the panes have the keys back");
}

#[test]
fn a_question_over_the_welcome_screen_leaves_the_welcome_screen_up() {
    let dir = std::env::temp_dir().join(format!("ferrit-overlay-{}", std::process::id()));
    fs::create_dir_all(&dir).unwrap();
    let mut app = App::welcome(&dir, ConfigLoad::default());
    assert_eq!(app.full_screen(), FullScreen::Welcome);

    ch(&mut app, 'i');
    assert!(app.confirm_message().is_some(), "git init asks first");
    assert_eq!(app.full_screen(), FullScreen::Welcome);

    press(&mut app, KeyCode::Esc);
    assert!(app.confirm_message().is_none());
    assert_eq!(app.full_screen(), FullScreen::Welcome);
    assert!(!dir.join(".git").exists(), "nothing was created");
    let _ = fs::remove_dir_all(&dir);
}
