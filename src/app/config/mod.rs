//! The user's `config.toml`: loading with per-section error reporting, and
//! saving that merges into the file instead of rewriting it. See
//! `docs/PLAN_12_POLISH.md` P1.
//!
//! Rules:
//! - A missing file is the defaults, silently.
//! - A file that is not TOML gives every default and one issue.
//! - A section that does not fit its type gives that section's defaults and one
//!   issue naming it; the other sections still apply. Nothing is applied
//!   silently: every fallback is reported.
//! - Unknown sections and keys are reported once and never rejected, so a
//!   newer config still opens in an older ferrit.
//! - Saving replaces only the section being saved and keeps everything else
//!   in the file, including sections this version does not know. Comments are
//!   not preserved (the `toml` crate has no comment-preserving writer).
//! - Nothing in the library reads the real config directory on its own: only
//!   `Config::load`, which the binary calls, and `tests/config.rs` checks that.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use directories::ProjectDirs;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use super::theme_config::ThemeConfig;

const FILE_HEADER: &str = "# Ferrit configuration. Ferrit rewrites this file when it saves a\n\
                           # setting: unknown sections are kept, comments are not.\n\n";

/// Every setting ferrit reads from `config.toml`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub struct Config {
    pub theme: ThemeConfig,
    pub ui: UiConfig,
    pub diff: DiffConfig,
    pub commit: CommitConfig,
    pub log: LogConfig,
    /// `[keys.<context>] <action> = "<key>"`; see `keymap::Keymap::from_overrides`.
    pub keys: KeyOverrides,
}

/// `context name -> action name -> keys`, as written in the file. Names and
/// key text are checked when the keymap is built, not here, so one typo is one
/// reported entry and not a dropped section.
pub type KeyOverrides = BTreeMap<String, BTreeMap<String, KeyList>>;

/// One key or several: `quit = "Q"` or `quit = ["Q", "ctrl-q"]`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum KeyList {
    One(String),
    Many(Vec<String>),
}

impl KeyList {
    /// The key texts, in order.
    pub fn texts(&self) -> Vec<&str> {
        match self {
            Self::One(text) => vec![text.as_str()],
            Self::Many(texts) => texts.iter().map(String::as_str).collect(),
        }
    }
}

/// `[ui]`: how ferrit talks to the terminal.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub struct UiConfig {
    /// `false`: do not capture the mouse, so the terminal's own text selection
    /// works. Click, hover and wheel are then unavailable.
    pub mouse: bool,
    /// Lines the mouse wheel moves the right pane by, `1..=50`.
    pub wheel_step: u8,
    /// Seconds between background refreshes when no filesystem event arrives,
    /// `1..=3600`.
    pub poll_secs: u64,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            mouse: true,
            wheel_step: 3,
            poll_secs: 10,
        }
    }
}

/// `[diff]`: how `git diff` and `git show` are run, mirroring lazygit's
/// `git.*` settings and defaults.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub struct DiffConfig {
    /// Context lines around a change, `0..=200`.
    pub context: u32,
    pub ignore_whitespace: bool,
    /// Similarity percentage for rename detection, `0..=100`.
    pub rename_threshold: u32,
}

impl Default for DiffConfig {
    fn default() -> Self {
        Self {
            context: 3,
            ignore_whitespace: false,
            rename_threshold: 50,
        }
    }
}

/// `[commit]`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub struct CommitConfig {
    /// Start the commit editor with sign-off on. Still visible in its footer
    /// and flipped per commit with `Ctrl-O`.
    pub sign_off: bool,
}

/// `[log]`: the command log panel.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(default)]
pub struct LogConfig {
    /// List read-only commands (`git diff` on every selection) in the panel,
    /// not just writes. The `@` viewer always lists everything.
    pub show_reads: bool,
}

/// A loaded configuration plus where it came from and what was wrong with it.
#[derive(Debug, Clone, Default)]
pub struct ConfigLoad {
    pub config: Config,
    /// The file a later save writes to. `None`: nowhere (tests, or a system
    /// with no config directory).
    pub file: Option<PathBuf>,
    /// One line per problem, shown once at startup.
    pub issues: Vec<String>,
}

impl Config {
    /// `<platform config dir>/config.toml`.
    pub fn default_path() -> Option<PathBuf> {
        ProjectDirs::from("dev", "Ferrit", "Ferrit")
            .map(|dirs| dirs.config_dir().join("config.toml"))
    }

    /// Read the user's file. The only reader of the real config directory.
    pub fn load() -> ConfigLoad {
        match Self::default_path() {
            Some(path) => Self::load_from(&path),
            None => ConfigLoad::default(),
        }
    }

    /// Read `path`; a missing file is the defaults.
    pub fn load_from(path: &Path) -> ConfigLoad {
        let (config, issues) = match fs::read_to_string(path) {
            Ok(text) => Self::parse(&text),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => (Self::default(), Vec::new()),
            Err(e) => (
                Self::default(),
                vec![format!("cannot read {}: {e}", path.display())],
            ),
        };
        ConfigLoad {
            config,
            file: Some(path.to_path_buf()),
            issues,
        }
    }

    /// Parse the text of a config file. The seam tests use: no file, no
    /// environment.
    pub fn parse(text: &str) -> (Self, Vec<String>) {
        let table = match toml::from_str::<toml::Table>(text) {
            Ok(table) => table,
            Err(e) => {
                return (
                    Self::default(),
                    vec![format!("not valid TOML, using defaults: {}", one_line(&e))],
                );
            },
        };
        let mut issues = Vec::new();
        let mut failed = Vec::new();
        let mut config = Self {
            theme: section(&table, "theme", &mut issues, &mut failed),
            ui: section(&table, "ui", &mut issues, &mut failed),
            diff: section(&table, "diff", &mut issues, &mut failed),
            commit: section(&table, "commit", &mut issues, &mut failed),
            log: section(&table, "log", &mut issues, &mut failed),
            keys: section(&table, "keys", &mut issues, &mut failed),
        };
        // A section that fell back is reported by `section`; its keys would
        // only be listed a second time as unknown.
        let mut file = table;
        for name in failed {
            file.remove(name);
        }
        issues.extend(unknown_keys(&file, &config));
        issues.extend(config.theme.drop_unknown_colors());
        issues.extend(config.clamp_ranges());
        issues.extend(super::keymap::Keymap::from_overrides(&config.keys).1);
        (config, issues)
    }

    /// Put every out-of-range value back to its default and say which. Run
    /// after parsing: the types accept any `u16`, the meaning does not.
    fn clamp_ranges(&mut self) -> Vec<String> {
        let mut issues = Vec::new();
        let defaults = Self::default();
        let mut check = |name: &str, ok: bool, range: &str, reset: &mut dyn FnMut()| {
            if !ok {
                reset();
                issues.push(format!("`{name}` must be {range}, using the default"));
            }
        };
        check(
            "ui.wheel_step",
            (1..=50).contains(&self.ui.wheel_step),
            "1 to 50",
            &mut || self.ui.wheel_step = defaults.ui.wheel_step,
        );
        check(
            "ui.poll_secs",
            (1..=3600).contains(&self.ui.poll_secs),
            "1 to 3600",
            &mut || self.ui.poll_secs = defaults.ui.poll_secs,
        );
        check(
            "diff.context",
            self.diff.context <= 200,
            "0 to 200",
            &mut || self.diff.context = defaults.diff.context,
        );
        check(
            "diff.rename_threshold",
            self.diff.rename_threshold <= 100,
            "0 to 100",
            &mut || self.diff.rename_threshold = defaults.diff.rename_threshold,
        );
        issues
    }

    /// Write `theme` into `path`'s `[theme]` section, leaving every other
    /// section exactly as the file has it. Refuses to touch a file that is not
    /// valid TOML: saving would destroy what the user wrote.
    pub fn save_theme(path: &Path, theme: &ThemeConfig) -> Result<(), String> {
        let mut table = match fs::read_to_string(path) {
            Ok(text) => toml::from_str::<toml::Table>(&text).map_err(|e| {
                format!(
                    "{} is not valid TOML, fix it first (not overwritten): {e}",
                    path.display()
                )
            })?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => toml::Table::new(),
            Err(e) => return Err(format!("cannot read {}: {e}", path.display())),
        };
        let value = toml::Value::try_from(theme).map_err(|e| e.to_string())?;
        table.insert("theme".to_owned(), value);
        let body = toml::to_string_pretty(&table).map_err(|e| e.to_string())?;

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        // Write beside the target and rename, so a crash mid-write cannot
        // leave a truncated config behind.
        let staging = path.with_extension("toml.tmp");
        fs::write(&staging, format!("{FILE_HEADER}{body}")).map_err(|e| e.to_string())?;
        fs::rename(&staging, path).map_err(|e| e.to_string())
    }
}

/// Deserialize `name` from `table`, or its default plus an issue when it does
/// not fit (and `name` in `failed`).
fn section<'n, T: DeserializeOwned + Default>(
    table: &toml::Table,
    name: &'n str,
    issues: &mut Vec<String>,
    failed: &mut Vec<&'n str>,
) -> T {
    match table.get(name) {
        None => T::default(),
        Some(value) => value
            .clone()
            .try_into()
            .unwrap_or_else(|e: toml::de::Error| {
                issues.push(format!(
                    "[{name}] ignored, using its defaults: {}",
                    one_line(&e)
                ));
                failed.push(name);
                T::default()
            }),
    }
}

/// A parser message on one line, for a toast: its own line breaks and
/// alignment padding collapsed to single spaces.
fn one_line(error: &impl std::fmt::Display) -> String {
    error
        .to_string()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Keys in `file` that `config`, serialised back, does not have: they were
/// ignored. A section that failed to parse is reported by `section` already,
/// so its keys are not repeated here.
fn unknown_keys(file: &toml::Table, config: &Config) -> Vec<String> {
    let Ok(known) = toml::Table::try_from(config) else {
        return Vec::new();
    };
    let mut unknown = Vec::new();
    collect_unknown(file, &known, "", &mut unknown);
    unknown
        .into_iter()
        .map(|path| format!("unknown setting `{path}` ignored"))
        .collect()
}

fn collect_unknown(file: &toml::Table, known: &toml::Table, prefix: &str, out: &mut Vec<String>) {
    for (key, value) in file {
        let path = if prefix.is_empty() {
            key.clone()
        } else {
            format!("{prefix}.{key}")
        };
        match (known.get(key), value) {
            (None, _) => out.push(path),
            (Some(toml::Value::Table(known_inner)), toml::Value::Table(inner)) => {
                collect_unknown(inner, known_inner, &path, out);
            },
            _ => {},
        }
    }
}
