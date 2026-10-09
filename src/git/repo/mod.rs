//! The `git2` and subprocess adapter behind the `GitPort` traits (`crate::git::port`).
//! `Repo` opens a repository and answers the traits; each submodule is the
//! implementation half of the domain module of the same name, which holds the
//! types. Nothing outside this module and `crate::git::fake` names `Repo`
//! except the composition root (`App::open`).

pub(crate) mod apply;
pub(crate) mod blob;
pub(crate) mod branch;
pub(crate) mod commit;
pub(crate) mod config;
pub(crate) mod diff;
pub(crate) mod host;
mod init;
pub(crate) mod log;
pub(crate) mod operation;
pub(crate) mod port_impl;
pub(crate) mod rebase;
pub(crate) mod refs;
pub(crate) mod remote;
pub(crate) mod stash;
pub(crate) mod stats;
pub(crate) mod status;

use std::path::Path;
use std::sync::atomic::AtomicBool;

use git2::Repository;

use crate::git::Snapshot;
use crate::git::apply::{ApplyDir, ApplyTarget};
use crate::git::blob::Rev;
use crate::git::branch::MergeOutcome;
use crate::git::commit::{CommitKind, CommitOpts};
use crate::git::config::{ConfigView, ValueKind, WriteScope};
use crate::git::diff::{Diff, DiffOpts, DiffSide};
use crate::git::error::{GitError, GitResult};
use crate::git::host::{CreateRequest, CreatedRepo, GhProgram};
use crate::git::identity::{Identity, IdentitySource};
use crate::git::model::{self, CommitEntry, RemoteEntry};
use crate::git::operation::{OperationOutcome, Step};
use crate::git::rebase::RebaseEdit;
use crate::git::stash::StashOutcome;
use crate::git::stats::{RepoStats, StatsOptions, Window};

/// A `git2` failure with the text git gave, kept as the `source` of
/// `GitError::Read` and `GitError::Open`.
#[derive(Debug, thiserror::Error)]
#[error("{message}")]
struct Git2Failure {
    message: String,
    #[source]
    source: git2::Error,
}

impl From<git2::Error> for Git2Failure {
    fn from(source: git2::Error) -> Self {
        Self {
            message: source.message().to_owned(),
            source,
        }
    }
}

/// A `git2` read that failed.
pub(crate) fn read_error(error: git2::Error) -> GitError {
    GitError::Read(Box::new(Git2Failure::from(error)))
}

/// The repository could not be opened.
fn open_error(error: git2::Error) -> GitError {
    GitError::Open(Box::new(Git2Failure::from(error)))
}

/// How many commits `Repo::snapshot()` reads for the Commits pane. lazygit lists every
/// commit; 1,000 covers all but the largest histories (the counter then reads `1 of
/// 1000`) while the walk stays cheap. Plain constant until the pane pages.
const COMMITS_LIMIT: usize = 1000;

fn config_values(config: &git2::Config, key: &str) -> Vec<String> {
    let Ok(mut entries) = config.multivar(key, None) else {
        return Vec::new();
    };
    let mut values = Vec::new();
    while let Some(Ok(entry)) = entries.next() {
        if let Ok(value) = entry.value() {
            values.push(value.to_owned());
        }
    }
    values
}

fn unique_identities(identities: impl IntoIterator<Item = Identity>) -> Vec<Identity> {
    let mut unique = Vec::new();
    for identity in identities {
        if !unique.contains(&identity) {
            unique.push(identity);
        }
    }
    unique
}

fn config_identities(config: &git2::Config) -> Vec<Identity> {
    let names = config_values(config, "user.name");
    let emails = config_values(config, "user.email");
    unique_identities(names.into_iter().enumerate().map(|(index, name)| Identity {
        name,
        email: emails.get(index).cloned(),
    }))
}

fn global_config_identities(config: &git2::Config) -> Vec<Identity> {
    [git2::ConfigLevel::XDG, git2::ConfigLevel::Global]
        .into_iter()
        .filter_map(|level| config.open_level(level).ok())
        .flat_map(|level| config_identities(&level))
        .fold(Vec::new(), |mut unique, identity| {
            if !unique.contains(&identity) {
                unique.push(identity);
            }
            unique
        })
}

fn config_identity(config: &git2::Config) -> Option<Identity> {
    let name = config.get_string("user.name").ok()?;
    Some(Identity {
        name,
        email: config.get_string("user.email").ok(),
    })
}

/// An open repository. Wraps `git2::Repository` and hands out owned snapshots.
pub struct Repo {
    inner: Repository,
    /// Path used to discover this repository. Lets the TUI reopen an owned
    /// handle inside a refresh worker; `git2::Repository` itself stays local.
    reopen_path: std::path::PathBuf,
    /// A throwaway global config file, set by `isolate_config`. `None` in
    /// real use: git then reads the user's own files.
    config_global: Option<std::path::PathBuf>,
}

/// Abbreviated hash, the 7 hex chars `git` shows by default. Shared by
/// `status` (upstream not needed there, but commits/refs both want it).
pub(crate) fn short_hash(oid: &git2::Oid) -> String {
    oid.to_string().chars().take(7).collect()
}

#[allow(
    clippy::same_name_method,
    reason = "the `GitPort` roles forward to these methods under the same names"
)]
impl Repo {
    /// Open the repository at or above `path`. Walks up like `git` does.
    pub fn open(path: &Path) -> GitResult<Self> {
        let reopen_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let inner = Repository::discover(path).map_err(|e| {
            if e.code() == git2::ErrorCode::NotFound {
                GitError::NotARepository(path.to_path_buf())
            } else {
                open_error(e)
            }
        })?;
        Ok(Self {
            inner,
            reopen_path,
            config_global: None,
        })
    }

    /// Path for opening a fresh handle in a background worker.
    pub fn reopen_path(&self) -> &Path {
        &self.reopen_path
    }

    /// The repository's directory name, e.g. `ferrit`. Used in the status
    /// header (`ferrit -> main`). Falls back to `"repo"` for odd layouts.
    pub fn name(&self) -> String {
        self.inner
            .workdir()
            .and_then(|w| w.file_name())
            .or_else(|| self.inner.path().parent().and_then(|p| p.file_name()))
            .map_or_else(|| "repo".to_owned(), |n| n.to_string_lossy().into_owned())
    }

    /// Configured Git author name (`user.name`), respecting repo, global,
    /// and system config precedence.
    pub fn user_name(&self) -> Option<String> {
        self.inner.config().ok()?.get_string("user.name").ok()
    }

    /// Global choices, repository override, resolved identity, and its source.
    pub fn identity_settings(
        &self,
    ) -> (
        Vec<Identity>,
        Option<Identity>,
        Option<Identity>,
        IdentitySource,
    ) {
        let global_identities = git2::Config::open_default()
            .ok()
            .map_or_else(Vec::new, |config| global_config_identities(&config));
        let local_config = git2::Config::open(&self.inner.path().join("config")).ok();
        let repository_identity = local_config.as_ref().and_then(config_identity);
        let repository_overrides_identity = local_config.as_ref().is_some_and(|config| {
            config.get_entry("user.name").is_ok() || config.get_entry("user.email").is_ok()
        });
        let effective_identity = self
            .inner
            .config()
            .ok()
            .and_then(|config| config_identity(&config));
        let identity_source = if repository_overrides_identity {
            IdentitySource::Repository
        } else if global_identities.is_empty() {
            if effective_identity.is_some() {
                IdentitySource::System
            } else {
                IdentitySource::Unset
            }
        } else {
            IdentitySource::Global
        };
        (
            global_identities,
            repository_identity,
            effective_identity,
            identity_source,
        )
    }

    /// Dashboard statistics over `window`, polling `cancel` while it walks
    /// (`GitError::Cancelled` when set). `docs/PLAN_13_DASHBOARD.md`.
    pub fn stats(&self, window: Window, cancel: &AtomicBool) -> GitResult<RepoStats> {
        self.stats_with(window, &StatsOptions::default(), cancel)
    }

    /// `stats` with the clock and the caps given (tests, tuning).
    pub fn stats_with(
        &self,
        window: Window,
        opts: &StatsOptions,
        cancel: &AtomicBool,
    ) -> GitResult<RepoStats> {
        stats::repo_stats(&self.inner, window, opts, cancel)
    }

    /// Re-read every wired pane in one go. Partial failure fails the whole call.
    ///
    /// `&mut self`: reading the stash list needs `&mut git2::Repository`.
    pub fn snapshot(&mut self) -> GitResult<Snapshot> {
        Ok(Snapshot {
            header: status::header(&self.inner)?,
            files: status::files(&self.inner)?,
            branches: refs::branches(&self.inner)?,
            commits: log::commits(&self.inner, COMMITS_LIMIT)?,
            stashes: stash::stashes(&mut self.inner)?,
            remotes: remote::remotes(&self.inner)?,
            operation: operation::current(&self.inner),
        })
    }

    /// The worktree root, or `None` for a bare repo. The filesystem watcher
    /// recurses from here; `.git` lives under it in the normal layout.
    pub fn workdir(&self) -> Option<&Path> {
        self.inner.workdir()
    }

    /// Raw bytes of `path` at `rev`. Feeds the right-pane image preview.
    pub fn blob_bytes(&self, path: &Path, rev: Rev) -> GitResult<Vec<u8>> {
        blob::blob_bytes(&self.inner, path, rev)
    }

    /// One file's diff (`git diff [--cached] -- <path>`), parsed. Untracked
    /// files come back via `--no-index` as all-additions.
    pub fn file_diff(&self, path: &Path, side: DiffSide, opts: DiffOpts) -> GitResult<Diff> {
        diff::file_diff(&self.inner, path, side, opts)
    }

    /// One commit's diff against its first parent (`git show <hash>`), parsed.
    pub fn commit_diff(&self, hash: &str, opts: DiffOpts) -> GitResult<Diff> {
        diff::commit_diff(&self.inner, hash, opts)
    }

    /// One local branch's own commit history, newest first, bounded by
    /// `COMMITS_LIMIT`. Feeds the Branches pane's Enter-to-drill-down log
    /// (`docs/PLAN_2_GIT_BACKEND.md`, G7).
    pub fn branch_log(&self, branch: &str) -> GitResult<Vec<CommitEntry>> {
        log::commits_for(&self.inner, branch, COMMITS_LIMIT)
    }

    /// Stage or unstage a whole file. No patch: `git add` / `git restore
    /// --staged`. See `docs/PLAN_6_STAGING.md`.
    pub fn stage_file(&self, path: &Path, dir: ApplyDir) -> GitResult<()> {
        apply::stage_file(&self.inner, path, dir)
    }

    /// Stage or unstage every changed file (`a`): `git add -A` / `git
    /// restore --staged .`.
    pub fn stage_all(&self, dir: ApplyDir) -> GitResult<()> {
        apply::stage_all(&self.inner, dir)
    }

    /// Stage everything except `excluded` (paths that must stay unstaged).
    pub fn stage_all_except(&self, excluded: &[std::path::PathBuf]) -> GitResult<()> {
        apply::stage_all_except(&self.inner, excluded)
    }

    /// Does the file still contain merge conflict markers?
    pub fn has_conflict_markers(&self, path: &Path) -> GitResult<bool> {
        apply::has_conflict_markers(&self.inner, path)
    }

    /// Discard a whole file's worktree change, never the index. `untracked`
    /// picks `git clean` (nothing to restore *to*) over `git restore
    /// --worktree`.
    pub fn discard_file(&self, path: &Path, untracked: bool) -> GitResult<()> {
        apply::discard_file(&self.inner, path, untracked)
    }

    /// Stage / unstage / discard one hunk. `patch` is `file.header.start
    /// .. hunk.body.end` over a `Diff::text`; the caller slices it so this
    /// module never re-runs the diff.
    pub fn apply_hunk(&self, patch: &str, dir: ApplyDir, target: ApplyTarget) -> GitResult<()> {
        apply::apply_hunk(&self.inner, patch, dir, target)
    }

    /// Stage / unstage / discard a set of body lines within one hunk.
    /// `lines` are 0-based indices into `hunk_body`'s own lines.
    pub fn apply_lines(
        &self,
        file_header: &str,
        hunk_header: &str,
        hunk_body: &str,
        lines: &[usize],
        dir: ApplyDir,
        target: ApplyTarget,
    ) -> GitResult<()> {
        apply::apply_lines(
            &self.inner,
            file_header,
            hunk_header,
            hunk_body,
            lines,
            dir,
            target,
        )
    }

    /// Run `git commit`. See `docs/PLAN_7_COMMIT.md`.
    pub fn commit(&self, kind: &CommitKind, message: &str, opts: CommitOpts) -> GitResult<String> {
        commit::commit(&self.inner, kind, message, opts)
    }

    /// `git remote set-url <name> <url>`: where a remote points, rewritten.
    pub fn set_remote_url(&self, name: &str, url: &str) -> GitResult<()> {
        host::set_remote_url(&self.inner, name, url)
    }

    /// Whether the current branch has a commit; a new repository has none.
    pub fn has_commits(&self) -> bool {
        commit::has_commits(&self.inner)
    }

    /// The first commit of a repository with none: an empty `README.md`.
    /// `Ok(false)` when there is already a commit. See
    /// `docs/PLAN_15_CREATE_REMOTE.md`.
    pub fn initial_commit(&self, author: Option<String>) -> GitResult<bool> {
        commit::initial_commit(&self.inner, author)
    }

    /// The `commit.template` file's message, comments removed (`None`: no
    /// template, or an empty one).
    pub fn commit_template(&self) -> Option<String> {
        commit::template(&self.inner)
    }

    /// `HEAD`'s current message, for pre-filling the Amend / Reword popup.
    pub fn head_message(&self) -> GitResult<Option<String>> {
        commit::head_message(&self.inner)
    }

    /// Count of paths staged relative to `HEAD`; the commit popup's
    /// precondition.
    pub fn staged_count(&self) -> GitResult<usize> {
        commit::staged_count(&self.inner)
    }

    /// `git checkout <name>`. See `docs/PLAN_8_BRANCHES.md`.
    pub fn checkout(&self, name: &str) -> GitResult<()> {
        branch::checkout(&self.inner, name)
    }

    /// `git checkout -b <name>` from the current `HEAD`.
    pub fn create_branch(&self, name: &str) -> GitResult<()> {
        branch::create_branch(&self.inner, name)
    }

    /// `git branch -d <name>` (`-D` when `force`).
    pub fn delete_branch(&self, name: &str, force: bool) -> GitResult<()> {
        branch::delete_branch(&self.inner, name, force)
    }

    /// Fast-forward `name` to its upstream, checked out or not.
    pub fn fast_forward(&self, name: &str) -> GitResult<()> {
        branch::fast_forward(&self.inner, name)
    }

    /// `git checkout -b <name> <hash> --no-track`; `hash` may be a ref.
    pub fn create_branch_at(&self, name: &str, hash: &str) -> GitResult<()> {
        branch::create_branch_at(&self.inner, name, hash)
    }

    /// `git branch -m <old> <new>`.
    pub fn rename_branch(&self, old: &str, new: &str) -> GitResult<()> {
        branch::rename_branch(&self.inner, old, new)
    }

    /// `git merge --no-ff <name>`: always a merge commit.
    pub fn merge_branch_no_ff(&self, name: &str) -> GitResult<MergeOutcome> {
        branch::merge_branch_no_ff(&self.inner, name)
    }

    /// `git merge --squash <name>`, then a commit when `commit` is set.
    pub fn merge_squash(&self, name: &str, commit: bool) -> GitResult<()> {
        branch::merge_squash(&self.inner, name, commit)
    }

    /// Take one side of a conflicted file whole (`checkout --ours|--theirs`).
    pub fn take_side(&self, path: &Path, ours: bool) -> GitResult<()> {
        apply::take_side(&self.inner, path, ours)
    }

    /// `git merge <name>` into the current branch.
    pub fn merge_branch(&self, name: &str) -> GitResult<MergeOutcome> {
        branch::merge_branch(&self.inner, name)
    }

    /// Create the repository on GitHub through `gh` and add it as `origin`
    /// (not pushed). Slow and network-crossing: call it off the main thread.
    /// See `docs/PLAN_15_CREATE_REMOTE.md`.
    pub fn create_repo(
        &self,
        gh: &GhProgram,
        req: &CreateRequest,
        cancel: &AtomicBool,
    ) -> GitResult<CreatedRepo> {
        host::create_repo(&self.inner, gh, req, cancel)
    }

    /// Point this handle's `git config` calls at `global` as the global file
    /// and an empty system file, so a test can write the global scope without
    /// touching the user's real `~/.gitconfig`. Nothing else is affected.
    pub fn isolate_config(&mut self, global: &Path) {
        self.config_global = Some(global.to_path_buf());
    }

    fn config_envs(&self) -> Vec<(&'static str, &std::ffi::OsStr)> {
        self.config_global
            .as_deref()
            .map_or_else(Vec::new, |global| {
                vec![
                    ("GIT_CONFIG_GLOBAL", global.as_os_str()),
                    ("GIT_CONFIG_SYSTEM", std::ffi::OsStr::new("/dev/null")),
                ]
            })
    }

    /// Every git config value with its scope and origin.
    /// See `docs/PLAN_14_GIT_CONFIG.md`.
    pub fn config(&self) -> GitResult<ConfigView> {
        config::read(&self.inner, &self.config_envs())
    }

    /// `git config <scope> <key> <value>`; git validates a typed value.
    pub fn config_set(
        &self,
        scope: WriteScope,
        key: &str,
        value: &str,
        kind: ValueKind,
    ) -> GitResult<()> {
        config::set(&self.inner, &self.config_envs(), scope, key, value, kind)
    }

    /// `git config --add`: one more value for a multi-valued key.
    pub fn config_add(
        &self,
        scope: WriteScope,
        key: &str,
        value: &str,
        kind: ValueKind,
    ) -> GitResult<()> {
        config::add(&self.inner, &self.config_envs(), scope, key, value, kind)
    }

    /// `git config --replace-all`: every value of the key in `scope` becomes this one.
    pub fn config_replace_all(
        &self,
        scope: WriteScope,
        key: &str,
        value: &str,
        kind: ValueKind,
    ) -> GitResult<()> {
        config::replace_all(&self.inner, &self.config_envs(), scope, key, value, kind)
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
        config::replace_value(
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
        config::unset_value(&self.inner, &self.config_envs(), scope, key, old)
    }

    /// `git config --unset-all`: drop the key from `scope` only.
    pub fn config_unset(&self, scope: WriteScope, key: &str) -> GitResult<()> {
        config::unset(&self.inner, &self.config_envs(), scope, key)
    }

    /// `git stash push --include-untracked`. See `docs/PLAN_10_STASH.md`.
    pub fn stash_push(&self, message: &str) -> GitResult<()> {
        stash::push(&self.inner, message, false)
    }

    /// Like `stash_push` but the index stays staged (`--keep-index`).
    pub fn stash_push_keeping_index(&self, message: &str) -> GitResult<()> {
        stash::push(&self.inner, message, true)
    }

    /// Give the stash entry with this oid a new message; it becomes `stash@{0}`.
    pub fn stash_rename(&mut self, oid: &str, message: &str) -> GitResult<()> {
        stash::rename(&mut self.inner, oid, message)
    }

    /// `git stash apply` for the entry with this oid; the entry stays.
    pub fn stash_apply(&mut self, oid: &str) -> GitResult<StashOutcome> {
        stash::apply(&mut self.inner, oid)
    }

    /// `git stash pop` for the entry with this oid.
    pub fn stash_pop(&mut self, oid: &str) -> GitResult<StashOutcome> {
        stash::pop(&mut self.inner, oid)
    }

    /// `git stash drop` for the entry with this oid.
    pub fn stash_drop(&mut self, oid: &str) -> GitResult<()> {
        stash::drop_entry(&mut self.inner, oid)
    }

    /// The entry's stat and patch under `header`, for the right pane.
    pub fn stash_diff(&self, oid: &str, header: &str, opts: DiffOpts) -> GitResult<Diff> {
        diff::stash_diff(&self.inner, oid, header, opts)
    }

    /// Reword, drop, edit, squash or fixup the commit `hash` with one
    /// `git rebase -i`. See `docs/PLAN_11_REBASE.md`.
    pub fn rebase_edit(&self, hash: &str, edit: &RebaseEdit) -> GitResult<OperationOutcome> {
        rebase::rebase_edit(&self.inner, hash, edit)
    }

    /// Fold every `fixup!` / `squash!` commit after `hash`'s parent into its
    /// target.
    pub fn autosquash(&self, hash: &str) -> GitResult<OperationOutcome> {
        rebase::autosquash(&self.inner, hash)
    }

    /// The full message of a commit, for pre-filling a reword.
    pub fn commit_message(&self, hash: &str) -> GitResult<String> {
        rebase::commit_message(&self.inner, hash)
    }

    /// Continue, skip or abort the operation git is stopped in.
    pub fn operation_step(&self, step: Step) -> GitResult<OperationOutcome> {
        operation::step(&self.inner, step)
    }

    /// The merge, rebase, cherry-pick or revert git is stopped in, if any.
    pub fn operation(&self) -> Option<model::Operation> {
        operation::current(&self.inner)
    }

    /// Configured remotes, alphabetical. See `docs/PLAN_9_REMOTE.md`.
    pub fn remotes(&self) -> GitResult<Vec<RemoteEntry>> {
        remote::remotes(&self.inner)
    }

    /// `git fetch <remote>`, or every remote when `remote` is `None`. Slow:
    /// run off the main thread, see `docs/PLAN_9_REMOTE.md` "Approach part 2".
    pub fn fetch(&self, remote: Option<&str>) -> GitResult<String> {
        remote::fetch(&self.inner, remote)
    }

    pub(crate) fn fetch_cancellable(
        &self,
        remote: Option<&str>,
        cancel: &AtomicBool,
    ) -> GitResult<String> {
        remote::fetch_cancellable(&self.inner, remote, cancel)
    }

    /// `git pull`, honouring the user's `pull.rebase`/`pull.ff` config. Slow,
    /// same as `fetch`.
    pub fn pull(&self) -> GitResult<String> {
        remote::pull(&self.inner)
    }

    pub(crate) fn pull_cancellable(&self, cancel: &AtomicBool) -> GitResult<String> {
        remote::pull_cancellable(&self.inner, cancel)
    }

    /// `git push`, or `git push -u <remote> <branch>` when `set_upstream` is
    /// `Some`. Slow, same as `fetch`.
    pub fn push(&self, set_upstream: Option<&str>) -> GitResult<String> {
        remote::push(&self.inner, set_upstream)
    }

    pub(crate) fn push_cancellable(
        &self,
        set_upstream: Option<&str>,
        upstream_branch: Option<&str>,
        force_with_lease: bool,
        set_upstream_current: bool,
        cancel: &AtomicBool,
    ) -> GitResult<String> {
        remote::push_cancellable(
            &self.inner,
            set_upstream,
            upstream_branch,
            force_with_lease,
            set_upstream_current,
            cancel,
        )
    }

    /// Whether `push.default` is `current`: a plain `git push` then creates the remote
    /// branch of the same name.
    pub fn push_default_current(&self) -> bool {
        self.inner
            .config()
            .ok()
            .and_then(|config| config.get_string("push.default").ok())
            .is_some_and(|value| value == "current")
    }
}

#[cfg(test)]
mod identity_tests {
    use super::unique_identities;
    use crate::git::identity::Identity;

    #[test]
    fn removes_duplicate_identity_pairs_and_preserves_first_seen_order() {
        let max = Identity {
            name: "Max Wells".to_owned(),
            email: Some("max@example.com".to_owned()),
        };
        let username = Identity {
            name: "Username".to_owned(),
            email: Some("user@example.com".to_owned()),
        };
        let distinct_email = Identity {
            name: "Max Wells".to_owned(),
            email: Some("other@example.com".to_owned()),
        };

        assert_eq!(
            unique_identities([
                max.clone(),
                username.clone(),
                max.clone(),
                distinct_email.clone(),
            ]),
            [max, username, distinct_email]
        );
    }
}
