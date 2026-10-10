//! The git configuration as `git config` reports it: every value, the level
//! it was set at and the file it came from. Reads and writes shell out to
//! `git` so the file format, includes and locking stay git's. See
//! `docs/PLAN_14_GIT_CONFIG.md`.
//!
//! This file holds the types and the pure functions. The code that reads with
//! `git2` or runs `git` is `crate::git::repo::gitconfig`.

use std::path::PathBuf;

use strum::{EnumString, IntoStaticStr};

use crate::git::command_log::redact;

/// The level a value was set at, in git's own precedence order (later wins).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, EnumString, IntoStaticStr)]
#[strum(serialize_all = "lowercase")]
pub enum Scope {
    /// `/etc/gitconfig` and the like, for every user of the machine.
    System,
    /// The user's own file (`~/.gitconfig`, `~/.config/git/config`).
    Global,
    /// The repository's `.git/config`.
    Local,
    /// The worktree's own file, with `extensions.worktreeConfig`.
    Worktree,
    /// Set for one command: `-c` or the `GIT_CONFIG_*` variables.
    Command,
}

/// Where a value was read from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    /// A config file, as a path.
    File(PathBuf),
    /// `-c`, `GIT_CONFIG_COUNT` and the like.
    CommandLine,
    /// Anything else git reports, as it writes it.
    Other(String),
}

/// One value of one key, with where it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigEntry {
    /// The level it was set at.
    pub scope: Scope,
    /// The file, or the command line, it was read from.
    pub origin: Origin,
    /// Section and variable names lower-cased by git; the subsection keeps its case.
    pub key: String,
    /// Empty for `key =` and for a bare `key` (git's implicit true).
    pub value: String,
}

/// Every value in git's order: for a key set several times the last one wins.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConfigView {
    /// One entry per value, in git's order. A key set at several levels appears several times.
    pub entries: Vec<ConfigEntry>,
}

impl ConfigView {
    /// The value git uses for `key`.
    #[must_use]
    pub fn effective(&self, key: &str) -> Option<&ConfigEntry> {
        self.entries.iter().rev().find(|e| e.key == key)
    }

    /// The values of `key` that a later one hides.
    pub fn shadowed<'a>(&'a self, key: &'a str) -> impl Iterator<Item = &'a ConfigEntry> {
        let last = self.entries.iter().rposition(|e| e.key == key);
        self.entries
            .iter()
            .enumerate()
            .filter(move |(i, e)| e.key == key && Some(*i) != last)
            .map(|(_, e)| e)
    }
}

impl ConfigView {
    /// Whether `entry` was read from a file the scope's own file includes:
    /// the first entry of a scope always comes from its main file, and git
    /// lists an included file's entries right after the `include.path` line.
    #[must_use]
    pub fn is_included(&self, entry: &ConfigEntry) -> bool {
        self.entries
            .iter()
            .find(|e| e.scope == entry.scope)
            .is_some_and(|first| first.origin != entry.origin)
    }
}

/// Words that make a key's value a secret to keep off the screen and out of
/// the command log. `credential.helper` names a program, not a secret.
pub(crate) const SECRET_WORDS: [&str; 4] = ["password", "token", "secret", "credential"];

/// Whether the value of `key` must never be shown.
#[must_use]
pub fn is_secret_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    !key.ends_with(".helper") && SECRET_WORDS.iter().any(|word| key.contains(word))
}

/// A value as the screen may show it: hidden for a secret key, with the
/// password of a URL hidden otherwise.
#[must_use]
pub fn display_value(key: &str, value: &str) -> String {
    if value.is_empty() {
        String::new()
    } else if is_secret_key(key) {
        "***".to_owned()
    } else {
        redact(value)
    }
}

pub(crate) fn parse_scope(text: &str) -> Scope {
    text.parse().unwrap_or(Scope::Local)
}

pub(crate) fn parse_origin(text: &str) -> Origin {
    match text.split_once(':') {
        Some(("file", path)) => Origin::File(PathBuf::from(path)),
        Some(("command line", _)) => Origin::CommandLine,
        _ => Origin::Other(text.to_owned()),
    }
}

/// Parse `git config --list --show-origin --show-scope -z`: per entry
/// `scope NUL origin NUL key NL value NUL`, the `NL value` part missing for a
/// key with no `=`. A truncated trailing entry is dropped.
#[must_use]
pub fn parse(raw: &str) -> ConfigView {
    let mut fields = raw.split('\0');
    let mut entries = Vec::new();
    while let (Some(scope), Some(origin), Some(pair)) =
        (fields.next(), fields.next(), fields.next())
    {
        let (key, value) = pair.split_once('\n').unwrap_or((pair, ""));
        if key.is_empty() {
            break;
        }
        entries.push(ConfigEntry {
            scope: parse_scope(scope),
            origin: parse_origin(origin),
            key: key.to_owned(),
            value: value.to_owned(),
        });
    }
    ConfigView { entries }
}

/// The files ferrit may write. No `System` variant: writing the system file
/// or an included file cannot be expressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, IntoStaticStr)]
#[strum(serialize_all = "lowercase")]
pub enum WriteScope {
    /// The repository's `.git/config`.
    Local,
    /// The user's `~/.gitconfig`.
    Global,
    /// Only meaningful with `extensions.worktreeConfig`; git decides.
    Worktree,
}

impl WriteScope {
    pub(crate) const fn flag(self) -> &'static str {
        match self {
            Self::Local => "--local",
            Self::Global => "--global",
            Self::Worktree => "--worktree",
        }
    }
}

/// How git validates and normalises a value before storing it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueKind {
    /// Stored as written.
    Text,
    /// A boolean; git normalises it to `true` or `false`.
    Bool,
    /// An integer; git accepts `k`, `m` and `g` suffixes.
    Int,
    /// A path; git expands `~`.
    Path,
}

impl ValueKind {
    pub(crate) const fn flag(self) -> Option<&'static str> {
        match self {
            Self::Text => None,
            Self::Bool => Some("--type=bool"),
            Self::Int => Some("--type=int"),
            Self::Path => Some("--type=path"),
        }
    }
}
