//! The `git2` and subprocess half of `crate::git::apply`: the types are there.

use crate::git::apply::{ApplyDir, ApplyTarget, transform_body};
use crate::git::error::{GitError, GitResult};
use crate::git::repo::exec;
use crate::git::repo::read::{stderr, workdir};
use git2::Repository;
use std::collections::BTreeSet;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

// --- apply ---
/// `git -C <workdir> <...args>`, run to completion. One owned `Command` built
/// up by the caller (rather than a match returning different builder chains)
/// so a conditional flag never fights the borrow checker over a temporary.
fn run_git(workdir: &Path, build: impl FnOnce(&mut Command)) -> GitResult<()> {
    let mut cmd = exec::git(workdir);
    build(&mut cmd);
    let out = exec::output(&mut cmd)
        .map_err(|e| GitError::ApplyFailed(format!("cannot run git: {e}")))?;
    if !out.status.success() {
        return Err(GitError::ApplyFailed(stderr(&out)));
    }
    Ok(())
}

/// Stage or unstage a whole file. No patch: `git add` / `git restore
/// --staged`. Untracked files stage the same way `git add` always has.
pub(crate) fn stage_file(repo: &Repository, path: &Path, dir: ApplyDir) -> GitResult<()> {
    let workdir = workdir(repo)?;
    run_git(workdir, |cmd| {
        match dir {
            ApplyDir::Forward => cmd.arg("add"),
            ApplyDir::Reverse => cmd.arg("restore").arg("--staged"),
        };
        cmd.arg("--").arg(path);
    })
}

/// Stage or unstage every changed file in one call (`a`, `docs/PLAN_6_STAGING.md`
/// milestone S4): `git add -A` / `git restore --staged .`.
pub(crate) fn stage_all(repo: &Repository, dir: ApplyDir) -> GitResult<()> {
    let workdir = workdir(repo)?;
    run_git(workdir, |cmd| {
        match dir {
            ApplyDir::Forward => cmd.arg("add").arg("-A"),
            ApplyDir::Reverse => cmd.arg("restore").arg("--staged").arg("."),
        };
    })
}

/// `git add -A` for everything except `excluded`, each path named literally
/// (no glob or magic in a path such as `we ird [1].txt`). An excluded path
/// that is unmerged stays unmerged. See `has_conflict_markers`.
///
/// The paths are listed rather than excluded with `:(exclude,literal)<path>`:
/// git 2.50 stages an unmerged path that a pathspec excludes, which is the
/// marker-guard bypass this function exists to prevent.
pub(crate) fn stage_all_except(repo: &Repository, excluded: &[PathBuf]) -> GitResult<()> {
    let workdir = workdir(repo)?;
    let mut list = exec::git(workdir);
    list.args(["ls-files", "-z", "-m", "-d", "-o", "--exclude-standard"]);
    let out = exec::output(&mut list)
        .map_err(|e| GitError::ApplyFailed(format!("cannot run git: {e}")))?;
    if !out.status.success() {
        return Err(GitError::ApplyFailed(stderr(&out)));
    }
    let listed = String::from_utf8_lossy(&out.stdout);
    let paths: BTreeSet<&str> = listed
        .split('\0')
        .filter(|p| !p.is_empty() && !excluded.iter().any(|e| e.to_str() == Some(p)))
        .collect();
    if paths.is_empty() {
        return Ok(());
    }
    run_git(workdir, |cmd| {
        cmd.arg("--literal-pathspecs")
            .args(["add", "-A", "--"])
            .args(&paths);
    })
}

/// `git checkout --ours|--theirs -- <path>`: take one side of a conflicted
/// file whole. The path stays unmerged until it is staged, which the marker
/// guard now allows because the markers are gone.
pub(crate) fn take_side(repo: &Repository, path: &Path, ours: bool) -> GitResult<()> {
    let workdir = workdir(repo)?;
    run_git(workdir, |cmd| {
        cmd.arg("checkout")
            .arg(if ours { "--ours" } else { "--theirs" })
            .arg("--")
            .arg(path);
    })
}

/// Does `path` still hold merge conflict markers? True when the file has a
/// line starting `<<<<<<<` and a line starting `>>>>>>>`. A lone `=======`
/// is not enough: it is also a Markdown heading underline. A file that no
/// longer exists (deleted on one side of the conflict) has none.
///
/// `git add` on an unmerged path marks it resolved whatever the file holds,
/// so ferrit checks this first (`docs/PLAN_11_REBASE.md` R0).
pub(crate) fn has_conflict_markers(repo: &Repository, path: &Path) -> GitResult<bool> {
    let full = workdir(repo)?.join(path);
    let bytes = match std::fs::read(&full) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => {
            return Err(GitError::ApplyFailed(format!(
                "cannot read {}: {e}",
                path.display()
            )));
        },
    };
    let text = String::from_utf8_lossy(&bytes);
    let starts = |marker: &str| text.lines().any(|line| line.starts_with(marker));
    Ok(starts("<<<<<<<") && starts(">>>>>>>"))
}

/// Discard a whole file's worktree change: `git restore --worktree` for a
/// tracked path, `git clean -f` for one git has never seen (there is nothing
/// to restore it *to*). Never touches the index.
pub(crate) fn discard_file(repo: &Repository, path: &Path, untracked: bool) -> GitResult<()> {
    let workdir = workdir(repo)?;
    run_git(workdir, |cmd| {
        if untracked {
            cmd.arg("clean").arg("-f").arg("-q");
        } else {
            cmd.arg("restore").arg("--worktree");
        }
        cmd.arg("--").arg(path);
    })
}

/// Stage / unstage / discard one hunk. `patch` is `file.header.start
/// .. hunk.body.end` over a `Diff::text`, handed in by the caller so this
/// module never re-runs the diff. The hunk's own `@@ -a,b +c,d @@` counts are
/// already correct, so no `--recount`.
pub(crate) fn apply_hunk(
    repo: &Repository,
    patch: &str,
    dir: ApplyDir,
    target: ApplyTarget,
) -> GitResult<()> {
    run_apply(repo, patch, dir, target, false)
}

/// Stage / unstage / discard a set of body lines within one hunk. `lines`
/// are 0-based indices into `hunk_body`'s own lines (`split_inclusive('\n')`,
/// context lines counted but never selected). Builds the line-subset patch
/// (`transform_body`), which leaves the hunk header's counts wrong by
/// construction, so this always passes `--recount` and lets `git` recompute
/// them.
pub(crate) fn apply_lines(
    repo: &Repository,
    file_header: &str,
    hunk_header: &str,
    hunk_body: &str,
    lines: &[usize],
    dir: ApplyDir,
    target: ApplyTarget,
) -> GitResult<()> {
    let body = transform_body(hunk_body, lines);
    let patch = format!("{file_header}{hunk_header}\n{body}");
    run_apply(repo, &patch, dir, target, true)
}

/// Pipe `patch` to `git apply`, mapping `target`/`dir` to its flags. Non-zero
/// exit becomes `ApplyFailed(stderr)`; `git apply` is atomic per invocation,
/// so a failure leaves the index and worktree untouched.
fn run_apply(
    repo: &Repository,
    patch: &str,
    dir: ApplyDir,
    target: ApplyTarget,
    recount: bool,
) -> GitResult<()> {
    let workdir = workdir(repo)?;
    let mut args = vec!["apply".to_owned()];
    match target {
        ApplyTarget::Index => args.push("--cached".to_owned()),
        ApplyTarget::Worktree => {},
    }
    if dir == ApplyDir::Reverse {
        args.push("--reverse".to_owned());
    }
    if recount {
        args.push("--recount".to_owned());
    }

    let mut cmd = exec::git(workdir);
    cmd.args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let tracked = exec::track(&cmd);
    let mut child = cmd
        .spawn()
        .map_err(|e| GitError::ApplyFailed(format!("cannot run git: {e}")))?;

    let mut stdin = child
        .stdin
        .take()
        .ok_or_else(|| GitError::ApplyFailed("no stdin pipe to git apply".to_owned()))?;
    stdin
        .write_all(patch.as_bytes())
        .map_err(|e| GitError::ApplyFailed(format!("cannot write patch: {e}")))?;
    drop(stdin);

    let out = child
        .wait_with_output()
        .map_err(|e| GitError::ApplyFailed(format!("cannot run git: {e}")))?;
    tracked.finish(out.status.code());
    if !out.status.success() {
        return Err(GitError::ApplyFailed(stderr(&out)));
    }
    Ok(())
}
