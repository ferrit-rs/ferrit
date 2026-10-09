#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "integration test scaffolding: a failed setup is the assertion"
)]
//! The welcome screen (`docs/PLAN_16_START_WITHOUT_REPO.md`, W2): ferrit in a
//! folder with no repository. Nothing is created without the yes to a question
//! that names the folder.

use std::fs;
use std::path::PathBuf;

use ferrit::app::App;
use ferrit::app::full_screens::FullScreen;
use ferrit::config::ConfigLoad;
use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = fs::canonicalize(std::env::temp_dir())
            .unwrap()
            .join(format!("ferrit-{tag}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn welcome(&self) -> App {
        App::welcome(&self.0, ConfigLoad::default())
    }

    fn has_repo(&self) -> bool {
        self.0.join(".git").exists()
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn key(c: char) -> KeyEvent {
    KeyEvent::from(KeyCode::Char(c))
}

fn press(app: &mut App, code: KeyCode) {
    app.feed_key(KeyEvent::from(code));
}

fn status_text(app: &App) -> String {
    app.status_lines()
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn it_starts_on_the_welcome_screen_about_the_absolute_folder() {
    let dir = TempDir::new("welcome-start");
    let app = dir.welcome();
    assert_eq!(app.full_screen(), FullScreen::Welcome);
    assert_eq!(app.welcome_dir(), Some(dir.0.as_path()));
    assert!(!dir.has_repo());
}

#[test]
fn q_and_esc_leave_without_creating_anything() {
    for code in [KeyCode::Char('q'), KeyCode::Esc] {
        let dir = TempDir::new("welcome-quit");
        let mut app = dir.welcome();
        assert!(!app.is_quitting());
        press(&mut app, code);
        assert!(app.is_quitting());
        assert!(!dir.has_repo());
    }
}

#[test]
fn i_asks_naming_the_folder_and_creates_nothing_yet() {
    let dir = TempDir::new("welcome-ask");
    let mut app = dir.welcome();
    app.feed_key(key('i'));

    let question = app.confirm_message().unwrap();
    assert_eq!(question, format!("run git init in {}?", dir.0.display()));
    assert!(!dir.has_repo());
    assert_eq!(app.full_screen(), FullScreen::Welcome);
}

#[test]
fn the_home_folder_is_called_out_in_the_question() {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return;
    };
    let Ok(home) = fs::canonicalize(home) else {
        return;
    };
    let mut app = App::welcome(&home, ConfigLoad::default());
    app.feed_key(key('i'));
    // Only the question is asked: the answer is never given, nothing is created.
    let question = app.confirm_message().unwrap();
    assert!(
        question.ends_with("This is your home folder."),
        "{question}"
    );
}

#[test]
fn n_and_esc_at_the_question_create_nothing_and_stay() {
    for answer in [KeyCode::Char('n'), KeyCode::Esc] {
        let dir = TempDir::new("welcome-no");
        let mut app = dir.welcome();
        app.feed_key(key('i'));
        press(&mut app, answer);
        assert!(app.confirm_message().is_none());
        assert!(!dir.has_repo());
        assert_eq!(app.full_screen(), FullScreen::Welcome);
        assert!(
            !app.is_quitting(),
            "an Esc that answers a question does not quit"
        );
    }
}

#[test]
fn y_and_enter_create_the_repository_and_open_the_panes() {
    for answer in [KeyCode::Char('y'), KeyCode::Enter] {
        let dir = TempDir::new("welcome-yes");
        fs::write(dir.0.join("notes.txt"), "hello").unwrap();
        let mut app = dir.welcome();
        app.feed_key(key('i'));
        press(&mut app, answer);

        assert!(dir.has_repo());
        assert_eq!(app.full_screen(), FullScreen::None);
        assert!(app.welcome_dir().is_none());
        assert!(
            status_text(&app).contains("welcome-yes"),
            "{}",
            status_text(&app)
        );
        assert!(
            app.file_lines()
                .iter()
                .any(|l| l.to_string().contains("notes.txt")),
            "the folder's file shows as untracked"
        );
    }
}

#[test]
fn a_refused_init_says_why_and_the_welcome_screen_stays() {
    let dir = TempDir::new("welcome-refused");
    let mut app = dir.welcome();
    app.feed_key(key('i'));
    // The folder disappears between the question and the yes.
    fs::remove_dir_all(&dir.0).unwrap();
    app.feed_key(key('y'));

    assert_eq!(app.full_screen(), FullScreen::Welcome);
    assert!(
        status_text(&app).contains("git init failed"),
        "{}",
        status_text(&app)
    );
    assert!(!dir.0.exists());
}

#[test]
fn no_pane_action_runs_without_a_repository() {
    let dir = TempDir::new("welcome-inert");
    let mut app = dir.welcome();
    for c in [
        'x', 'j', 'D', 'C', 'c', 'f', 'p', 'P', '?', '@', '1', 's', 'd',
    ] {
        app.feed_key(key(c));
        assert_eq!(app.full_screen(), FullScreen::Welcome, "{c}");
        assert!(app.confirm_message().is_none(), "{c}");
        assert!(!app.is_quitting(), "{c}");
    }
    app.feed_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
    assert!(app.is_quitting(), "Ctrl-C still quits, as everywhere");
}

#[test]
fn the_mouse_does_nothing() {
    let dir = TempDir::new("welcome-mouse");
    let mut app = dir.welcome();
    for kind in [
        MouseEventKind::Down(MouseButton::Left),
        MouseEventKind::Down(MouseButton::Right),
        MouseEventKind::ScrollDown,
    ] {
        app.feed_mouse(MouseEvent {
            kind,
            column: 10,
            row: 5,
            modifiers: KeyModifiers::NONE,
        });
    }
    assert_eq!(app.full_screen(), FullScreen::Welcome);
    assert!(app.confirm_message().is_none());
}

// ------------------------------------------------------- how ferrit starts

#[test]
fn with_no_path_a_folder_without_a_repository_opens_the_welcome_screen() {
    let dir = TempDir::new("welcome-start-implicit");
    let app = App::open_or_welcome(&dir.0, false, ConfigLoad::default()).unwrap();
    assert_eq!(app.full_screen(), FullScreen::Welcome);
    assert_eq!(app.welcome_dir(), Some(dir.0.as_path()));
}

#[test]
fn a_path_named_on_purpose_keeps_the_error() {
    let dir = TempDir::new("welcome-start-explicit");
    let err = App::open_or_welcome(&dir.0, true, ConfigLoad::default())
        .err()
        .unwrap();
    assert!(
        matches!(err, ferrit::git::error::GitError::NotARepository(_)),
        "{err:?}"
    );
}

#[test]
fn a_folder_with_a_repository_opens_it_either_way() {
    let dir = TempDir::new("welcome-start-repo");
    ferrit::git::repo::Repo::init(&dir.0).unwrap();
    for explicit in [false, true] {
        let app = App::open_or_welcome(&dir.0, explicit, ConfigLoad::default()).unwrap();
        assert_eq!(app.full_screen(), FullScreen::None, "explicit: {explicit}");
        assert!(app.welcome_dir().is_none());
    }
}

#[test]
fn a_folder_below_a_repository_opens_that_repository() {
    let dir = TempDir::new("welcome-start-below");
    ferrit::git::repo::Repo::init(&dir.0).unwrap();
    let below = dir.0.join("deep/er");
    fs::create_dir_all(&below).unwrap();
    let app = App::open_or_welcome(&below, false, ConfigLoad::default()).unwrap();
    assert_eq!(
        app.full_screen(),
        FullScreen::None,
        "no welcome inside a repository"
    );
}

#[test]
fn the_injected_gh_survives_the_git_init() {
    use ferrit::app::views::PopupView;
    use ferrit::git::host::GhProgram;

    let dir = TempDir::new("welcome-gh");
    let mut app = dir.welcome();
    app.set_gh_program(GhProgram::new("/nonexistent/ferrit-test/gh"));
    app.feed_key(key('i'));
    app.feed_key(key('y'));
    assert_eq!(app.full_screen(), FullScreen::None);

    // The rebuilt app still runs the injected program, not the real `gh`.
    app.open_create_remote();
    match app.popup_view() {
        Some(PopupView::Note(message)) => {
            assert!(message.contains("gh is required"), "{message}");
        },
        other => panic!("expected the missing-gh note, got {}", other.is_some()),
    }
}

// ------------------------------------------------- arrows and Enter

#[test]
fn enter_on_the_first_row_asks_about_git_init() {
    let dir = TempDir::new("welcome-enter-init");
    let mut app = dir.welcome();
    assert_eq!(app.welcome_selected(), 0, "git init is highlighted first");
    press(&mut app, KeyCode::Enter);
    assert_eq!(
        app.confirm_message(),
        Some(format!("run git init in {}?", dir.0.display()).as_str())
    );
    assert!(!dir.has_repo(), "asked, not done");
    assert!(!app.is_quitting());
}

#[test]
fn down_then_enter_quits() {
    for down in [KeyCode::Down, KeyCode::Char('j')] {
        let dir = TempDir::new("welcome-down-enter");
        let mut app = dir.welcome();
        press(&mut app, down);
        assert_eq!(app.welcome_selected(), 1);
        press(&mut app, KeyCode::Enter);
        assert!(app.is_quitting());
        assert!(!dir.has_repo());
    }
}

#[test]
fn up_comes_back_and_the_highlight_stops_at_both_ends() {
    let dir = TempDir::new("welcome-ends");
    let mut app = dir.welcome();
    press(&mut app, KeyCode::Up);
    assert_eq!(app.welcome_selected(), 0, "no row above the first");
    for _ in 0..3 {
        press(&mut app, KeyCode::Down);
    }
    assert_eq!(app.welcome_selected(), 1, "no row below the last");
    press(&mut app, KeyCode::Char('k'));
    assert_eq!(app.welcome_selected(), 0);
    press(&mut app, KeyCode::End);
    assert_eq!(app.welcome_selected(), 1);
    press(&mut app, KeyCode::Home);
    assert_eq!(app.welcome_selected(), 0);
    assert_eq!(app.full_screen(), FullScreen::Welcome);
}

#[test]
fn the_letters_still_work_from_any_highlighted_row() {
    let dir = TempDir::new("welcome-letters");
    let mut app = dir.welcome();
    press(&mut app, KeyCode::Down);
    app.feed_key(key('i'));
    assert!(
        app.confirm_message().is_some(),
        "i asks even with quit highlighted"
    );
    press(&mut app, KeyCode::Char('n'));
    app.feed_key(key('q'));
    assert!(app.is_quitting());
}

#[test]
fn arrows_then_enter_then_enter_walks_the_whole_init() {
    let dir = TempDir::new("welcome-keyboard-only");
    let mut app = dir.welcome();
    press(&mut app, KeyCode::Down);
    press(&mut app, KeyCode::Up);
    press(&mut app, KeyCode::Enter);
    assert!(app.confirm_message().is_some());
    assert!(!dir.has_repo());
    press(&mut app, KeyCode::Enter);
    assert!(dir.has_repo(), "Enter answers the question like y");
    assert_eq!(app.full_screen(), FullScreen::None);
}

#[test]
fn the_arrows_move_nothing_while_the_question_is_up() {
    let dir = TempDir::new("welcome-question-arrows");
    let mut app = dir.welcome();
    app.feed_key(key('i'));
    press(&mut app, KeyCode::Down);
    assert_eq!(app.welcome_selected(), 0, "the question owns the keys");
    assert!(app.confirm_message().is_some());
}
