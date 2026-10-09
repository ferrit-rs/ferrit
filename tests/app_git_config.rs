#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::pathbuf_init_then_push,
    reason = "integration test scaffolding: a failed setup is the assertion"
)]
//! The git config screen's state and keys (`docs/PLAN_14_GIT_CONFIG.md`, G3):
//! no drawing yet, so these tests read `App::git_config()`. The global file is
//! a throwaway one (`App::isolate_git_config`), never the user's.

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use ferrit::git::config::{Scope, WriteScope};
use ferrit::tui::App;
use ferrit::tui::components::diff::views::PopupView;
use ferrit::tui::draw::FullScreen;
use ratatui::crossterm::event::{KeyCode, KeyEvent};

fn key(c: char) -> KeyEvent {
    KeyEvent::from(KeyCode::Char(c))
}

fn press(app: &mut App, code: KeyCode) {
    app.feed_key(KeyEvent::from(code));
}

fn type_text(app: &mut App, text: &str) {
    for c in text.chars() {
        app.feed_key(key(c));
    }
}

struct Fixture {
    dir: PathBuf,
    global: PathBuf,
}

impl Fixture {
    fn new(tag: &str) -> Self {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("ferrit-{tag}-{}-{nanos}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let fixture = Self {
            global: dir.join("sandbox-global"),
            dir,
        };
        fixture.git(&["init", "-q", "."]);
        fixture.git(&["config", "--local", "pull.rebase", "merges"]);
        fixture.git(&["config", "--global", "pull.rebase", "true"]);
        fixture.git(&["config", "--global", "core.editor", "nvim"]);
        fixture.git(&["config", "--global", "github.token", "ghp_secret"]);
        fixture
    }

    fn git(&self, args: &[&str]) {
        let out = Command::new("git")
            .arg("-C")
            .arg(&self.dir)
            .args(args)
            .env("GIT_CONFIG_GLOBAL", &self.global)
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// An app on the fixture whose config calls stay in its sandbox.
    fn app(&self) -> App {
        let mut app = App::open(&self.dir).unwrap();
        app.isolate_git_config(&self.global);
        app
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn keys(app: &App) -> Vec<String> {
    app.git_config()
        .rows
        .iter()
        .map(|r| r.entry.key.clone())
        .collect()
}

#[test]
fn opening_lists_every_value_ordered_by_key_and_marks_the_winner() {
    let fx = Fixture::new("gc-open");
    let mut app = fx.app();
    app.open_git_config();

    assert_eq!(app.full_screen(), FullScreen::GitConfig);
    let screen = app.git_config();
    assert_eq!(screen.scope, WriteScope::Local);
    assert!(screen.total() >= 4);
    let mut sorted = keys(&app);
    sorted.sort();
    assert_eq!(keys(&app), sorted);

    let rebase: Vec<_> = screen
        .rows
        .iter()
        .filter(|r| r.entry.key == "pull.rebase")
        .collect();
    assert_eq!(rebase.len(), 2);
    assert_eq!(
        (rebase[0].entry.scope, rebase[0].shadowed, rebase[0].winner),
        (Scope::Global, true, false)
    );
    assert_eq!(
        (rebase[1].entry.scope, rebase[1].shadowed, rebase[1].winner),
        (Scope::Local, false, true)
    );
    let editor = screen
        .rows
        .iter()
        .find(|r| r.entry.key == "core.editor")
        .unwrap();
    assert!(!editor.shadowed && !editor.winner);
}

#[test]
fn navigation_filter_scope_and_close_keys() {
    let fx = Fixture::new("gc-keys");
    let mut app = fx.app();
    app.open_git_config();
    assert_eq!(app.git_config().selected, 0);

    app.feed_key(key('j'));
    assert_eq!(app.git_config().selected, 1);
    app.feed_key(key('k'));
    app.feed_key(key('k'));
    assert_eq!(app.git_config().selected, 0);
    press(&mut app, KeyCode::End);
    assert_eq!(app.git_config().selected, app.git_config().rows.len() - 1);
    press(&mut app, KeyCode::Home);
    assert_eq!(app.git_config().selected, 0);

    app.feed_key(key('s'));
    assert_eq!(app.git_config().scope, WriteScope::Global);
    app.feed_key(key('s'));
    assert_eq!(app.git_config().scope, WriteScope::Local);

    // `/` filters by key; the letters go to the filter, not to the bindings.
    app.feed_key(key('/'));
    type_text(&mut app, "pull");
    assert_eq!(keys(&app), ["pull.rebase", "pull.rebase"]);
    press(&mut app, KeyCode::Enter);
    assert!(!app.git_config().filtering);
    assert_eq!(app.git_config().filter, "pull");
    app.feed_key(key('/'));
    press(&mut app, KeyCode::Backspace);
    type_text(&mut app, "L");
    assert_eq!(app.git_config().filter, "pulL");
    press(&mut app, KeyCode::Esc);
    assert_eq!(app.git_config().filter, "");
    assert!(app.git_config().rows.len() > 2);

    // A kept filter is cleared by the first Esc; the second leaves.
    app.feed_key(key('/'));
    type_text(&mut app, "pull");
    press(&mut app, KeyCode::Enter);
    press(&mut app, KeyCode::Esc);
    assert_eq!(app.git_config().filter, "");
    assert_eq!(app.full_screen(), FullScreen::GitConfig);
    press(&mut app, KeyCode::Esc);
    assert_eq!(app.full_screen(), FullScreen::None);

    app.open_git_config();
    app.feed_key(key('q'));
    assert_eq!(app.full_screen(), FullScreen::None);
}

#[test]
fn a_secret_value_is_never_matched_by_the_filter() {
    let fx = Fixture::new("gc-secret");
    let mut app = fx.app();
    app.open_git_config();
    app.feed_key(key('/'));
    type_text(&mut app, "ghp_secret");
    assert!(app.git_config().rows.is_empty());
}

#[test]
fn a_reread_keeps_the_selection_on_the_same_value() {
    let fx = Fixture::new("gc-reread");
    let mut app = fx.app();
    app.open_git_config();
    let at = app
        .git_config()
        .rows
        .iter()
        .position(|r| r.entry.key == "core.editor")
        .unwrap();
    for _ in 0..at {
        app.feed_key(key('j'));
    }
    // Something else changes the file: a key that sorts before the selection.
    fx.git(&["config", "--local", "a.first", "x"]);
    app.feed_key(key('r'));
    let row = app.git_config().selected_row().unwrap();
    assert_eq!(row.entry.key, "core.editor");
    assert_eq!(app.git_config().selected, at + 1);
}

/// Move the selection onto the row of `key` at `scope`.
fn select(app: &mut App, key: &str, scope: Scope) {
    app.feed_key(KeyEvent::from(KeyCode::Home));
    let at = app
        .git_config()
        .rows
        .iter()
        .position(|r| r.entry.key == key && r.entry.scope == scope)
        .unwrap_or_else(|| panic!("no row {key} at {scope:?}"));
    for _ in 0..at {
        app.feed_key(key_j());
    }
}

fn key_j() -> KeyEvent {
    key('j')
}

fn popup_text(app: &App) -> Option<(String, String)> {
    match app.popup_view()? {
        PopupView::Name(view) => Some((view.title.to_owned(), view.lines.join("\n"))),
        PopupView::Menu(view) => Some((view.title, view.rows.join("\n"))),
        _ => None,
    }
}

fn get(fx: &Fixture, scope: &str, key: &str) -> Vec<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(&fx.dir)
        .args(["config", scope, "--get-all", key])
        .env("GIT_CONFIG_GLOBAL", &fx.global)
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_owned)
        .collect()
}

#[test]
fn space_flips_a_known_boolean_and_refuses_the_others() {
    let fx = Fixture::new("gc-bool");
    fx.git(&["config", "--local", "commit.gpgsign", "true"]);
    let mut app = fx.app();
    app.open_git_config();

    select(&mut app, "commit.gpgsign", Scope::Local);
    app.feed_key(key(' '));
    assert_eq!(get(&fx, "--local", "commit.gpgsign"), ["false"]);
    app.feed_key(key(' '));
    assert_eq!(get(&fx, "--local", "commit.gpgsign"), ["true"]);

    select(&mut app, "core.editor", Scope::Global);
    app.feed_key(key(' '));
    assert!(
        app.git_config()
            .note
            .as_deref()
            .unwrap()
            .contains("not a boolean")
    );
    assert_eq!(get(&fx, "--local", "core.editor"), Vec::<String>::new());
}

#[test]
fn enter_on_an_enum_opens_the_allowed_values_and_a_row_sets_it() {
    let fx = Fixture::new("gc-enum");
    let mut app = fx.app();
    app.open_git_config();

    select(&mut app, "pull.rebase", Scope::Local);
    press(&mut app, KeyCode::Enter);
    let (title, rows) = popup_text(&app).unwrap();
    assert_eq!(title, "pull.rebase (local)");
    assert_eq!(rows.lines().count(), 4);
    assert!(rows.contains("interactive"));
    // The current value (`merges`) is the highlighted row; `1` picks `false`.
    app.feed_key(key('1'));
    assert!(popup_text(&app).is_none());
    assert_eq!(get(&fx, "--local", "pull.rebase"), ["false"]);
    assert_eq!(
        app.git_config().selected_row().unwrap().entry.key,
        "pull.rebase"
    );
}

#[test]
fn a_text_value_is_edited_in_a_popup_in_the_write_scope_only() {
    let fx = Fixture::new("gc-text");
    let mut app = fx.app();
    app.open_git_config();

    select(&mut app, "core.editor", Scope::Global);
    press(&mut app, KeyCode::Enter);
    let (title, text) = popup_text(&app).unwrap();
    assert_eq!(
        (title.as_str(), text.as_str()),
        ("core.editor (local)", "nvim")
    );
    for _ in 0..4 {
        press(&mut app, KeyCode::Backspace);
    }
    type_text(&mut app, "vim -u NONE");
    press(&mut app, KeyCode::Enter);
    assert!(popup_text(&app).is_none());
    assert_eq!(get(&fx, "--local", "core.editor"), ["vim -u NONE"]);
    assert_eq!(get(&fx, "--global", "core.editor"), ["nvim"]);
    let rows = &app.git_config().rows;
    assert!(
        rows.iter()
            .any(|r| r.entry.key == "core.editor" && r.winner)
    );

    // Esc leaves the file alone.
    select(&mut app, "core.editor", Scope::Local);
    press(&mut app, KeyCode::Enter);
    type_text(&mut app, "x");
    press(&mut app, KeyCode::Esc);
    assert_eq!(get(&fx, "--local", "core.editor"), ["vim -u NONE"]);
}

#[test]
fn a_secret_value_is_replaced_without_ever_showing_the_old_one() {
    let fx = Fixture::new("gc-hide");
    let mut app = fx.app();
    app.open_git_config();

    select(&mut app, "github.token", Scope::Global);
    press(&mut app, KeyCode::Enter);
    let (title, text) = popup_text(&app).unwrap();
    assert!(!title.contains("ghp_secret") && !text.contains("ghp_secret"));
    assert_eq!(text, "");
    type_text(&mut app, "ghp_new");
    press(&mut app, KeyCode::Enter);
    assert_eq!(get(&fx, "--local", "github.token"), ["ghp_new"]);
}

#[test]
fn a_adds_a_key_and_a_second_value_when_the_key_is_already_set_there() {
    let fx = Fixture::new("gc-add");
    let mut app = fx.app();
    app.open_git_config();

    app.feed_key(key('a'));
    type_text(&mut app, "alias.co");
    press(&mut app, KeyCode::Enter);
    let (title, _) = popup_text(&app).unwrap();
    assert_eq!(title, "alias.co (local)");
    type_text(&mut app, "checkout -b");
    press(&mut app, KeyCode::Enter);
    assert_eq!(get(&fx, "--local", "alias.co"), ["checkout -b"]);

    for value in ["helper-a", "helper-b"] {
        app.feed_key(key('a'));
        type_text(&mut app, "credential.helper");
        press(&mut app, KeyCode::Enter);
        type_text(&mut app, value);
        press(&mut app, KeyCode::Enter);
    }
    assert_eq!(
        get(&fx, "--local", "credential.helper"),
        ["helper-a", "helper-b"]
    );

    // Editing one of two values changes that one only.
    select(&mut app, "credential.helper", Scope::Local);
    press(&mut app, KeyCode::Enter);
    for _ in 0..8 {
        press(&mut app, KeyCode::Backspace);
    }
    type_text(&mut app, "helper-c");
    press(&mut app, KeyCode::Enter);
    assert_eq!(
        get(&fx, "--local", "credential.helper"),
        ["helper-c", "helper-b"]
    );
}

#[test]
fn git_refusing_a_value_keeps_the_popup_and_changes_nothing() {
    let fx = Fixture::new("gc-bad");
    let mut app = fx.app();
    app.open_git_config();

    app.feed_key(key('a'));
    type_text(&mut app, "nosection");
    press(&mut app, KeyCode::Enter);
    type_text(&mut app, "v");
    press(&mut app, KeyCode::Enter);
    assert!(popup_text(&app).is_some(), "the popup stays for a retry");
    assert!(
        app.git_config()
            .rows
            .iter()
            .all(|r| r.entry.key != "nosection")
    );

    app.feed_key(key('a'));
    // An empty key is a notice, not a git call.
    press(&mut app, KeyCode::Esc);
}

#[test]
fn an_include_line_is_read_only() {
    let fx = Fixture::new("gc-include");
    fs::write(fx.dir.join(".git/extra"), "[inc]\n\tk = v\n").unwrap();
    fx.git(&["config", "--local", "include.path", "extra"]);
    let mut app = fx.app();
    app.open_git_config();

    select(&mut app, "include.path", Scope::Local);
    press(&mut app, KeyCode::Enter);
    assert!(popup_text(&app).is_none());
    assert!(
        app.git_config()
            .note
            .as_deref()
            .unwrap()
            .contains("include")
    );

    let included = app
        .git_config()
        .rows
        .iter()
        .find(|r| r.entry.key == "inc.k")
        .unwrap();
    assert!(included.included);
}

#[test]
fn d_asks_then_unsets_one_value_and_says_which_one_wins() {
    let fx = Fixture::new("gc-unset");
    fx.git(&["config", "--local", "core.editor", "vim"]);
    let mut app = fx.app();
    app.open_git_config();

    select(&mut app, "core.editor", Scope::Local);
    app.feed_key(key('d'));
    assert_eq!(
        app.confirm_message(),
        Some("unset core.editor = vim in local?")
    );
    app.feed_key(key('n'));
    assert!(app.confirm_message().is_none());
    assert_eq!(get(&fx, "--local", "core.editor"), ["vim"]);

    app.feed_key(key('d'));
    app.feed_key(key('y'));
    assert_eq!(get(&fx, "--local", "core.editor"), Vec::<String>::new());
    assert_eq!(get(&fx, "--global", "core.editor"), ["nvim"]);
    assert_eq!(
        app.git_config().note.as_deref(),
        Some("unset core.editor in local; global value nvim now wins")
    );

    // A key set nowhere else says so.
    select(&mut app, "pull.rebase", Scope::Local);
    app.feed_key(key('d'));
    app.feed_key(key('y'));
    assert!(
        app.git_config()
            .note
            .as_deref()
            .unwrap()
            .contains("global value true now wins")
    );
}

#[test]
fn d_removes_only_the_selected_value_of_a_multi_valued_key() {
    let fx = Fixture::new("gc-unset-multi");
    fx.git(&["config", "--local", "credential.helper", "a"]);
    fx.git(&["config", "--local", "--add", "credential.helper", "b"]);
    let mut app = fx.app();
    app.open_git_config();

    select(&mut app, "credential.helper", Scope::Local);
    app.feed_key(key('d'));
    app.feed_key(key('y'));
    assert_eq!(get(&fx, "--local", "credential.helper"), ["b"]);
}

#[test]
fn d_on_a_value_from_another_scope_points_at_s() {
    let fx = Fixture::new("gc-unset-scope");
    let mut app = fx.app();
    app.open_git_config();

    select(&mut app, "core.editor", Scope::Global);
    app.feed_key(key('d'));
    assert!(app.confirm_message().is_none());
    assert!(
        app.git_config()
            .note
            .as_deref()
            .unwrap()
            .contains("press s")
    );
    assert_eq!(get(&fx, "--global", "core.editor"), ["nvim"]);
}

#[test]
fn the_first_global_write_asks_once_and_carries_on_after_the_yes() {
    let fx = Fixture::new("gc-global");
    fx.git(&["config", "--global", "commit.gpgsign", "true"]);
    let mut app = fx.app();
    app.open_git_config();
    app.feed_key(key('s'));

    select(&mut app, "commit.gpgsign", Scope::Global);
    app.feed_key(key(' '));
    let question = app.confirm_message().unwrap().to_owned();
    assert!(question.starts_with("write to "), "{question}");
    assert!(question.contains("sandbox-global"), "{question}");
    app.feed_key(key('n'));
    assert_eq!(get(&fx, "--global", "commit.gpgsign"), ["true"]);

    app.feed_key(key(' '));
    app.feed_key(key('y'));
    assert_eq!(get(&fx, "--global", "commit.gpgsign"), ["false"]);
    // Asked once: the next write goes straight through.
    app.feed_key(key(' '));
    assert!(app.confirm_message().is_none());
    assert_eq!(get(&fx, "--global", "commit.gpgsign"), ["true"]);
}

#[test]
fn a_global_edit_opens_its_popup_after_the_yes_and_keeps_what_is_typed() {
    let fx = Fixture::new("gc-global-edit");
    let mut app = fx.app();
    app.open_git_config();
    app.feed_key(key('s'));

    select(&mut app, "core.editor", Scope::Global);
    press(&mut app, KeyCode::Enter);
    assert!(popup_text(&app).is_none(), "the question comes first");
    app.feed_key(key('y'));
    let (title, text) = popup_text(&app).unwrap();
    assert_eq!(
        (title.as_str(), text.as_str()),
        ("core.editor (global)", "nvim")
    );
    type_text(&mut app, "!");
    press(&mut app, KeyCode::Enter);
    assert_eq!(get(&fx, "--global", "core.editor"), ["nvim!"]);

    // `a` after the yes needs no second question.
    app.feed_key(key('a'));
    assert!(app.confirm_message().is_none());
    assert!(popup_text(&app).is_some());
}

#[test]
fn unsetting_in_global_names_the_file_and_counts_as_the_confirmation_and_hides_secrets() {
    let fx = Fixture::new("gc-global-unset");
    let mut app = fx.app();
    app.open_git_config();
    app.feed_key(key('s'));

    select(&mut app, "github.token", Scope::Global);
    app.feed_key(key('d'));
    let question = app.confirm_message().unwrap().to_owned();
    assert!(question.contains("sandbox-global"), "{question}");
    assert!(
        question.contains("***") && !question.contains("ghp_secret"),
        "{question}"
    );
    press(&mut app, KeyCode::Esc);
    assert_eq!(get(&fx, "--global", "github.token"), ["ghp_secret"]);

    app.feed_key(key('d'));
    app.feed_key(key('y'));
    assert_eq!(get(&fx, "--global", "github.token"), Vec::<String>::new());
    // The one question already covered the session.
    select(&mut app, "core.editor", Scope::Global);
    app.feed_key(key(' '));
    assert!(app.confirm_message().is_none());
}

#[test]
fn capital_c_opens_the_screen_from_the_panes_and_closes_it_again() {
    let fx = Fixture::new("gc-toggle");
    let mut app = fx.app();
    assert_eq!(app.full_screen(), FullScreen::None);

    app.feed_key(key('C'));
    assert_eq!(app.full_screen(), FullScreen::GitConfig);
    app.feed_key(key('C'));
    assert_eq!(app.full_screen(), FullScreen::None);

    // While typing a filter, `C` is a letter.
    app.feed_key(key('C'));
    app.feed_key(key('/'));
    app.feed_key(key('C'));
    assert_eq!(app.full_screen(), FullScreen::GitConfig);
    assert_eq!(app.git_config().filter, "C");
}

#[test]
fn a_new_filter_selects_its_first_match_and_a_reread_stays_near() {
    let fx = Fixture::new("gc-filter-top");
    fx.git(&["config", "--local", "zz.last", "x"]);
    let mut app = fx.app();
    app.open_git_config();
    press(&mut app, KeyCode::End);
    assert_eq!(
        app.git_config().selected_row().unwrap().entry.key,
        "zz.last"
    );
    app.feed_key(key('/'));
    type_text(&mut app, "pull");
    assert_eq!(
        app.git_config().selected,
        0,
        "the old row is not in the matches"
    );
    assert_eq!(
        app.git_config().selected_row().unwrap().entry.key,
        "pull.rebase"
    );
}
