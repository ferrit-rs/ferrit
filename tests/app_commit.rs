#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::pathbuf_init_then_push,
    clippy::iter_on_single_items,
    clippy::format_collect,
    elided_lifetimes_in_paths,
    reason = "integration test scaffolding: a failed setup is the assertion, helper ergonomics beat lint-cleanliness here"
)]
//! `App`-level wiring for the commit popup (`docs/PLAN_7_COMMIT.md`): `c`
//! opens it, typing fills the draft, `Enter` commits and refreshes, `Esc`
//! cancels but keeps the draft for the next `c`. `A` / `w` pre-fill from
//! `HEAD`'s message.

mod common;

use common::{TempDir, commit_all};
use std::fs;
use std::path::Path;
use std::process::Command;

use ferrit::tui::App;
use ferrit::tui::components::diff::{DiffView, PopupView};
use ferrit::tui::components::panes::Pane;
use git2::Repository;
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::style::Color;

fn configure_identity(dir: &Path) {
    for (key, value) in [
        ("user.name", "Max Wells"),
        ("user.email", "maxwells.pro@proton.me"),
    ] {
        let out = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["config", key, value])
            .output()
            .unwrap();
        assert!(out.status.success());
    }
}

fn char_key(c: char) -> KeyEvent {
    KeyEvent::from(KeyCode::Char(c))
}

fn ctrl_key(c: char) -> KeyEvent {
    KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
}

fn type_text(app: &mut App, text: &str) {
    for c in text.chars() {
        app.feed_key(char_key(c));
    }
}

fn head_summary(app: &mut App) -> String {
    app.select(Pane::Commits, 0);
    match app.diff_view() {
        DiffView::Commit(entry, _) => entry.summary.clone(),
        other => panic!("expected the newest commit's diff, got {other:?}"),
    }
}

#[test]
fn c_opens_types_and_enter_commits() {
    let dir = TempDir::new("app-commit-basic");
    let repo = Repository::init(dir.path()).unwrap();
    configure_identity(dir.path());
    fs::write(dir.path().join("a.txt"), "one\n").unwrap();
    commit_all(&repo, "init");
    fs::write(dir.path().join("a.txt"), "one\ntwo\n").unwrap();
    Command::new("git")
        .arg("-C")
        .arg(dir.path())
        .args(["add", "a.txt"])
        .output()
        .unwrap();

    let mut app = App::open(dir.path()).unwrap();
    let before = app.row_count(Pane::Commits);

    app.feed_key(char_key('c'));
    assert!(app.commit_popup().is_some(), "c opened the popup");
    type_text(&mut app, "feat: a new line");
    app.feed_key(KeyEvent::from(KeyCode::Enter));

    assert!(app.commit_popup().is_none(), "popup closed on success");
    assert!(app.note_popup().is_none());
    assert_eq!(
        app.row_count(Pane::Commits),
        before + 1,
        "the new commit showed up"
    );
    assert_eq!(head_summary(&mut app), "feat: a new line");
}

#[test]
fn the_new_commit_is_the_selected_row_whichever_row_the_cursor_was_on() {
    let dir = TempDir::new("app-commit-selects");
    let repo = Repository::init(dir.path()).unwrap();
    configure_identity(dir.path());
    fs::write(dir.path().join("a.txt"), "one\n").unwrap();
    commit_all(&repo, "init");
    fs::write(dir.path().join("a.txt"), "one\ntwo\n").unwrap();
    commit_all(&repo, "second");
    fs::write(dir.path().join("a.txt"), "one\ntwo\nthree\n").unwrap();
    Command::new("git")
        .arg("-C")
        .arg(dir.path())
        .args(["add", "a.txt"])
        .output()
        .unwrap();

    let mut app = App::open(dir.path()).unwrap();
    app.select(Pane::Commits, 1); // an older commit, not the top one
    app.select(Pane::Files, 0);
    app.feed_key(char_key('c'));
    type_text(&mut app, "feat: third");
    app.feed_key(KeyEvent::from(KeyCode::Enter));

    assert_eq!(
        app.selected(Pane::Commits),
        0,
        "the commit just made is the selected row"
    );
}

#[test]
fn tab_switches_to_body_and_ctrl_enter_commits_message() {
    let dir = TempDir::new("app-commit-body");
    let repo = Repository::init(dir.path()).unwrap();
    configure_identity(dir.path());
    fs::write(dir.path().join("a.txt"), "one\n").unwrap();
    commit_all(&repo, "init");
    fs::write(dir.path().join("a.txt"), "one\ntwo\n").unwrap();
    Command::new("git")
        .arg("-C")
        .arg(dir.path())
        .args(["add", "a.txt"])
        .output()
        .unwrap();

    let mut app = App::open(dir.path()).unwrap();
    app.feed_key(char_key('c'));
    type_text(&mut app, "feat: summary");
    app.feed_key(KeyEvent::from(KeyCode::Tab));
    type_text(&mut app, "body line one");
    app.feed_key(KeyEvent::from(KeyCode::Enter));
    type_text(&mut app, "body line two");
    app.feed_key(KeyEvent::new(KeyCode::Enter, KeyModifiers::CONTROL));

    let head = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(
        head.message().unwrap(),
        "feat: summary\n\nbody line one\nbody line two\n"
    );
}

#[test]
fn esc_cancels_but_keeps_the_draft_for_next_time() {
    let dir = TempDir::new("app-commit-draft");
    let repo = Repository::init(dir.path()).unwrap();
    configure_identity(dir.path());
    fs::write(dir.path().join("a.txt"), "one\n").unwrap();
    commit_all(&repo, "init");
    fs::write(dir.path().join("a.txt"), "one\ntwo\n").unwrap();
    Command::new("git")
        .arg("-C")
        .arg(dir.path())
        .args(["add", "a.txt"])
        .output()
        .unwrap();

    let mut app = App::open(dir.path()).unwrap();

    app.feed_key(char_key('c'));
    type_text(&mut app, "wip: half a thought");
    app.feed_key(KeyEvent::from(KeyCode::Esc));
    assert!(app.commit_popup().is_none(), "Esc closed the popup");

    app.feed_key(char_key('c'));
    let view = app.commit_popup().expect("c reopened the popup");
    assert_eq!(
        view.lines.join("\n"),
        "wip: half a thought",
        "draft came back"
    );
}

#[test]
fn empty_message_is_rejected_and_popup_stays_open() {
    let dir = TempDir::new("app-commit-empty");
    let repo = Repository::init(dir.path()).unwrap();
    configure_identity(dir.path());
    fs::write(dir.path().join("a.txt"), "one\n").unwrap();
    commit_all(&repo, "init");
    fs::write(dir.path().join("a.txt"), "one\ntwo\n").unwrap();
    Command::new("git")
        .arg("-C")
        .arg(dir.path())
        .args(["add", "a.txt"])
        .output()
        .unwrap();

    let mut app = App::open(dir.path()).unwrap();
    let before = app.row_count(Pane::Commits);

    app.feed_key(char_key('c'));
    app.feed_key(ctrl_key('s'));

    assert!(
        app.commit_popup().is_some(),
        "popup stays open for correction"
    );
    assert!(
        app.status_lines()[0]
            .to_string()
            .contains("cannot be empty"),
        "empty message error is shown in Status"
    );
    assert_eq!(app.row_count(Pane::Commits), before, "no commit was made");
}

#[test]
fn the_popup_actually_renders_its_title_text_and_footer() {
    let dir = TempDir::new("app-commit-render");
    let repo = Repository::init(dir.path()).unwrap();
    configure_identity(dir.path());
    fs::write(dir.path().join("a.txt"), "one\n").unwrap();
    commit_all(&repo, "init");
    fs::write(dir.path().join("a.txt"), "one\ntwo\n").unwrap();
    Command::new("git")
        .arg("-C")
        .arg(dir.path())
        .args(["add", "a.txt"])
        .output()
        .unwrap();

    let mut app = App::open(dir.path()).unwrap();
    app.feed_key(char_key('c'));
    type_text(&mut app, "feat: rendered");

    let mut term = Terminal::new(TestBackend::new(120, 40)).unwrap();
    term.draw(|f| ferrit::tui::draw::draw(f, &mut app)).unwrap();
    let out = term.backend().to_string();

    assert!(out.contains("Commit"), "popup title shows:\n{out}");
    assert!(out.contains("feat: rendered"), "typed text shows:\n{out}");
    assert!(out.contains("Enter: commit"), "footer hints show:\n{out}");
    assert!(out.contains("sign-off"), "toggle status shows:\n{out}");
}

#[test]
fn c_with_empty_index_prompts_and_n_cancels() {
    let dir = TempDir::new("app-commit-nostage");
    let repo = Repository::init(dir.path()).unwrap();
    configure_identity(dir.path());
    fs::write(dir.path().join("a.txt"), "one\n").unwrap();
    commit_all(&repo, "init");

    let mut app = App::open(dir.path()).unwrap();
    app.feed_key(char_key('c'));
    assert!(matches!(
        app.popup_view(),
        Some(PopupView::CommitAllConfirm(_))
    ));
    app.feed_key(char_key('n'));
    assert!(app.commit_popup().is_none(), "n cancels the confirmation");
}

#[test]
fn y_stages_tracked_and_untracked_changes_then_opens_commit_editor() {
    let dir = TempDir::new("app-commit-all");
    let repo = Repository::init(dir.path()).unwrap();
    configure_identity(dir.path());
    fs::write(dir.path().join("a.txt"), "one\n").unwrap();
    commit_all(&repo, "init");
    fs::write(dir.path().join("a.txt"), "one\ntwo\n").unwrap();
    fs::write(dir.path().join("new.txt"), "new file\n").unwrap();

    let mut app = App::open(dir.path()).unwrap();
    app.feed_key(char_key('c'));
    assert!(matches!(
        app.popup_view(),
        Some(PopupView::CommitAllConfirm(_))
    ));
    app.feed_key(char_key('y'));
    assert!(app.commit_popup().is_some(), "y opens the commit editor");

    let mut index = repo.index().unwrap();
    index.read(true).unwrap();
    assert!(index.get_path(Path::new("a.txt"), 0).is_some());
    assert!(index.get_path(Path::new("new.txt"), 0).is_some());

    type_text(&mut app, "feat: commit all");
    app.feed_key(KeyEvent::from(KeyCode::Enter));
    let head = repo.head().unwrap().peel_to_commit().unwrap();
    assert_eq!(head.summary().unwrap(), Some("feat: commit all"));
    assert_eq!(head.author().name().unwrap(), "Max Wells");
    assert_eq!(head.author().email().unwrap(), "maxwells.pro@proton.me");
    let tree = head.tree().unwrap();
    assert!(tree.get_path(Path::new("a.txt")).is_ok());
    assert!(tree.get_path(Path::new("new.txt")).is_ok());
}

#[test]
fn empty_index_confirmation_uses_overlay_backdrop() {
    let dir = TempDir::new("app-commit-confirm-render");
    let repo = Repository::init(dir.path()).unwrap();
    configure_identity(dir.path());
    fs::write(dir.path().join("a.txt"), "one\n").unwrap();
    commit_all(&repo, "init");

    let mut app = App::open(dir.path()).unwrap();
    app.feed_key(char_key('c'));
    let mut term = Terminal::new(TestBackend::new(120, 40)).unwrap();
    term.draw(|f| ferrit::tui::draw::draw(f, &mut app)).unwrap();
    let out = term.backend().to_string();

    assert!(out.contains("No files staged"), "heading shows:\n{out}");
    assert!(
        out.contains("You have not staged any files."),
        "question shows:\n{out}"
    );
    assert!(out.contains("Y: Yes, stage all"), "choices show:\n{out}");
    assert_eq!(term.backend().buffer()[(0, 0)].fg, Color::DarkGray);
    assert_eq!(term.backend().buffer()[(0, 0)].bg, Color::Black);
}

#[test]
fn amend_prefills_the_current_message_and_keeps_the_parent() {
    let dir = TempDir::new("app-commit-amend");
    let repo = Repository::init(dir.path()).unwrap();
    configure_identity(dir.path());
    fs::write(dir.path().join("a.txt"), "one\n").unwrap();
    commit_all(&repo, "base");
    fs::write(dir.path().join("b.txt"), "two\n").unwrap();
    commit_all(&repo, "feat: wip message");

    let mut app = App::open(dir.path()).unwrap();
    app.feed_key(KeyEvent::from(KeyCode::Char('A')));
    let view = app.commit_popup().expect("A opened the popup");
    assert_eq!(view.title, "Amend HEAD");
    assert_eq!(view.lines.join("\n"), "feat: wip message");

    // Replace the prefilled text with a fixed one: backspace it all out,
    // then type the real message.
    for _ in 0.."feat: wip message".chars().count() {
        app.feed_key(KeyEvent::from(KeyCode::Backspace));
    }
    type_text(&mut app, "feat: the real message");
    app.feed_key(ctrl_key('s'));

    assert_eq!(head_summary(&mut app), "feat: the real message");
    assert_eq!(
        app.row_count(Pane::Commits),
        2,
        "amend did not add a commit"
    );
}

/// A repo with one staged change and `commit.template` set to `template`.
fn staged_repo_with_template(tag: &str, template: &str) -> TempDir {
    let dir = TempDir::new(tag);
    let repo = Repository::init(dir.path()).unwrap();
    configure_identity(dir.path());
    fs::write(dir.path().join("a.txt"), "one\n").unwrap();
    commit_all(&repo, "init");
    fs::write(dir.path().join("a.txt"), "one\ntwo\n").unwrap();
    fs::write(dir.path().join("tpl.txt"), template).unwrap();
    for args in [
        vec!["config", "commit.template", "tpl.txt"],
        vec!["add", "a.txt"],
    ] {
        let out = Command::new("git")
            .arg("-C")
            .arg(dir.path())
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success());
    }
    dir
}

#[test]
fn a_new_commit_starts_from_the_template_split_into_subject_and_body() {
    let dir = staged_repo_with_template("app-commit-template", "feat: \n\nWhy:\n# comment\n");
    let mut app = App::open(dir.path()).unwrap();
    app.feed_key(char_key('c'));
    let view = app.commit_popup().expect("c opened the popup");
    assert_eq!(view.lines.join("\n"), "feat: ", "the subject line");
    assert_eq!(
        view.description.unwrap().text(),
        "Why:",
        "the body, comment gone"
    );
}

#[test]
fn a_kept_draft_wins_over_the_template_and_amend_ignores_it() {
    let dir = staged_repo_with_template("app-commit-template-draft", "template subject\n");
    let mut app = App::open(dir.path()).unwrap();
    app.feed_key(char_key('c'));
    for _ in 0.."template subject".chars().count() {
        app.feed_key(KeyEvent::from(KeyCode::Backspace));
    }
    type_text(&mut app, "my draft");
    app.feed_key(KeyEvent::from(KeyCode::Esc));

    app.feed_key(char_key('c'));
    let view = app.commit_popup().expect("reopened");
    assert_eq!(view.lines.join("\n"), "my draft");
    app.feed_key(KeyEvent::from(KeyCode::Esc));

    app.feed_key(KeyEvent::from(KeyCode::Char('A')));
    let view = app.commit_popup().expect("A opened the popup");
    assert_eq!(view.lines.join("\n"), "init", "amend shows HEAD's message");
}

#[test]
fn a_template_commit_is_committed_as_written_when_confirmed() {
    let dir = staged_repo_with_template("app-commit-template-commit", "chore: from template\n");
    let mut app = App::open(dir.path()).unwrap();
    app.feed_key(char_key('c'));
    app.feed_key(KeyEvent::from(KeyCode::Enter));
    assert_eq!(head_summary(&mut app), "chore: from template");
}

#[test]
fn the_template_also_fills_the_editor_reached_through_stage_all() {
    let dir = staged_repo_with_template("app-commit-template-all", "feat: via stage all\n");
    let reset = Command::new("git")
        .arg("-C")
        .arg(dir.path())
        .args(["reset", "-q"])
        .output()
        .unwrap();
    assert!(reset.status.success());
    let mut app = App::open(dir.path()).unwrap();
    app.feed_key(char_key('c')); // nothing staged: asks first
    app.feed_key(char_key('y'));
    let view = app.commit_popup().expect("the editor opened after y");
    assert_eq!(view.lines.join("\n"), "feat: via stage all");
}

/// The `n/50` counter cell colours: the counter's own text, from a frame drawn
/// after typing `subject`.
fn counter_colour(dir: &Path, subject: &str) -> (bool, Option<Color>) {
    let mut app = App::open(dir).unwrap();
    app.feed_key(char_key('c'));
    type_text(&mut app, subject);
    let mut term = Terminal::new(TestBackend::new(140, 40)).unwrap();
    term.draw(|f| ferrit::tui::draw::draw(f, &mut app)).unwrap();
    let buffer = term.backend().buffer();
    let label = format!(" {}/50 ", subject.chars().count());
    let width = usize::from(buffer.area.width);
    let text: Vec<String> = buffer
        .content
        .chunks(width)
        .map(|row| row.iter().map(ratatui::buffer::Cell::symbol).collect())
        .collect();
    for (y, line) in text.iter().enumerate() {
        if let Some(byte) = line.find(&label) {
            let x = line[..byte].chars().count() + 1;
            return (true, Some(buffer.content[y * width + x].fg));
        }
    }
    (false, None)
}

#[test]
fn the_summary_counts_its_length_and_warns_past_50() {
    let dir = staged_repo_with_template("app-commit-counter", "\n");
    // `commit.template` is a blank line: the editor starts empty.
    let palette = ferrit::theme::palette::Palette::DARK;
    let at_limit = "a".repeat(50);
    let over = "a".repeat(51);
    assert_eq!(
        counter_colour(dir.path(), "feat: x"),
        (true, Some(palette.idle))
    );
    assert_eq!(
        counter_colour(dir.path(), &at_limit),
        (true, Some(palette.idle))
    );
    assert_eq!(
        counter_colour(dir.path(), &over),
        (true, Some(palette.warn))
    );
}

#[test]
fn a_too_long_subject_can_still_be_committed() {
    let dir = staged_repo_with_template("app-commit-counter-nonblocking", "\n");
    let subject = "b".repeat(80);
    let mut app = App::open(dir.path()).unwrap();
    app.feed_key(char_key('c'));
    type_text(&mut app, &subject);
    app.feed_key(KeyEvent::from(KeyCode::Enter));
    assert!(app.commit_popup().is_none(), "no block, only a colour");
    assert_eq!(head_summary(&mut app), subject);
}

fn screen_text(app: &mut App) -> String {
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    terminal.draw(|f| ferrit::tui::draw::draw(f, app)).unwrap();
    let buf = terminal.backend().buffer().clone();
    (0..buf.area.height)
        .map(|y| {
            (0..buf.area.width)
                .map(|x| buf[(x, y)].symbol())
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// A repository with one staged change and its own identity, an app on it that
/// knows two global identities.
fn app_with_identities(tag: &str) -> (TempDir, App) {
    let dir = TempDir::new(tag);
    let repo = Repository::init(dir.path()).unwrap();
    configure_identity(dir.path());
    fs::write(dir.path().join("a.txt"), "one\n").unwrap();
    commit_all(&repo, "init");
    fs::write(dir.path().join("a.txt"), "one\ntwo\n").unwrap();
    Command::new("git")
        .arg("-C")
        .arg(dir.path())
        .args(["add", "a.txt"])
        .output()
        .unwrap();
    let mut app = App::open(dir.path()).unwrap();
    app.set_global_identities(vec![
        ("Ada Work".to_owned(), "ada@work.example".to_owned()),
        ("Ada Home".to_owned(), "ada@home.example".to_owned()),
    ]);
    (dir, app)
}

#[test]
fn the_commit_popup_names_the_author_and_ctrl_a_cycles_the_identities_then_git_own() {
    let (_dir, mut app) = app_with_identities("app-commit-author");
    app.feed_key(char_key('c'));
    let text = screen_text(&mut app);
    assert!(
        text.contains("author: git's own (Max Wells <maxwells.pro@proton.me>)"),
        "{text}"
    );
    assert!(
        text.contains("Ctrl-O/N/A"),
        "the hint names the key: {text}"
    );

    app.feed_key(ctrl_key('a'));
    assert!(screen_text(&mut app).contains("author: Ada Work <ada@work.example>"));
    app.feed_key(ctrl_key('a'));
    assert!(screen_text(&mut app).contains("author: Ada Home <ada@home.example>"));
    app.feed_key(ctrl_key('a'));
    assert!(screen_text(&mut app).contains("author: git's own (Max Wells"));
}

#[test]
fn a_commit_is_by_the_chosen_author_and_the_choice_lasts_for_the_next_commit_of_the_run() {
    let (dir, mut app) = app_with_identities("app-commit-by");
    app.feed_key(char_key('c'));
    app.feed_key(ctrl_key('a'));
    type_text(&mut app, "by ada");
    app.feed_key(KeyEvent::from(KeyCode::Enter));

    let log = |format: &str| {
        let out = Command::new("git")
            .arg("-C")
            .arg(dir.path())
            .args(["log", "-1", &format!("--format={format}")])
            .output()
            .unwrap();
        String::from_utf8(out.stdout).unwrap().trim().to_owned()
    };
    assert_eq!(log("%an <%ae>"), "Ada Work <ada@work.example>");
    assert_eq!(
        log("%cn <%ce>"),
        "Max Wells <maxwells.pro@proton.me>",
        "the committer stays git's own"
    );

    fs::write(dir.path().join("a.txt"), "one\ntwo\nthree\n").unwrap();
    Command::new("git")
        .arg("-C")
        .arg(dir.path())
        .args(["add", "a.txt"])
        .output()
        .unwrap();
    app.refresh();
    app.feed_key(char_key('c'));
    assert!(
        screen_text(&mut app).contains("author: Ada Work"),
        "still chosen for this run"
    );
    // The choice never touches git's config.
    let configured = Command::new("git")
        .arg("-C")
        .arg(dir.path())
        .args(["config", "user.name"])
        .output()
        .unwrap();
    assert_eq!(
        String::from_utf8(configured.stdout).unwrap().trim(),
        "Max Wells"
    );
}

#[test]
fn without_a_global_identity_ctrl_a_has_nothing_to_cycle() {
    let (_dir, mut app) = app_with_identities("app-commit-noident");
    app.set_global_identities(Vec::new());
    app.feed_key(char_key('c'));
    app.feed_key(ctrl_key('a'));
    assert!(screen_text(&mut app).contains("author: git's own (Max Wells"));
}
