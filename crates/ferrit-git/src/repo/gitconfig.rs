//! The `git2` and subprocess half of `ferrit_domain::config`: the types are there.

use crate::repo::read::{stderr, workdir};
use crate::repo::{Repo, exec};
use ferrit_domain::config::{ConfigView, ValueKind, WriteScope, parse};
use ferrit_domain::error::{GitError, GitResult};
use git2::Repository;
use std::ffi::OsStr;
use std::path::Path;

// --- config ---
pub(crate) fn read(repo: &Repository, envs: &[(&str, &OsStr)]) -> GitResult<ConfigView> {
    let mut cmd = exec::git(workdir(repo)?);
    cmd.args(["config", "--list", "--show-origin", "--show-scope", "-z"]);
    cmd.envs(envs.iter().copied());
    let out = exec::output(&mut cmd)
        .map_err(|e| GitError::ConfigFailed(format!("cannot run git: {e}")))?;
    if !out.status.success() {
        return Err(GitError::ConfigFailed(stderr(&out)));
    }
    Ok(parse(&String::from_utf8_lossy(&out.stdout)))
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
pub(crate) fn set(
    repo: &Repository,
    envs: &[(&str, &OsStr)],
    scope: WriteScope,
    key: &str,
    value: &str,
    kind: ValueKind,
) -> GitResult<()> {
    write(workdir(repo)?, envs, None, scope, key, value, kind)
}

/// Add one more value to a multi-valued key.
pub(crate) fn add(
    repo: &Repository,
    envs: &[(&str, &OsStr)],
    scope: WriteScope,
    key: &str,
    value: &str,
    kind: ValueKind,
) -> GitResult<()> {
    write(workdir(repo)?, envs, Some("--add"), scope, key, value, kind)
}

/// Replace every value of `key` in `scope` with this one.
pub(crate) fn replace_all(
    repo: &Repository,
    envs: &[(&str, &OsStr)],
    scope: WriteScope,
    key: &str,
    value: &str,
    kind: ValueKind,
) -> GitResult<()> {
    write(
        workdir(repo)?,
        envs,
        Some("--replace-all"),
        scope,
        key,
        value,
        kind,
    )
}

/// Change the one value `old` of a multi-valued `key` in `scope`, leaving its
/// siblings alone (`--fixed-value`: `old` is text, not a pattern).
pub(crate) fn replace_value(
    repo: &Repository,
    envs: &[(&str, &OsStr)],
    scope: WriteScope,
    key: &str,
    value: &str,
    old: &str,
    kind: ValueKind,
) -> GitResult<()> {
    let mut args = vec![scope.flag()];
    args.extend(kind.flag());
    args.extend(["--fixed-value", "--", key, value, old]);
    run(workdir(repo)?, envs, &args)
}

/// Remove only the value `old` of `key` in `scope`.
pub(crate) fn unset_value(
    repo: &Repository,
    envs: &[(&str, &OsStr)],
    scope: WriteScope,
    key: &str,
    old: &str,
) -> GitResult<()> {
    run(
        workdir(repo)?,
        envs,
        &[scope.flag(), "--fixed-value", "--unset", "--", key, old],
    )
}

/// Remove every value of `key` in `scope`; the other scopes keep theirs.
pub(crate) fn unset(
    repo: &Repository,
    envs: &[(&str, &OsStr)],
    scope: WriteScope,
    key: &str,
) -> GitResult<()> {
    run(
        workdir(repo)?,
        envs,
        &[scope.flag(), "--unset-all", "--", key],
    )
}

#[allow(
    clippy::same_name_method,
    reason = "the `GitPort` config role forwards to these methods under the same names"
)]
impl Repo {
    /// Point this handle's `git config` calls at `global` as the global file
    /// and an empty system file, so a test can write the global scope without
    /// touching the user's real `~/.gitconfig`. Nothing else is affected.
    pub fn isolate_config(&mut self, global: &Path) {
        self.config_global = Some(global.to_path_buf());
    }

    fn config_envs(&self) -> Vec<(&'static str, &OsStr)> {
        self.config_global
            .as_deref()
            .map_or_else(Vec::new, |global| {
                vec![
                    ("GIT_CONFIG_GLOBAL", global.as_os_str()),
                    ("GIT_CONFIG_SYSTEM", OsStr::new("/dev/null")),
                ]
            })
    }

    /// Every git config value with its scope and origin.
    /// See `docs/PLAN_14_GIT_CONFIG.md`.
    pub fn config(&self) -> GitResult<ConfigView> {
        read(&self.inner, &self.config_envs())
    }

    /// `git config <scope> <key> <value>`; git validates a typed value.
    pub fn config_set(
        &self,
        scope: WriteScope,
        key: &str,
        value: &str,
        kind: ValueKind,
    ) -> GitResult<()> {
        set(&self.inner, &self.config_envs(), scope, key, value, kind)
    }

    /// `git config --add`: one more value for a multi-valued key.
    pub fn config_add(
        &self,
        scope: WriteScope,
        key: &str,
        value: &str,
        kind: ValueKind,
    ) -> GitResult<()> {
        add(&self.inner, &self.config_envs(), scope, key, value, kind)
    }

    /// `git config --replace-all`: every value of the key in `scope` becomes this one.
    pub fn config_replace_all(
        &self,
        scope: WriteScope,
        key: &str,
        value: &str,
        kind: ValueKind,
    ) -> GitResult<()> {
        replace_all(&self.inner, &self.config_envs(), scope, key, value, kind)
    }

    /// Change one value of a multi-valued key (`--fixed-value`), leaving the others.
    pub fn config_replace_value(
        &self,
        scope: WriteScope,
        key: &str,
        value: &str,
        old: &str,
        kind: ValueKind,
    ) -> GitResult<()> {
        replace_value(
            &self.inner,
            &self.config_envs(),
            scope,
            key,
            value,
            old,
            kind,
        )
    }

    /// Remove one value of a multi-valued key, leaving the others.
    pub fn config_unset_value(&self, scope: WriteScope, key: &str, old: &str) -> GitResult<()> {
        unset_value(&self.inner, &self.config_envs(), scope, key, old)
    }

    /// `git config --unset-all`: drop the key from `scope` only.
    pub fn config_unset(&self, scope: WriteScope, key: &str) -> GitResult<()> {
        unset(&self.inner, &self.config_envs(), scope, key)
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::indexing_slicing,
    reason = "test scaffolding: a failed setup is the assertion"
)]
mod tests {
    use std::path::PathBuf;

    use crate::repo::exec;
    use crate::repo::gitconfig::{run, write};
    use ferrit_domain::config::{Scope, ValueKind, WriteScope, parse};
    use ferrit_domain::error::{GitError, GitResult};
    use std::ffi::OsStr;

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
