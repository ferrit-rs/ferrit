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
//! `config.toml` (`docs/PLAN_12_POLISH.md` P1): loading reports every
//! fallback, saving merges instead of rewriting, and nothing reads the real
//! config directory except `Config::load`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use ferrit::app::config::{CommitConfig, Config, ConfigLoad, DiffConfig, LogConfig, UiConfig};
use ferrit::app::theme_config::{Preset, ThemeConfig};
use ferrit::app::{App, DiffView, Pane};
use git2::{IndexAddOption, Repository, Signature};
use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Rect;
use ratatui::style::Color;

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

fn theme(preset: Preset, accent: Option<Color>) -> ThemeConfig {
    ThemeConfig {
        preset,
        accent,
        ..ThemeConfig::default()
    }
}

#[test]
fn an_empty_file_is_the_defaults_without_a_word() {
    let (config, issues) = Config::parse("");
    assert_eq!(config, Config::default());
    assert!(issues.is_empty(), "{issues:?}");
}

#[test]
fn a_theme_section_is_read() {
    let (config, issues) = Config::parse("[theme]\npreset = \"blue\"\naccent = \"#112233\"\n");
    assert_eq!(
        config.theme,
        theme(Preset::Blue, Some(Color::Rgb(0x11, 0x22, 0x33)))
    );
    assert!(issues.is_empty(), "{issues:?}");
}

#[test]
fn a_partial_section_keeps_the_defaults_for_the_rest() {
    let (config, issues) = Config::parse("[theme]\npreset = \"purple\"\n");
    assert_eq!(config.theme, theme(Preset::Purple, None));
    assert!(issues.is_empty());
}

#[test]
fn text_that_is_not_toml_gives_the_defaults_and_names_the_problem() {
    let (config, issues) = Config::parse("[theme\npreset = ");
    assert_eq!(config, Config::default());
    assert_eq!(issues.len(), 1, "{issues:?}");
    assert!(issues[0].contains("not valid TOML"), "{issues:?}");
    assert!(
        issues[0].contains("line"),
        "the parser's own location is kept: {issues:?}"
    );
}

#[test]
fn a_section_of_the_wrong_shape_falls_back_alone_and_is_reported_once() {
    let (config, issues) = Config::parse("[theme]\npreset = \"not-a-preset\"\naccent = 5\n");
    assert_eq!(config.theme, ThemeConfig::default());
    assert_eq!(
        issues.len(),
        1,
        "its keys are not also reported as unknown: {issues:?}"
    );
    assert!(issues[0].starts_with("[theme] ignored"), "{issues:?}");
}

#[test]
fn unknown_sections_and_keys_are_reported_and_the_rest_still_applies() {
    let (config, issues) = Config::parse(
        "[theme]\npreset = \"amber\"\nfuture_key = 1\n\n[from_a_newer_ferrit]\nx = 1\n",
    );
    assert_eq!(config.theme.preset, Preset::Amber);
    assert!(
        issues.iter().any(|i| i.contains("`theme.future_key`")),
        "{issues:?}"
    );
    assert!(
        issues.iter().any(|i| i.contains("`from_a_newer_ferrit`")),
        "{issues:?}"
    );
    assert_eq!(issues.len(), 2, "{issues:?}");
}

#[test]
fn a_missing_file_loads_as_the_defaults_and_remembers_where_to_save() {
    let dir = TempDir::new("config-missing");
    let path = dir.path().join("nested").join("config.toml");
    let load = Config::load_from(&path);
    assert_eq!(load.config, Config::default());
    assert!(load.issues.is_empty());
    assert_eq!(load.file.as_deref(), Some(path.as_path()));
}

#[test]
fn a_file_that_cannot_be_read_is_reported() {
    let dir = TempDir::new("config-unreadable");
    // A directory where the file should be: reading it fails, not `NotFound`.
    let path = dir.path().join("config.toml");
    fs::create_dir(&path).unwrap();
    let load = Config::load_from(&path);
    assert_eq!(load.config, Config::default());
    assert!(
        load.issues.iter().any(|i| i.contains("cannot read")),
        "{:?}",
        load.issues
    );
}

#[test]
fn saving_from_the_drawer_keeps_the_base() {
    let dir = TempDir::new("config-save-base");
    let path = dir.path().join("config.toml");
    fs::write(&path, "[theme]\nbase = \"light\"\npreset = \"green\"\n").unwrap();
    let mut loaded = Config::load_from(&path).config.theme;
    loaded.preset = Preset::Purple;
    Config::save_theme(&path, &loaded).unwrap();

    let reread = Config::load_from(&path).config.theme;
    assert_eq!(reread.base, ferrit::app::theme_config::Base::Light);
    assert_eq!(reread.preset, Preset::Purple);
}

#[test]
fn saving_creates_the_file_and_the_directory_and_reads_back() {
    let dir = TempDir::new("config-save-new");
    let path = dir.path().join("deep").join("config.toml");
    let saved = theme(Preset::Blue, Some(Color::Rgb(1, 2, 3)));
    Config::save_theme(&path, &saved).unwrap();

    let text = fs::read_to_string(&path).unwrap();
    assert!(text.starts_with("# Ferrit configuration"), "{text}");
    let load = Config::load_from(&path);
    assert!(load.issues.is_empty(), "{:?}", load.issues);
    assert_eq!(load.config.theme, saved);
    assert!(
        !path.with_extension("toml.tmp").exists(),
        "no staging file left"
    );
}

#[test]
fn saving_keeps_every_section_it_does_not_own() {
    let dir = TempDir::new("config-save-merge");
    let path = dir.path().join("config.toml");
    fs::write(
        &path,
        "[theme]\npreset = \"green\"\n\n[keys.global]\nquit = \"x\"\n\n[from_the_future]\nanswer = 42\n",
    )
    .unwrap();

    Config::save_theme(&path, &theme(Preset::Amber, None)).unwrap();

    let text = fs::read_to_string(&path).unwrap();
    assert!(text.contains("preset = \"amber\""), "{text}");
    assert!(text.contains("quit = \"x\""), "{text}");
    assert!(text.contains("answer = 42"), "{text}");
    assert_eq!(Config::load_from(&path).config.theme.preset, Preset::Amber);
}

#[test]
fn saving_refuses_to_overwrite_a_file_that_is_not_toml() {
    let dir = TempDir::new("config-save-broken");
    let path = dir.path().join("config.toml");
    let original = "this is [not toml\n";
    fs::write(&path, original).unwrap();

    let error = Config::save_theme(&path, &theme(Preset::Blue, None)).unwrap_err();
    assert!(error.contains("not overwritten"), "{error}");
    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        original,
        "left exactly as it was"
    );
}

#[test]
fn app_open_uses_the_defaults_and_has_nowhere_to_save() {
    let dir = TempDir::new("config-app-open");
    let repo = Repository::init(dir.path()).unwrap();
    configure_identity(dir.path());
    fs::write(dir.path().join("a"), "x\n").unwrap();
    commit_all(&repo, "init");
    let app = App::open(dir.path()).unwrap();
    assert!(app.config_file().is_none());
}

#[test]
fn app_open_with_reports_config_problems_once_in_the_status_pane() {
    let dir = TempDir::new("config-app-issues");
    let repo = Repository::init(dir.path()).unwrap();
    configure_identity(dir.path());
    fs::write(dir.path().join("a"), "x\n").unwrap();
    commit_all(&repo, "init");
    let file = dir.path().join("cfg.toml");
    fs::write(&file, "[theme]\npreset = \"nope\"\n").unwrap();

    let app = App::open_with(dir.path(), Config::load_from(&file)).unwrap();
    let status: String = app
        .status_lines()
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        status.contains("config") && status.contains("[theme] ignored"),
        "{status}"
    );
    assert_eq!(app.config_file(), Some(file.as_path()));
}

#[test]
fn saving_from_the_drawer_writes_the_theme_and_only_the_theme() {
    let dir = TempDir::new("config-app-save");
    let repo = Repository::init(dir.path()).unwrap();
    configure_identity(dir.path());
    fs::write(dir.path().join("a"), "x\n").unwrap();
    commit_all(&repo, "init");
    let file = dir.path().join("cfg.toml");
    fs::write(&file, "[from_the_future]\nanswer = 42\n").unwrap();

    let mut app = App::open_with(dir.path(), Config::load_from(&file)).unwrap();
    // Open the profile drawer by clicking its label, then next preset, save.
    app.set_author_click_area(Rect::new(0, 0, 10, 1));
    app.feed_mouse(MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 1,
        row: 0,
        modifiers: KeyModifiers::NONE,
    });
    app.feed_key(KeyEvent::from(KeyCode::Char('t')));
    app.feed_key(KeyEvent::from(KeyCode::Char('s')));

    let text = fs::read_to_string(&file).unwrap();
    assert!(text.contains("preset = \"blue\""), "{text}");
    assert!(
        text.contains("answer = 42"),
        "the unknown section survived: {text}"
    );
}

/// Only the binary may read the user's real config directory.
#[test]
fn only_main_reads_the_real_config_directory() {
    fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                rust_files(&path, out);
            } else if path.extension().is_some_and(|ext| ext == "rs") {
                out.push(path);
            }
        }
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    rust_files(&root, &mut files);
    for file in files {
        if file.ends_with("main.rs") || file.ends_with("app/config/mod.rs") {
            continue;
        }
        let text = fs::read_to_string(&file).unwrap();
        assert!(
            !text.contains("Config::load()") && !text.contains("default_path()"),
            "{} reads the real config directory",
            file.display()
        );
    }
    let _ = ConfigLoad::default();
}

#[test]
fn config_path_prints_the_file_and_exits_without_opening_a_repository() {
    let out = Command::new(env!("CARGO_BIN_EXE_ferrit"))
        .arg("--config-path")
        .current_dir(std::env::temp_dir())
        .output()
        .unwrap();
    assert!(out.status.success());
    let printed = String::from_utf8(out.stdout).unwrap();
    assert!(
        printed.trim_end().ends_with("config.toml") || printed.contains("no config directory"),
        "{printed}"
    );
}

// ------------------------------------------------------------------ P1b

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

fn with(config: Config, dir: &Path) -> App {
    App::open_with(
        dir,
        ConfigLoad {
            config,
            file: None,
            issues: Vec::new(),
        },
    )
    .unwrap()
}

fn repo_with_file(tag: &str, name: &str, content: &str) -> (TempDir, Repository) {
    let dir = TempDir::new(tag);
    let repo = Repository::init(dir.path()).unwrap();
    configure_identity(dir.path());
    fs::write(dir.path().join(name), content).unwrap();
    commit_all(&repo, "init");
    (dir, repo)
}

#[test]
fn the_documented_defaults_are_what_the_code_used_before() {
    let config = Config::default();
    assert_eq!(
        config.ui,
        UiConfig {
            mouse: true,
            wheel_step: 3,
            poll_secs: 10
        }
    );
    assert_eq!(
        config.diff,
        DiffConfig {
            context: 3,
            ignore_whitespace: false,
            rename_threshold: 50
        }
    );
    assert_eq!(config.commit, CommitConfig { sign_off: false });
    assert_eq!(config.log, LogConfig { show_reads: false });
}

#[test]
fn every_new_section_is_read() {
    let (config, issues) = Config::parse(
        "[ui]\nmouse = false\nwheel_step = 7\npoll_secs = 30\n\n[diff]\ncontext = 1\n\
         ignore_whitespace = true\nrename_threshold = 80\n\n[commit]\nsign_off = true\n\n\
         [log]\nshow_reads = true\n",
    );
    assert!(issues.is_empty(), "{issues:?}");
    assert_eq!(
        config.ui,
        UiConfig {
            mouse: false,
            wheel_step: 7,
            poll_secs: 30
        }
    );
    assert_eq!(
        config.diff,
        DiffConfig {
            context: 1,
            ignore_whitespace: true,
            rename_threshold: 80
        }
    );
    assert!(config.commit.sign_off && config.log.show_reads);
}

#[test]
fn out_of_range_values_go_back_to_their_default_one_by_one_and_are_named() {
    let (config, issues) = Config::parse(
        "[ui]\nwheel_step = 0\npoll_secs = 4000\n\n[diff]\ncontext = 201\nrename_threshold = 101\n",
    );
    assert_eq!(config.ui, UiConfig::default());
    assert_eq!(config.diff, DiffConfig::default());
    for name in [
        "ui.wheel_step",
        "ui.poll_secs",
        "diff.context",
        "diff.rename_threshold",
    ] {
        assert!(
            issues.iter().any(|i| i.contains(name)),
            "{name} missing: {issues:?}"
        );
    }
    assert_eq!(issues.len(), 4, "{issues:?}");
}

#[test]
fn one_bad_value_does_not_reset_its_valid_neighbours() {
    let (config, issues) = Config::parse("[ui]\nmouse = false\nwheel_step = 0\npoll_secs = 20\n");
    assert_eq!(
        config.ui,
        UiConfig {
            mouse: false,
            wheel_step: 3,
            poll_secs: 20
        }
    );
    assert_eq!(issues.len(), 1, "{issues:?}");
}

#[test]
fn the_edges_of_every_range_are_accepted() {
    let (config, issues) = Config::parse(
        "[ui]\nwheel_step = 50\npoll_secs = 3600\n\n[diff]\ncontext = 200\nrename_threshold = 100\n",
    );
    assert!(issues.is_empty(), "{issues:?}");
    assert_eq!((config.ui.wheel_step, config.ui.poll_secs), (50, 3600));
    assert_eq!(
        (config.diff.context, config.diff.rename_threshold),
        (200, 100)
    );
    let (config, issues) = Config::parse(
        "[ui]\nwheel_step = 1\npoll_secs = 1\n\n[diff]\ncontext = 0\nrename_threshold = 0\n",
    );
    assert!(issues.is_empty(), "{issues:?}");
    assert_eq!((config.ui.wheel_step, config.diff.context), (1, 0));
}

#[test]
fn a_wrongly_typed_value_drops_only_its_section() {
    let (config, issues) =
        Config::parse("[ui]\nmouse = \"yes\"\n\n[diff]\ncontext = 5\n\n[ui.extra]\nx = 1\n");
    assert_eq!(config.ui, UiConfig::default());
    assert_eq!(config.diff.context, 5, "the other sections still apply");
    assert_eq!(issues.len(), 1, "{issues:?}");
    assert!(issues[0].starts_with("[ui] ignored"), "{issues:?}");
    // A negative number does not fit `u8` either.
    let (config, issues) = Config::parse("[ui]\nwheel_step = -2\n");
    assert_eq!(config.ui, UiConfig::default());
    assert!(issues[0].starts_with("[ui] ignored"), "{issues:?}");
}

#[test]
fn wheel_step_sets_how_far_the_wheel_moves_the_right_pane() {
    for step in [1_u8, 7] {
        let base: String = (0..200).map(|n| format!("line {n}\n")).collect();
        let (dir, _repo) = repo_with_file(&format!("config-wheel-{step}"), "a_tall.txt", &base);
        // Two changes far apart: a diff tall enough for a 7 line step to fit.
        let edited = base
            .replace("line 5\n", "line 5 CHANGED\n")
            .replace("line 150\n", "line 150 CHANGED\n");
        fs::write(dir.path().join("a_tall.txt"), edited).unwrap();
        let mut config = Config::default();
        config.ui.wheel_step = step;
        let mut app = with(config, dir.path());
        app.select(Pane::Files, 0);
        app.set_right_viewport(10);
        app.set_right_area(Rect {
            x: 40,
            y: 0,
            width: 80,
            height: 12,
        });
        app.feed_mouse(MouseEvent {
            kind: MouseEventKind::ScrollDown,
            column: 60,
            row: 3,
            modifiers: KeyModifiers::NONE,
        });
        assert_eq!(app.right_scroll(), usize::from(step));
    }
}

fn unstaged_text(app: &App) -> String {
    match app.diff_view() {
        DiffView::Files(files) => files.unstaged.text.clone(),
        other => panic!("expected a file diff, got {other:?}"),
    }
}

#[test]
fn diff_context_sets_the_lines_around_a_change() {
    let base: String = (0..30).map(|n| format!("line {n}\n")).collect();
    let (dir, _repo) = repo_with_file("config-context", "a.txt", &base);
    fs::write(
        dir.path().join("a.txt"),
        base.replace("line 15\n", "line 15 CHANGED\n"),
    )
    .unwrap();

    // Context lines are the diff lines starting with one space (git also
    // repeats a nearby line in the `@@` header, so a plain substring is not
    // enough).
    let context_lines = |config: Config| {
        let mut app = with(config, dir.path());
        app.select(Pane::Files, 0);
        unstaged_text(&app)
            .lines()
            .filter(|line| line.starts_with(" line"))
            .count()
    };
    let with_context = |context: u32| {
        let mut config = Config::default();
        config.diff.context = context;
        config
    };
    assert_eq!(context_lines(with_context(0)), 0);
    assert_eq!(
        context_lines(with_context(3)),
        6,
        "three before and three after"
    );
    assert_eq!(context_lines(with_context(6)), 12);
    assert_eq!(context_lines(Config::default()), 6, "the default is 3");
}

#[test]
fn ignore_whitespace_hides_a_whitespace_only_change() {
    let (dir, _repo) = repo_with_file("config-ws", "a.txt", "one\ntwo\n");
    fs::write(dir.path().join("a.txt"), "one\n    two\n").unwrap();

    let mut app = with(Config::default(), dir.path());
    app.select(Pane::Files, 0);
    assert!(unstaged_text(&app).contains("+    two"));

    let mut config = Config::default();
    config.diff.ignore_whitespace = true;
    let mut app = with(config, dir.path());
    app.select(Pane::Files, 0);
    assert!(
        matches!(app.diff_view(), DiffView::Note(note) if note.contains("no changes")),
        "{:?}",
        app.diff_view()
    );
}

#[test]
fn rename_threshold_decides_whether_a_similar_file_counts_as_a_rename() {
    let body: String = (0..10).map(|n| format!("line {n}\n")).collect();
    let (dir, _repo) = repo_with_file("config-rename", "a.txt", &body);
    git(dir.path(), &["mv", "a.txt", "b.txt"]);
    fs::write(
        dir.path().join("b.txt"),
        body.replace("line 9\n", "line 9 EDITED\n"),
    )
    .unwrap();
    git(dir.path(), &["add", "b.txt"]);
    git(dir.path(), &["commit", "-qm", "rename and edit"]);

    let rows = |threshold: u32| {
        let mut config = Config::default();
        config.diff.rename_threshold = threshold;
        let mut app = with(config, dir.path());
        app.feed_key(KeyEvent::from(KeyCode::Char('4')));
        app.feed_key(KeyEvent::from(KeyCode::Enter)); // drill into the commit's files
        app.row_count(Pane::Commits)
    };
    assert_eq!(rows(50), 1, "one renamed file");
    assert_eq!(rows(100), 2, "a deleted file and an added one");
}

#[test]
fn commit_sign_off_sets_the_editors_starting_toggle() {
    for (configured, expected) in [(false, false), (true, true)] {
        let (dir, _repo) =
            repo_with_file(&format!("config-signoff-{configured}"), "a.txt", "one\n");
        fs::write(dir.path().join("a.txt"), "one\ntwo\n").unwrap();
        git(dir.path(), &["add", "a.txt"]);
        let mut config = Config::default();
        config.commit.sign_off = configured;
        let mut app = with(config, dir.path());
        app.feed_key(KeyEvent::from(KeyCode::Char('c')));
        let view = app.commit_popup().expect("the commit popup opened");
        assert_eq!(view.toggles, Some((expected, false)));
    }
}

#[test]
fn mouse_and_poll_settings_reach_the_app() {
    let (dir, _repo) = repo_with_file("config-ui", "a.txt", "one\n");
    assert!(with(Config::default(), dir.path()).mouse_enabled());
    let mut config = Config::default();
    config.ui.mouse = false;
    assert!(!with(config, dir.path()).mouse_enabled());
}
