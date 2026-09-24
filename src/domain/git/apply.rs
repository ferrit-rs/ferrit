//! Write the index (and, for discard, the worktree) by piping a patch built
//! from `Diff` byte ranges to `git apply`, or by shelling out to `git add` /
//! `restore` / `clean` for whole-file moves. See `docs/PLAN_6_STAGING.md`.
//!
//! Same rule as the rest of `git::`: no `ratatui`, `git` does the applying
//! (fuzz, whitespace policy, `core.autocrlf` are its problem, not ours), one
//! subprocess per action, no long-lived patch-builder state.

use std::collections::HashSet;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use git2::Repository;

use crate::domain::git::diff::{stderr, workdir};
use crate::domain::git::error::{GitError, GitResult};

/// Which way a patch runs: stage / discard read forward, unstage reads
/// `git apply --reverse`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyDir {
    Forward,
    Reverse,
}

/// What the patch touches. `Index` alone is stage/unstage; `Worktree` /
/// `WorktreeAndIndex` are discard (the worktree side, optionally keeping the
/// index in step).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyTarget {
    Index,
    Worktree,
    WorktreeAndIndex,
}

/// `git -C <workdir> <...args>`, run to completion. One owned `Command` built
/// up by the caller (rather than a match returning different builder chains)
/// so a conditional flag never fights the borrow checker over a temporary.
fn run_git(workdir: &Path, build: impl FnOnce(&mut Command)) -> GitResult<()> {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(workdir);
    build(&mut cmd);
    let out = cmd
        .output()
        .map_err(|e| GitError::ApplyFailed(format!("cannot run git: {e}")))?;
    if !out.status.success() {
        return Err(GitError::ApplyFailed(stderr(&out)));
    }
    Ok(())
}

/// Stage or unstage a whole file. No patch: `git add` / `git restore
/// --staged`. Untracked files stage the same way `git add` always has.
pub(super) fn stage_file(repo: &Repository, path: &Path, dir: ApplyDir) -> GitResult<()> {
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
pub(super) fn stage_all(repo: &Repository, dir: ApplyDir) -> GitResult<()> {
    let workdir = workdir(repo)?;
    run_git(workdir, |cmd| {
        match dir {
            ApplyDir::Forward => cmd.arg("add").arg("-A"),
            ApplyDir::Reverse => cmd.arg("restore").arg("--staged").arg("."),
        };
    })
}

/// `git add -A` for everything except `excluded`, each matched literally
/// (no glob or magic in a path such as `we ird [1].txt`). An excluded path
/// that is unmerged stays unmerged. See `has_conflict_markers`.
pub(super) fn stage_all_except(repo: &Repository, excluded: &[PathBuf]) -> GitResult<()> {
    let workdir = workdir(repo)?;
    run_git(workdir, |cmd| {
        cmd.arg("add").arg("-A").arg("--").arg(".");
        for path in excluded {
            let mut spec = std::ffi::OsString::from(":(exclude,literal)");
            spec.push(path.as_os_str());
            cmd.arg(spec);
        }
    })
}

/// Does `path` still hold merge conflict markers? True when the file has a
/// line starting `<<<<<<<` and a line starting `>>>>>>>`. A lone `=======`
/// is not enough: it is also a Markdown heading underline. A file that no
/// longer exists (deleted on one side of the conflict) has none.
///
/// `git add` on an unmerged path marks it resolved whatever the file holds,
/// so ferrit checks this first (`docs/PLAN_11_REBASE.md` R0).
pub(super) fn has_conflict_markers(repo: &Repository, path: &Path) -> GitResult<bool> {
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
pub(super) fn discard_file(repo: &Repository, path: &Path, untracked: bool) -> GitResult<()> {
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
pub(super) fn apply_hunk(
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
pub(super) fn apply_lines(
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

/// Turn a hunk body into a valid, self-contained patch body covering only
/// `lines` (0-based indices into `body`'s own lines), gitu's
/// `format_line_patch` rule. Exposed for `tests/apply_patch.rs`, which checks
/// it byte-for-byte with no repo involved.
///
/// The same rule serves staging and unstaging: `git apply --reverse` (used
/// by unstage and by a worktree discard) flips which side of the patch is
/// "old" and which is "new", so a selected line kept verbatim always means
/// "this exact hunk line moves" in whichever direction is asked for, and an
/// unselected `+` dropped / unselected `-` demoted to context always means
/// "this line is untouched" on the side that has to stay fixed either way.
/// Only the CLI flag differs between the two directions, not this transform.
pub fn transform_body(body: &str, lines: &[usize]) -> String {
    let selected: HashSet<usize> = lines.iter().copied().collect();
    let mut out = String::with_capacity(body.len());
    // Did the immediately preceding non-`\` line survive the transform? A
    // trailing `\ No newline at end of file` marker is not itself
    // selectable: it rides along with the line above, dropped with it if
    // that line was an unselected `+`.
    let mut last_kept = true;
    for (i, line) in body.split_inclusive('\n').enumerate() {
        match line.as_bytes().first() {
            Some(b'+') if selected.contains(&i) => {
                out.push_str(line);
                last_kept = true;
            },
            Some(b'+') => last_kept = false,
            Some(b'-') if selected.contains(&i) => {
                out.push_str(line);
                last_kept = true;
            },
            Some(b'-') => {
                out.push(' ');
                out.push_str(line.get(1..).unwrap_or_default());
                last_kept = true;
            },
            Some(b'\\') => {
                if last_kept {
                    out.push_str(line);
                }
            },
            _ => {
                out.push_str(line);
                last_kept = true;
            },
        }
    }
    out
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
        ApplyTarget::WorktreeAndIndex => args.push("--index".to_owned()),
    }
    if dir == ApplyDir::Reverse {
        args.push("--reverse".to_owned());
    }
    if recount {
        args.push("--recount".to_owned());
    }

    let mut child = Command::new("git")
        .arg("-C")
        .arg(workdir)
        .args(&args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
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
    if !out.status.success() {
        return Err(GitError::ApplyFailed(stderr(&out)));
    }
    Ok(())
}
