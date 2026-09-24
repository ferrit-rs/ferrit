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
//! History rewrites from the Commits pane (`docs/PLAN_11_REBASE.md` R4):
//! `w` reword, `d` drop (asks), `s` squash, `S` fixup, `e` edit.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use ferrit::app::screens as ui;
use ferrit::app::{App, Pane};
use git2::{IndexAddOption, Repository, Signature};
use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::crossterm::event::{KeyCode, KeyEvent};

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let mut path = std::env::temp_dir();
        path.push(format!("ferrit-{tag}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn configure_identity(dir: &Path) {
    for (key, value) in [("user.name", "Test"), ("user.email", "test@example.com")] {
        let out = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["config", key, value])
            .output()
            .unwrap();
        assert!(out.status.success());
    }
}

fn commit_all(repo: &Repository, message: &str) {
    let mut index = repo.index().unwrap();
    index
        .add_all(["*"].iter(), IndexAddOption::DEFAULT, None)
        .unwrap();
    index.write().unwrap();
    let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
    let sig = Signature::now("Test", "test@example.com").unwrap();
    let parent = repo
        .head()
        .ok()
        .and_then(|h| h.target())
        .and_then(|oid| repo.find_commit(oid).ok());
    let parents: Vec<&git2::Commit> = parent.iter().collect();
    repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)
        .unwrap();
}

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

fn char_key(c: char) -> KeyEvent {
    KeyEvent::from(KeyCode::Char(c))
}

fn history(tag: &str) -> TempDir {
    let dir = TempDir::new(tag);
    let repo = Repository::init(dir.path()).unwrap();
    configure_identity(dir.path());
    git(dir.path(), &["checkout", "-q", "-b", "main"]);
    for content in ["base", "one", "two", "three"] {
        fs::write(dir.path().join("f"), format!("{content}\n")).unwrap();
        commit_all(&repo, content);
    }
    dir
}

fn status_lines(app: &App) -> Vec<String> {
    app.status_lines().iter().map(ToString::to_string).collect()
}

fn frame(app: &mut App) -> String {
    let mut terminal = Terminal::new(TestBackend::new(120, 40)).unwrap();
    terminal.draw(|f| ui::draw(f, app)).unwrap();
    terminal.backend().to_string()
}

fn commits_app(dir: &TempDir) -> App {
    let mut app = App::open(dir.path()).unwrap();
    app.feed_key(char_key('4')); // Commits: three, two, one, base
    app
}

fn subjects(dir: &TempDir) -> Vec<String> {
    git(dir.path(), &["log", "--format=%s"])
        .lines()
        .map(str::to_owned)
        .collect()
}

fn select_row(app: &mut App, row: usize) {
    app.select(Pane::Commits, row);
}

fn type_text(app: &mut App, text: &str) {
    for c in text.chars() {
        app.feed_key(char_key(c));
    }
}

fn enter(app: &mut App) {
    app.feed_key(KeyEvent::from(KeyCode::Enter));
}

#[test]
fn w_on_the_first_row_keeps_the_head_amend_path() {
    let dir = history("rw-app-head");
    let mut app = commits_app(&dir);
    app.feed_key(char_key('w'));

    let out = frame(&mut app);
    assert!(out.contains("Reword HEAD"), "{out}");
}

#[test]
fn w_on_an_older_commit_opens_a_prefilled_reword_and_rewrites_it() {
    let dir = history("rw-app-reword");
    let short = git(dir.path(), &["rev-parse", "--short", "HEAD~1"]);
    let mut app = commits_app(&dir);
    select_row(&mut app, 1);
    app.feed_key(char_key('w'));

    let out = frame(&mut app);
    assert!(out.contains(&format!("Reword {short}")), "{out}");
    assert!(
        out.contains("two"),
        "pre-filled with the commit's message:\n{out}"
    );
    assert!(
        !out.contains("sign-off"),
        "no sign-off / no-verify toggles for a rebase reword:\n{out}"
    );

    assert!(
        out.contains("Enter: reword"),
        "hints say reword, not commit:\n{out}"
    );
    assert!(!out.contains("Ctrl-O"), "no toggle hint:\n{out}");
    app.feed_key(KeyEvent::new(
        KeyCode::Char('o'),
        ratatui::crossterm::event::KeyModifiers::CONTROL,
    ));
    assert!(
        !frame(&mut app).contains("sign-off"),
        "Ctrl-O does nothing here"
    );

    type_text(&mut app, " edited");
    enter(&mut app);
    assert_eq!(subjects(&dir), ["three", "two edited", "one", "base"]);
    assert!(app.commit_popup().is_none(), "the popup closed");
    assert_eq!(app.row_count(Pane::Commits), 4);
}

#[test]
fn cancelling_an_older_reword_does_not_become_the_next_commit_draft() {
    let dir = history("rw-app-cancel");
    let mut app = commits_app(&dir);
    select_row(&mut app, 1);
    app.feed_key(char_key('w'));
    type_text(&mut app, " half typed");
    app.feed_key(KeyEvent::from(KeyCode::Esc));

    fs::write(dir.path().join("g"), "x\n").unwrap();
    git(dir.path(), &["add", "g"]);
    app.refresh();
    app.feed_key(char_key('c'));
    let draft = app.commit_popup().expect("the commit popup opened");
    assert_eq!(
        draft.lines.join(""),
        "",
        "an empty draft, not the reword's text"
    );
}

#[test]
fn a_refused_reword_keeps_the_popup_and_the_text() {
    let dir = history("rw-app-refused");
    fs::write(dir.path().join("f"), "local edit\n").unwrap(); // dirty worktree
    let mut app = commits_app(&dir);
    select_row(&mut app, 1);
    app.feed_key(char_key('w'));
    type_text(&mut app, "!");
    enter(&mut app);

    let view = app.commit_popup().expect("the popup stays for a retry");
    assert!(view.lines.join("").contains("two!"), "{:?}", view.lines);
    assert_eq!(subjects(&dir), ["three", "two", "one", "base"]);
    assert!(status_lines(&app).join("\n").contains("unstaged changes"));
}

#[test]
fn drop_asks_and_y_removes_a_commit_nothing_depends_on() {
    let dir = history("rw-app-drop");
    fs::write(dir.path().join("g"), "x\n").unwrap();
    git(dir.path(), &["add", "g"]);
    git(dir.path(), &["commit", "-qm", "extra"]);
    let mut app = commits_app(&dir);
    app.feed_key(char_key('d'));

    let prompt = app.confirm_message().expect("asks first").to_owned();
    assert!(
        prompt.starts_with("drop ") && prompt.contains("extra"),
        "{prompt}"
    );
    app.feed_key(char_key('n'));
    assert_eq!(subjects(&dir).len(), 5, "n keeps it");

    app.feed_key(char_key('d'));
    app.feed_key(char_key('y'));
    assert_eq!(subjects(&dir), ["three", "two", "one", "base"]);
    assert!(!dir.path().join("g").exists());
}

#[test]
fn a_drop_that_conflicts_stops_and_the_menu_abort_restores_the_branch() {
    let dir = history("rw-app-drop-conflict");
    let before = git(dir.path(), &["rev-parse", "HEAD"]);
    let mut app = commits_app(&dir);
    select_row(&mut app, 2); // `one`: `two` was written on top of it
    app.feed_key(char_key('d'));
    app.feed_key(char_key('y'));

    let note = app
        .note_popup()
        .expect("a note explains the stop")
        .to_owned();
    assert!(note.contains("stopped on a conflict"), "{note}");
    assert!(status_lines(&app).contains(&"REBASING 2/3".to_owned()));

    app.feed_key(KeyEvent::from(KeyCode::Enter)); // dismiss the note
    app.feed_key(char_key('m'));
    app.feed_key(char_key('a'));
    app.feed_key(char_key('y'));
    assert_eq!(git(dir.path(), &["rev-parse", "HEAD"]), before);
    assert!(!status_lines(&app).join("\n").contains("REBASING"));
}

#[test]
fn s_squashes_into_the_commit_below() {
    let dir = history("rw-app-squash");
    let mut app = commits_app(&dir);
    select_row(&mut app, 1);
    app.feed_key(char_key('s'));

    assert_eq!(subjects(&dir), ["three", "one", "base"]);
    let message = git(dir.path(), &["log", "-1", "--format=%B", "HEAD~1"]);
    assert!(
        message.contains("one") && message.contains("two"),
        "{message}"
    );
    assert_eq!(app.row_count(Pane::Commits), 3);
}

#[test]
fn capital_s_fixes_up_and_drops_the_message() {
    let dir = history("rw-app-fixup");
    let mut app = commits_app(&dir);
    select_row(&mut app, 1);
    app.feed_key(char_key('S'));

    assert_eq!(subjects(&dir), ["three", "one", "base"]);
    let message = git(dir.path(), &["log", "-1", "--format=%B", "HEAD~1"]);
    assert!(!message.contains("two"), "{message}");
}

#[test]
fn squashing_the_oldest_commit_reports_there_is_nothing_below() {
    let dir = history("rw-app-root");
    let mut app = commits_app(&dir);
    select_row(&mut app, 3);
    app.feed_key(char_key('s'));

    assert_eq!(subjects(&dir).len(), 4);
    assert!(status_lines(&app).join("\n").contains("no commit below"));
}

#[test]
fn e_stops_at_the_commit_and_the_menu_continues() {
    let dir = history("rw-app-edit");
    let mut app = commits_app(&dir);
    select_row(&mut app, 1);
    app.feed_key(char_key('e'));

    let note = app
        .note_popup()
        .expect("a note explains the stop")
        .to_owned();
    assert!(note.contains("stopped for you to edit"), "{note}");
    assert!(status_lines(&app).contains(&"REBASING 1/2".to_owned()));

    enter(&mut app); // dismiss the note
    app.feed_key(char_key('m'));
    app.feed_key(char_key('c'));
    assert!(!status_lines(&app).join("\n").contains("REBASING"));
    assert_eq!(subjects(&dir), ["three", "two", "one", "base"]);
}

#[test]
fn rewrite_keys_say_why_while_an_operation_is_stopped() {
    let dir = history("rw-app-busy");
    let mut app = commits_app(&dir);
    select_row(&mut app, 1);
    app.feed_key(char_key('e')); // stops
    enter(&mut app);
    let before = git(dir.path(), &["rev-parse", "HEAD"]);

    app.feed_key(char_key('s'));
    assert!(
        status_lines(&app)
            .join("\n")
            .contains("finish or abort the operation"),
        "{:?}",
        status_lines(&app)
    );
    assert_eq!(
        git(dir.path(), &["rev-parse", "HEAD"]),
        before,
        "nothing was rewritten"
    );
}

#[test]
fn rewrite_keys_are_inert_inside_a_commit_and_on_other_panes() {
    let dir = history("rw-app-inert");
    let mut app = commits_app(&dir);
    select_row(&mut app, 1);
    enter(&mut app); // drill into the commit's files
    app.feed_key(char_key('s'));
    assert_eq!(subjects(&dir).len(), 4, "drilled: inert");
    app.feed_key(KeyEvent::from(KeyCode::Esc));

    app.feed_key(char_key('2')); // Files
    app.feed_key(char_key('s')); // stash, not squash: nothing to stash here
    app.feed_key(char_key('d')); // discard, not drop: no files
    assert_eq!(subjects(&dir).len(), 4);
    assert!(app.confirm_message().is_none());
}

#[test]
fn the_keybar_lists_the_commit_keys_only_on_the_commits_pane() {
    let dir = history("rw-app-keybar");
    let mut app = App::open(dir.path()).unwrap();
    assert!(!frame(&mut app).contains("Squash: s"));
    app.feed_key(char_key('4'));
    let out = frame(&mut app);
    assert!(
        out.contains("Reword: w") && out.contains("Squash: s"),
        "{out}"
    );
}

// ------------------------------------------------ F (fixup!) and a (autosquash)

fn stage_new_file(dir: &TempDir, name: &str) {
    fs::write(dir.path().join(name), "fix\n").unwrap();
    git(dir.path(), &["add", name]);
}

#[test]
fn capital_f_commits_what_is_staged_as_a_fixup_of_the_selected_commit() {
    let dir = history("rw-app-fixup-new");
    stage_new_file(&dir, "g");
    let mut app = commits_app(&dir);
    select_row(&mut app, 1); // `two`
    app.feed_key(char_key('F'));

    assert_eq!(
        subjects(&dir),
        ["fixup! two", "three", "two", "one", "base"]
    );
    assert!(git(dir.path(), &["show", "--stat", "--format=", "HEAD"]).contains('g'));
    assert_eq!(app.row_count(Pane::Commits), 5);
}

#[test]
fn capital_f_with_nothing_staged_is_an_error_and_makes_no_commit() {
    let dir = history("rw-app-fixup-empty");
    let mut app = commits_app(&dir);
    select_row(&mut app, 1);
    app.feed_key(char_key('F'));

    assert_eq!(subjects(&dir).len(), 4);
    assert!(
        status_lines(&app).join("\n").contains("nothing staged"),
        "{:?}",
        status_lines(&app)
    );
}

#[test]
fn a_folds_a_fixup_into_its_target() {
    let dir = history("rw-app-autosquash");
    stage_new_file(&dir, "g");
    let mut app = commits_app(&dir);
    select_row(&mut app, 2); // `one`
    app.feed_key(char_key('F'));
    select_row(&mut app, 3); // `one` again, now one row lower
    app.feed_key(char_key('a'));

    assert_eq!(subjects(&dir), ["three", "two", "one", "base"]);
    assert!(git(dir.path(), &["show", "--stat", "--format=", "HEAD~2"]).contains('g'));
    assert_eq!(app.row_count(Pane::Commits), 4);
}

#[test]
fn a_says_so_when_there_is_nothing_to_fold_and_rewrites_nothing() {
    let dir = history("rw-app-autosquash-none");
    let head = git(dir.path(), &["rev-parse", "HEAD"]);
    let mut app = commits_app(&dir);
    select_row(&mut app, 2);
    app.feed_key(char_key('a'));

    assert!(
        status_lines(&app).join("\n").contains("no fixup!"),
        "{:?}",
        status_lines(&app)
    );
    assert_eq!(git(dir.path(), &["rev-parse", "HEAD"]), head);
}

#[test]
fn a_ignores_a_fixup_whose_target_is_outside_the_range() {
    let dir = history("rw-app-autosquash-outside");
    stage_new_file(&dir, "g");
    let mut app = commits_app(&dir);
    select_row(&mut app, 3); // `base`, from where `one` is inside the range... 
    app.feed_key(char_key('F')); // fixup! base at the top
    let head = git(dir.path(), &["rev-parse", "HEAD"]);
    select_row(&mut app, 0); // range = the fixup alone: its target is not in it
    app.feed_key(char_key('a'));

    assert!(status_lines(&app).join("\n").contains("no fixup!"));
    assert_eq!(git(dir.path(), &["rev-parse", "HEAD"]), head);
}

#[test]
fn f_and_a_are_inert_off_the_commits_pane_and_a_still_stages_on_files() {
    let dir = history("rw-app-fa-inert");
    fs::write(dir.path().join("f"), "edited\n").unwrap();
    let mut app = App::open(dir.path()).unwrap();
    app.feed_key(char_key('2')); // Files
    app.feed_key(char_key('F'));
    assert_eq!(subjects(&dir).len(), 4);

    app.feed_key(char_key('a')); // stage all, as before
    assert!(git(dir.path(), &["diff", "--cached", "--name-only"]).contains('f'));
}

#[test]
fn the_commits_keybar_lists_f_and_a() {
    let dir = history("rw-app-keybar-fa");
    let mut app = commits_app(&dir);
    let out = frame(&mut app);
    assert!(
        out.contains("New fixup!: F") && out.contains("Autosquash: a"),
        "{out}"
    );
}
