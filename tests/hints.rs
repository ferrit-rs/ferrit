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
//! The key hint bar and the help screen are generated from the keymap
//! (`docs/PLAN_12_POLISH.md` P3): they show the keys the user has, fit the
//! terminal, and the help reaches every line at any height.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use ferrit::app::config::{Config, ConfigLoad};
use ferrit::app::hints::{Bar, HelpLine, help_lines, keybar_layout};
use ferrit::app::keymap::{Action, Context, Keymap};
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

fn keymap_from(toml: &str) -> Keymap {
    let (config, issues) = Config::parse(toml);
    assert!(issues.is_empty(), "{issues:?}");
    Keymap::from_overrides(&config.keys).0
}

fn bar(map: &Keymap, bar: Bar, width: usize) -> String {
    keybar_layout(map, bar, width).text
}

#[test]
fn the_default_bars_read_exactly_as_the_hand_written_ones_did() {
    let map = Keymap::default();
    assert_eq!(
        bar(&map, Bar::Default, 120),
        "Stage: <space> | All: a | Discard: d | Commit: c | Amend: A | Reword: w | \
         Fetch/Pull/Push: f/p/P | Help: ? | Quit: q"
    );
    assert_eq!(
        bar(&map, Bar::Branches, 120),
        "Checkout: <space> | New: n | Delete: d | Fast-forward: u | Merge: M | \
         Fetch/Pull/Push: f/p/P | Help: ? | Quit: q"
    );
    assert_eq!(
        bar(&map, Bar::Stash, 120),
        "Apply: <space> | Pop: g | Drop: d | Fetch/Pull/Push: f/p/P | Help: ? | Quit: q"
    );
    assert_eq!(
        bar(&map, Bar::Commits, 120),
        "Reword: r | Drop: d | Squash: s | Fixup: S | Edit: e | New fixup!: F | \
         Autosquash: a | Help: ? | Quit: q",
        "the fetch group no longer fits at 120 columns and goes first"
    );
    assert_eq!(
        bar(&map, Bar::Operation, 120),
        "Continue / skip / abort: m | Stage: <space> | All: a | Commit: c | Help: ? | Quit: q"
    );
}

#[test]
fn a_wider_terminal_gets_the_segment_a_narrow_one_dropped() {
    let map = Keymap::default();
    assert!(bar(&map, Bar::Commits, 140).contains("Fetch/Pull/Push: f/p/P"));
    assert!(!bar(&map, Bar::Commits, 120).contains("Fetch"));
}

#[test]
fn a_narrow_terminal_drops_body_segments_from_the_end_and_keeps_help_and_quit() {
    let map = Keymap::default();
    for width in [100, 80, 60, 40, 30, 17] {
        let text = bar(&map, Bar::Default, width);
        assert!(text.chars().count() <= width, "{width}: {text:?}");
        assert!(text.ends_with("Help: ? | Quit: q"), "{width}: {text:?}");
        assert!(
            text.starts_with("Stage: <space>") || width < 35,
            "{width}: {text:?}"
        );
    }
    assert_eq!(bar(&map, Bar::Default, 17), "Help: ? | Quit: q");
    assert_eq!(bar(&map, Bar::Default, 16), "Help: ?", "then Quit goes");
    assert_eq!(bar(&map, Bar::Default, 6), "");
    assert_eq!(bar(&map, Bar::Default, 0), "");
}

#[test]
fn a_remapped_key_shows_in_the_bar_and_the_first_of_a_list_is_the_one_shown() {
    let map = keymap_from("[keys.global]\nhelp = \"H\"\nquit = [\"Q\", \"ctrl-q\"]\n");
    let text = bar(&map, Bar::Default, 120);
    assert!(
        text.contains("Help: H") && text.contains("Quit: Q"),
        "{text}"
    );
    assert!(!text.contains("ctrl-q"), "{text}");
}

#[test]
fn an_unbound_action_loses_its_segment_and_a_group_shrinks() {
    let map = keymap_from("[keys.files]\nstage_file = []\n\n[keys.global]\npull = []\n");
    let text = bar(&map, Bar::Default, 120);
    assert!(!text.contains("Stage:"), "{text}");
    assert!(text.contains("Fetch/Push: f/P"), "{text}");
}

fn frame(app: &mut App, width: u16, height: u16) -> String {
    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
    terminal
        .draw(|f| ferrit::app::screens::draw(f, app))
        .unwrap();
    terminal.backend().to_string()
}

fn app_with(tag: &str, toml: &str) -> (TempDir, App) {
    let dir = TempDir::new(tag);
    let repo = Repository::init(dir.path()).unwrap();
    configure_identity(dir.path());
    fs::write(dir.path().join("a.txt"), "one\n").unwrap();
    commit_all(&repo, "init");
    let (config, issues) = Config::parse(toml);
    assert!(issues.is_empty(), "{issues:?}");
    let load = ConfigLoad {
        config,
        file: None,
        issues,
    };
    let app = App::open_with(dir.path(), load).unwrap();
    (dir, app)
}

fn key(c: char) -> KeyEvent {
    KeyEvent::from(KeyCode::Char(c))
}

#[test]
fn the_screen_shows_the_remapped_key_in_the_bar_and_in_the_help() {
    let (_dir, mut app) = app_with("hints-remap", "[keys.global]\nhelp = \"H\"\n");
    assert!(frame(&mut app, 120, 40).contains("Help: H"));
    app.feed_key(key('H'));
    let help = frame(&mut app, 120, 40);
    assert!(help.contains("keybindings"), "{help}");
    let line = help
        .lines()
        .find(|l| l.contains("toggle this help"))
        .unwrap();
    let column = line.split('\u{2502}').nth(1).unwrap_or_default();
    assert!(
        column.starts_with("H "),
        "the key column starts with H: {line}"
    );
}

#[test]
fn the_help_lists_the_focused_panes_keys_then_the_global_ones() {
    let (_dir, mut app) = app_with("hints-pane", "");
    app.feed_key(key('4'));
    app.feed_key(key('?'));
    let out = frame(&mut app, 120, 60);
    let commits = out.find("Commits keys").expect("the pane's section");
    let global = out.find("Global keys").expect("the global section");
    assert!(commits < global, "the pane first");
    assert!(out.contains("reword the selected commit"), "{out}");
    assert!(
        !out.contains("Files keys"),
        "other panes' sections are left out"
    );
    assert!(out.contains("Fixed keys"), "{out}");
}

#[test]
fn help_lines_leave_out_unbound_actions_and_join_several_keys() {
    let map = keymap_from("[keys.global]\nquit = [\"Q\", \"ctrl-q\"]\nrefresh = []\n");
    let lines = help_lines(&map, &[Context::Global]);
    let entry = |text: &str| {
        lines.iter().find_map(|l| match l {
            HelpLine::Entry { keys, text: t } if t == text => Some(keys.clone()),
            _ => None,
        })
    };
    assert_eq!(entry("quit").as_deref(), Some("Q, ctrl-q"));
    assert_eq!(entry("refresh"), None, "unbound, so not listed");
    assert_eq!(entry("toggle this help").as_deref(), Some("?"));
}

#[test]
fn the_help_reaches_every_line_on_a_24_row_terminal_by_scrolling() {
    let (_dir, mut app) = app_with("hints-scroll", "");
    app.feed_key(key('2')); // Files: a pane section plus Global
    app.feed_key(key('?'));

    let lines = help_lines(&Keymap::default(), &[Context::Files, Context::Global]);
    let wanted: BTreeSet<String> = lines
        .iter()
        .filter_map(|l| match l {
            HelpLine::Entry { text, .. } => Some(text.clone()),
            HelpLine::Heading(text) => Some(text.clone()),
            HelpLine::Blank => None,
        })
        .collect();
    assert!(wanted.len() > 40, "more lines than a 24 row screen shows");

    let mut seen = String::new();
    for _ in 0..80 {
        seen.push_str(&frame(&mut app, 100, 24));
        app.feed_key(key('j'));
    }
    for text in &wanted {
        assert!(seen.contains(text.as_str()), "{text:?} was never on screen");
    }
    assert!(
        frame(&mut app, 100, 24).contains("wheel / click"),
        "the last line is reachable"
    );
}

#[test]
fn help_scroll_keys_stop_at_both_ends_and_closing_resets_the_position() {
    let (_dir, mut app) = app_with("hints-ends", "");
    app.feed_key(key('?'));
    let top = frame(&mut app, 100, 24);
    app.feed_key(key('k'));
    assert_eq!(frame(&mut app, 100, 24), top, "already at the top");

    app.feed_key(KeyEvent::from(KeyCode::End));
    let bottom = frame(&mut app, 100, 24);
    assert_ne!(bottom, top);
    app.feed_key(key('j'));
    assert_eq!(frame(&mut app, 100, 24), bottom, "already at the bottom");

    app.feed_key(KeyEvent::from(KeyCode::Home));
    assert_eq!(frame(&mut app, 100, 24), top);
    app.feed_key(KeyEvent::from(KeyCode::PageDown));
    assert_ne!(frame(&mut app, 100, 24), top, "a page moves it");

    app.feed_key(key('?')); // close, scrolled
    app.feed_key(key('?')); // reopen
    assert_eq!(frame(&mut app, 100, 24), top, "back at the top");
}

#[test]
fn every_help_close_key_works_and_other_keys_are_swallowed() {
    let (_dir, mut app) = app_with("hints-close", "");
    for close in [key('?'), key('q'), KeyEvent::from(KeyCode::Esc)] {
        app.feed_key(key('?'));
        assert!(app.show_help);
        app.feed_key(key('d')); // would open a discard confirm
        assert!(app.confirm_message().is_none());
        app.feed_key(close);
        assert!(!app.show_help);
    }
    assert!(!app.is_quitting(), "q closed the help, it did not quit");
}

#[test]
fn a_pane_without_bindings_of_its_own_shows_only_the_global_section() {
    let (_dir, mut app) = app_with("hints-status", "");
    app.feed_key(key('1'));
    app.feed_key(key('?'));
    let out = frame(&mut app, 120, 60);
    assert!(out.contains("Global keys"), "{out}");
    assert!(
        !out.contains("Files keys") && !out.contains("Commits keys"),
        "{out}"
    );
    let _ = Pane::Status;
}

#[test]
fn no_help_text_is_longer_than_the_dialog_can_show() {
    // The dialog is 80 wide: borders, a scroll bar column, a key column of at
    // most 24 and two spaces leave 50 for the text. A longer one would be cut
    // off on screen, unnoticed.
    let map = Keymap::default();
    let all = [
        Context::Global,
        Context::Files,
        Context::Diff,
        Context::Branches,
        Context::Commits,
        Context::Stash,
    ];
    for line in help_lines(&map, &all) {
        if let HelpLine::Entry { keys, text } = line {
            assert!(
                text.chars().count() <= 50,
                "{text:?} is {} long",
                text.chars().count()
            );
            assert!(
                keys.chars().count() <= 24,
                "{keys:?} is too wide for the key column"
            );
        }
    }
}

fn hit_at(map: &Keymap, bar: Bar, width: usize, column: u16) -> Option<Action> {
    keybar_layout(map, bar, width)
        .hits
        .iter()
        .find(|hit| (hit.start..hit.end).contains(&column))
        .map(|hit| hit.action)
}

#[test]
fn a_single_hint_is_clickable_across_its_whole_text() {
    let map = Keymap::default();
    let text = bar(&map, Bar::Default, 120);
    // "Stage: <space>" is the first 14 cells, then " | ".
    assert!(text.starts_with("Stage: <space> | All: a"));
    for column in [0, 6, 13] {
        assert_eq!(
            hit_at(&map, Bar::Default, 120, column),
            Some(Action::StageFile)
        );
    }
    assert_eq!(hit_at(&map, Bar::Default, 120, 14), None, "the separator");
    assert_eq!(hit_at(&map, Bar::Default, 120, 17), Some(Action::StageAll));
}

#[test]
fn a_group_is_clickable_by_label_and_not_on_its_keys() {
    let map = Keymap::default();
    let text = bar(&map, Bar::Branches, 200);
    let start = u16::try_from(text.find("Fetch/Pull/Push").expect("the group")).unwrap();
    let at = |offset: u16| hit_at(&map, Bar::Branches, 200, start + offset);
    assert_eq!(at(0), Some(Action::Fetch));
    assert_eq!(at(4), Some(Action::Fetch));
    assert_eq!(at(5), None, "the slash between labels");
    assert_eq!(at(6), Some(Action::Pull));
    assert_eq!(at(11), Some(Action::Push));
    assert_eq!(at(15), None, "the colon");
    assert_eq!(at(18), None, "the keys of a group");
}

#[test]
fn dropped_segments_are_not_clickable_and_the_pinned_ones_keep_their_place() {
    let map = Keymap::default();
    let layout = keybar_layout(&map, Bar::Default, 40);
    assert!(
        layout.text.ends_with("Help: ? | Quit: q"),
        "{}",
        layout.text
    );
    assert!(
        layout
            .hits
            .iter()
            .all(|h| usize::from(h.end) <= layout.text.len())
    );
    let quit = layout.hits.last().unwrap();
    assert_eq!(quit.action, Action::Quit);
    assert_eq!(usize::from(quit.end), layout.text.len());
    assert!(!layout.hits.iter().any(|h| h.action == Action::Discard));
}
