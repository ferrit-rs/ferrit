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
        let config = Self {
            theme: section(&table, "theme", &mut issues, &mut failed),
        };
        // A section that fell back is reported by `section`; its keys would
        // only be listed a second time as unknown.
        let mut file = table;
        for name in failed {
            file.remove(name);
        }
        issues.extend(unknown_keys(&file, &config));
        (config, issues)
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
