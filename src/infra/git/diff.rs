//! The `git2` and subprocess half of `crate::domain::git::diff`: the types are there.

use std::path::Path;
use std::process::Output;

use git2::Repository;

use crate::domain::git::error::{GitError, GitResult};
use crate::domain::git::exec;

use crate::domain::git::diff::{Diff, DiffOpts, DiffSide};

/// One file's worktree-or-staged diff.
pub(super) fn file_diff(
    repo: &Repository,
    path: &Path,
    side: DiffSide,
    opts: DiffOpts,
) -> GitResult<Diff> {
    let workdir = workdir(repo)?;

    let mut cmd = DiffCmd::base("diff", opts);
    if side == DiffSide::Staged {
        cmd = cmd.arg("--cached");
    }
    let out = cmd
        .arg("--")
        .arg(path.to_string_lossy().into_owned())
        .run(workdir)?;
    if !out.status.success() {
        return Err(GitError::DiffFailed(stderr(&out)));
    }
    let text = String::from_utf8_lossy(&out.stdout).into_owned();

    // Untracked file: plain `git diff` prints nothing. Re-ask with --no-index
    // so it renders as all-additions, the way lazygit does. A tracked file
    // that simply has no worktree change also prints nothing: leave that one
    // empty, do not fake an all-additions diff for it.
    if text.trim().is_empty() && side == DiffSide::Worktree && is_untracked(repo, path) {
        let ni = DiffCmd::base("diff", opts)
            .arg("--no-index")
            .arg("--")
            .arg("/dev/null")
            .arg(path.to_string_lossy().into_owned())
            .run(workdir)?;
        // --no-index exits 1 when the files differ, which is the normal case.
        match ni.status.code() {
            Some(0 | 1) => {},
            _ => return Err(GitError::DiffFailed(stderr(&ni))),
        }
        return Ok(Diff::new(String::from_utf8_lossy(&ni.stdout).into_owned()));
    }

    Ok(Diff::new(text))
}

/// A commit against its first parent (`git show`). Empty-tree diff for the root
/// commit; first-parent diff for a merge (`-m --first-parent`). `hash` is a
/// `CommitEntry::full_hash`.
pub(super) fn commit_diff(repo: &Repository, hash: &str, opts: DiffOpts) -> GitResult<Diff> {
    let workdir = workdir(repo)?;

    // `--decorate` puts `(HEAD -> main, tag: v1, origin/main)` on the commit line and
    // `--stat` the per-file summary between the message and the patch, as lazygit's
    // Patch shows them.
    let out = DiffCmd::base("show", opts)
        .arg("-m")
        .arg("--first-parent")
        .arg("--decorate=short")
        .arg("--stat")
        .arg("-p")
        .arg(hash.to_owned())
        .run(workdir)?;

    if !out.status.success() {
        let err = stderr(&out);
        if err.contains("bad object")
            || err.contains("unknown revision")
            || err.contains("ambiguous argument")
        {
            return Err(GitError::NoSuchCommit(hash.to_owned()));
        }
        return Err(GitError::DiffFailed(err));
    }
    Ok(Diff::new(String::from_utf8_lossy(&out.stdout).into_owned()))
}

/// A stash entry's patch (`git stash show -p --stat`), untracked files included,
/// under lazygit's header: `header` (`stash@{0}: On main: msg`), a blank line,
/// the stat, a blank line, the patch. `oid` is a `StashEntry::oid`; git accepts a
/// stash-like commit directly, so a shifted `stash@{n}` cannot make this stale.
pub(super) fn stash_diff(
    repo: &Repository,
    oid: &str,
    header: &str,
    opts: DiffOpts,
) -> GitResult<Diff> {
    // `git stash show` wants its own verb before the diff flags.
    let out = DiffCmd::base("stash", opts)
        .after_subcommand("show")
        .arg("--stat")
        .arg("-p")
        .arg("--include-untracked")
        .arg(oid.to_owned())
        .run(workdir(repo)?)?;
    if !out.status.success() {
        return Err(GitError::DiffFailed(stderr(&out)));
    }
    Ok(Diff::new(format!(
        "{header}\n\n{}",
        String::from_utf8_lossy(&out.stdout)
    )))
}

/// Is `path` untracked (worktree-new) in `repo`? Decides whether an empty
/// `git diff` means "nothing changed" or "needs the `--no-index` fallback".
fn is_untracked(repo: &Repository, path: &Path) -> bool {
    repo.status_file(path)
        .is_ok_and(|s| s.contains(git2::Status::WT_NEW))
}

/// Shared with `apply.rs`, which runs `git apply` / `add` / `restore` /
/// `clean` against the same worktree.
pub(super) fn workdir(repo: &Repository) -> GitResult<&Path> {
    repo.workdir()
        .ok_or_else(|| GitError::DiffFailed("bare repository has no working tree".to_owned()))
}

/// Shared with `apply.rs`.
pub(super) fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).trim().to_owned()
}

/// Shared argv builder. The flag set lives here once so an `--ext-diff` /
/// `-c diff.external=` addition later is a one-line change.
struct DiffCmd {
    args: Vec<String>,
}

impl DiffCmd {
    pub(crate) fn base(sub: &str, opts: DiffOpts) -> Self {
        let mut args = vec![
            sub.to_owned(),
            "--no-ext-diff".to_owned(),
            "--color=never".to_owned(),
            format!("--unified={}", opts.context),
            format!("--find-renames={}%", opts.rename_threshold),
            "--submodule".to_owned(),
        ];
        if opts.ignore_whitespace {
            args.push("--ignore-all-space".to_owned());
        }
        Self { args }
    }

    /// Insert a verb right after the subcommand (`stash` `show` ...).
    fn after_subcommand(mut self, verb: &str) -> Self {
        self.args.insert(1, verb.to_owned());
        self
    }

    fn arg(mut self, a: impl Into<String>) -> Self {
        self.args.push(a.into());
        self
    }

    fn run(self, workdir: &Path) -> GitResult<Output> {
        exec::output(exec::git(workdir).args(&self.args))
            .map_err(|e| GitError::DiffFailed(format!("cannot run git: {e}")))
    }
}
