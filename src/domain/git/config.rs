//! The git configuration as `git config` reports it: every value, the level
//! it was set at and the file it came from. Reads and writes shell out to
//! `git` so the file format, includes and locking stay git's. See
//! `docs/PLAN_14_GIT_CONFIG.md`.

use std::path::PathBuf;

use git2::Repository;

use crate::domain::git::diff::{stderr, workdir};
use crate::domain::git::error::{GitError, GitResult};
use crate::domain::git::exec;

/// The level a value was set at, in git's own precedence order (later wins).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Scope {
    System,
    Global,
    Local,
    Worktree,
    Command,
}

/// Where a value was read from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    File(PathBuf),
    /// `-c`, `GIT_CONFIG_COUNT` and the like.
    CommandLine,
    Other(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigEntry {
    pub scope: Scope,
    pub origin: Origin,
    /// Section and variable names lower-cased by git; the subsection keeps its case.
    pub key: String,
    /// Empty for `key =` and for a bare `key` (git's implicit true).
    pub value: String,
}

/// Every value in git's order: for a key set several times the last one wins.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConfigView {
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

fn parse_scope(text: &str) -> Scope {
    match text {
        "system" => Scope::System,
        "global" => Scope::Global,
        "worktree" => Scope::Worktree,
        "command" => Scope::Command,
        _ => Scope::Local,
    }
}

fn parse_origin(text: &str) -> Origin {
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

pub(super) fn read(repo: &Repository) -> GitResult<ConfigView> {
    let mut cmd = exec::git(workdir(repo)?);
    cmd.args(["config", "--list", "--show-origin", "--show-scope", "-z"]);
    let out = exec::output(&mut cmd)
        .map_err(|e| GitError::ConfigFailed(format!("cannot run git: {e}")))?;
    if !out.status.success() {
        return Err(GitError::ConfigFailed(stderr(&out)));
    }
    Ok(parse(&String::from_utf8_lossy(&out.stdout)))
}
