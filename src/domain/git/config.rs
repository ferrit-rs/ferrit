//! The git configuration as `git config` reports it: every value, the level
//! it was set at and the file it came from. Reads and writes shell out to
//! `git` so the file format, includes and locking stay git's. See
//! `docs/PLAN_14_GIT_CONFIG.md`.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use git2::Repository;

use crate::domain::git::command_log::redact;
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
const SECRET_WORDS: [&str; 4] = ["password", "token", "secret", "credential"];

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

/// The files ferrit may write. No `System` variant: writing the system file
/// or an included file cannot be expressed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WriteScope {
    Local,
    Global,
    /// Only meaningful with `extensions.worktreeConfig`; git decides.
    Worktree,
}

impl WriteScope {
    const fn flag(self) -> &'static str {
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
    Text,
    Bool,
    Int,
    Path,
}

impl ValueKind {
    const fn flag(self) -> Option<&'static str> {
        match self {
            Self::Text => None,
            Self::Bool => Some("--type=bool"),
            Self::Int => Some("--type=int"),
            Self::Path => Some("--type=path"),
        }
    }
}

/// Run `git config <args>` in `workdir`. `envs` is how a test points git at
/// throwaway global and system files; production passes none.
fn run(workdir: &Path, envs: &[(&str, &OsStr)], args: &[&str]) -> GitResult<()> {
    let mut cmd = exec::git(workdir);
    cmd.arg("config").args(args).envs(envs.iter().copied());
    let out = exec::output(&mut cmd)
        .map_err(|e| GitError::ConfigFailed(format!("cannot run git: {e}")))?;
    if out.status.success() {
        return Ok(());
    }
    let message = stderr(&out);
    Err(GitError::ConfigFailed(
        match (message.is_empty(), out.status.code()) {
            (true, Some(5)) => "the key is not set at that level".to_owned(),
            (true, _) => "git config refused the change".to_owned(),
            (false, _) => message,
        },
    ))
}

/// `git config <scope> [--type=..] [--add|--replace-all] -- <key> <value>`.
fn write(
    workdir: &Path,
    envs: &[(&str, &OsStr)],
    mode: Option<&str>,
    scope: WriteScope,
    key: &str,
    value: &str,
    kind: ValueKind,
) -> GitResult<()> {
    let mut args = vec![scope.flag()];
    args.extend(kind.flag());
    args.extend(mode);
    args.extend(["--", key, value]);
    run(workdir, envs, &args)
}

/// Set `key` to `value` in `scope`. Git refuses a key that already has
/// several values there; use `replace_all`.
pub(super) fn set(
    repo: &Repository,
    scope: WriteScope,
    key: &str,
    value: &str,
    kind: ValueKind,
) -> GitResult<()> {
    write(workdir(repo)?, &[], None, scope, key, value, kind)
}

/// Add one more value to a multi-valued key.
pub(super) fn add(
    repo: &Repository,
    scope: WriteScope,
    key: &str,
    value: &str,
    kind: ValueKind,
) -> GitResult<()> {
    write(workdir(repo)?, &[], Some("--add"), scope, key, value, kind)
}

/// Replace every value of `key` in `scope` with this one.
pub(super) fn replace_all(
    repo: &Repository,
    scope: WriteScope,
    key: &str,
    value: &str,
    kind: ValueKind,
) -> GitResult<()> {
    write(
        workdir(repo)?,
        &[],
        Some("--replace-all"),
        scope,
        key,
        value,
        kind,
    )
}

/// Change the one value `old` of a multi-valued `key` in `scope`, leaving its
/// siblings alone (`--fixed-value`: `old` is text, not a pattern).
pub(super) fn replace_value(
    repo: &Repository,
    scope: WriteScope,
    key: &str,
    value: &str,
    old: &str,
    kind: ValueKind,
) -> GitResult<()> {
    let mut args = vec![scope.flag()];
    args.extend(kind.flag());
    args.extend(["--fixed-value", "--", key, value, old]);
    run(workdir(repo)?, &[], &args)
}

/// Remove only the value `old` of `key` in `scope`.
pub(super) fn unset_value(
    repo: &Repository,
    scope: WriteScope,
    key: &str,
    old: &str,
) -> GitResult<()> {
    run(
        workdir(repo)?,
        &[],
        &[scope.flag(), "--fixed-value", "--unset", "--", key, old],
    )
}

/// Remove every value of `key` in `scope`; the other scopes keep theirs.
pub(super) fn unset(repo: &Repository, scope: WriteScope, key: &str) -> GitResult<()> {
    run(
        workdir(repo)?,
        &[],
        &[scope.flag(), "--unset-all", "--", key],
    )
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test scaffolding: a failed setup is the assertion"
)]
mod tests {
    use super::*;

    /// A repository plus global and system files of its own, so no test
    /// touches the real `~/.gitconfig`.
    struct Sandbox {
        dir: PathBuf,
        global: PathBuf,
    }

    impl Sandbox {
        fn new(tag: &str) -> Self {
            let dir =
                std::env::temp_dir().join(format!("ferrit-cfgw-{tag}-{}", std::process::id()));
            std::fs::create_dir_all(&dir).unwrap();
            let sandbox = Self {
                global: dir.join("global"),
                dir,
            };
            sandbox.git(&["init", "-q", "."]);
            sandbox
        }

        fn envs(&self) -> [(&'static str, &OsStr); 2] {
            [
                ("GIT_CONFIG_GLOBAL", self.global.as_os_str()),
                ("GIT_CONFIG_SYSTEM", OsStr::new("/dev/null")),
            ]
        }

        fn git(&self, args: &[&str]) {
            let out = exec::git(&self.dir)
                .args(args)
                .envs(self.envs())
                .output()
                .unwrap();
            assert!(out.status.success());
        }

        fn put(&self, scope: WriteScope, key: &str, value: &str, kind: ValueKind) -> GitResult<()> {
            write(&self.dir, &self.envs(), None, scope, key, value, kind)
        }

        fn get(&self, key: &str) -> Vec<(Scope, String)> {
            let out = exec::git(&self.dir)
                .args(["config", "--list", "--show-origin", "--show-scope", "-z"])
                .envs(self.envs())
                .output()
                .unwrap();
            parse(&String::from_utf8_lossy(&out.stdout))
                .entries
                .into_iter()
                .filter(|e| e.key == key)
                .map(|e| (e.scope, e.value))
                .collect()
        }
    }

    impl Drop for Sandbox {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    #[test]
    fn set_writes_only_the_chosen_scope() {
        let sb = Sandbox::new("scope");
        sb.put(WriteScope::Global, "pull.rebase", "true", ValueKind::Bool)
            .unwrap();
        sb.put(WriteScope::Local, "pull.rebase", "merges", ValueKind::Text)
            .unwrap();
        assert_eq!(
            sb.get("pull.rebase"),
            [
                (Scope::Global, "true".to_owned()),
                (Scope::Local, "merges".to_owned())
            ]
        );
        assert!(
            std::fs::read_to_string(&sb.global)
                .unwrap()
                .contains("rebase = true")
        );
    }

    #[test]
    fn a_typed_value_is_normalised_and_a_bad_one_is_rejected_untouched() {
        let sb = Sandbox::new("typed");
        sb.put(WriteScope::Local, "fetch.prune", "yes", ValueKind::Bool)
            .unwrap();
        assert_eq!(sb.get("fetch.prune"), [(Scope::Local, "true".to_owned())]);
        let err = sb
            .put(WriteScope::Local, "http.postbuffer", "abc", ValueKind::Int)
            .unwrap_err();
        assert!(matches!(err, GitError::ConfigFailed(m) if !m.is_empty()));
        assert!(sb.get("http.postbuffer").is_empty());
    }

    #[test]
    fn a_value_starting_with_a_dash_is_a_value_not_an_option() {
        let sb = Sandbox::new("dash");
        sb.put(WriteScope::Local, "alias.l", "--oneline", ValueKind::Text)
            .unwrap();
        assert_eq!(sb.get("alias.l"), [(Scope::Local, "--oneline".to_owned())]);
    }

    #[test]
    fn multi_valued_keys_add_replace_and_refuse_a_plain_set() {
        let sb = Sandbox::new("multi");
        let (d, e) = (&sb.dir, sb.envs());
        write(
            d,
            &e,
            None,
            WriteScope::Local,
            "credential.helper",
            "a",
            ValueKind::Text,
        )
        .unwrap();
        write(
            d,
            &e,
            Some("--add"),
            WriteScope::Local,
            "credential.helper",
            "b",
            ValueKind::Text,
        )
        .unwrap();
        assert_eq!(sb.get("credential.helper").len(), 2);
        assert!(
            sb.put(WriteScope::Local, "credential.helper", "c", ValueKind::Text)
                .is_err()
        );
        write(
            d,
            &e,
            Some("--replace-all"),
            WriteScope::Local,
            "credential.helper",
            "c",
            ValueKind::Text,
        )
        .unwrap();
        assert_eq!(
            sb.get("credential.helper"),
            [(Scope::Local, "c".to_owned())]
        );
    }

    #[test]
    fn unset_removes_the_scope_value_and_keeps_the_other() {
        let sb = Sandbox::new("unset");
        sb.put(WriteScope::Global, "core.editor", "nvim", ValueKind::Text)
            .unwrap();
        sb.put(WriteScope::Local, "core.editor", "vi", ValueKind::Text)
            .unwrap();
        run(
            &sb.dir,
            &sb.envs(),
            &["--local", "--unset-all", "--", "core.editor"],
        )
        .unwrap();
        assert_eq!(sb.get("core.editor"), [(Scope::Global, "nvim".to_owned())]);
        let err = run(
            &sb.dir,
            &sb.envs(),
            &["--local", "--unset-all", "--", "core.editor"],
        )
        .unwrap_err();
        assert!(matches!(err, GitError::ConfigFailed(m) if m.contains("not set")));
    }

    #[test]
    fn a_locked_file_leaves_the_value_alone_and_says_why() {
        let sb = Sandbox::new("lock");
        sb.put(WriteScope::Local, "user.name", "before", ValueKind::Text)
            .unwrap();
        std::fs::write(sb.dir.join(".git/config.lock"), "").unwrap();
        assert!(
            sb.put(WriteScope::Local, "user.name", "after", ValueKind::Text)
                .is_err()
        );
        assert_eq!(sb.get("user.name"), [(Scope::Local, "before".to_owned())]);
    }
}
