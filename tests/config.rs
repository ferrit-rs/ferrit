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

use ferrit::app::App;
use ferrit::app::config::{Config, ConfigLoad};
use ferrit::app::theme_config::{Preset, ThemeConfig};
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
    ThemeConfig { preset, accent }
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
