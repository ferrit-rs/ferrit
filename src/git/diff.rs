//! `git diff` / `git show` run as a subprocess, lazygit style.
//!
//! lazygit never computes diffs with a library: it shells out and renders the
//! output, so the user's `git config` (`diff.algorithm`, `diff.noprefix`, ...)
//! is honoured for free. ferrit does the same. See `docs/PLAN_3_DIFF_VIEW.md`.
//!
//! Nothing here imports `ratatui`. The parser (`parse.rs`) hands back byte
//! `Range`s over one owned `String`, exactly like gitu's public `Diff`.

use std::path::Path;
use std::process::{Command, Output};

use git2::Repository;

use crate::git::error::{GitError, GitResult};

mod parse;

pub use parse::{FileMeta, FileStatus, HunkMeta};

/// Which pair of trees `git diff` compares. Deliberately not a reuse of
/// `blob::Rev`: that names one version of one path, this names a pair of trees.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiffSide {
    /// `git diff`: working tree vs index.
    Worktree,
    /// `git diff --cached`: index vs HEAD.
    Staged,
}

/// Diff knobs, mirroring lazygit's `git.*` config with the same defaults, so
/// behaviour is familiar out of the box.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiffOpts {
    /// Context lines, `--unified`. lazygit `git.diffContextSize` (default 3).
    pub context: u32,
    /// `--ignore-all-space`. lazygit `git.ignoreWhitespaceInDiffView` (false).
    pub ignore_whitespace: bool,
    /// `--find-renames=<n>%`. lazygit `git.renameSimilarityThreshold` (50).
    pub rename_threshold: u32,
}

impl Default for DiffOpts {
    fn default() -> Self {
        Self {
            context: 3,
            ignore_whitespace: false,
            rename_threshold: 50,
        }
    }
}

/// One parsed diff: the raw plain-text `git` output plus byte-range metadata
/// into it. Holds every file the diff touched (a `file_diff` yields one).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diff {
    pub text: String,
    pub files: Vec<FileMeta>,
}

impl Diff {
    fn new(text: String) -> Self {
        let files = parse::parse(&text);
        Self { text, files }
    }

    /// Line index (0-based) of every hunk header, file-then-hunk order. `]` /
    /// `[` move the right-pane scroll between these.
    pub fn hunk_lines(&self) -> Vec<usize> {
        self.files
            .iter()
            .flat_map(|f| &f.hunks)
            .map(|h| line_of(&self.text, h.header.start))
            .collect()
    }

    /// Line index (0-based) of every `diff --git` header.
    pub fn file_lines(&self) -> Vec<usize> {
        self.files
            .iter()
            .map(|f| line_of(&self.text, f.header.start))
            .collect()
    }
}

fn line_of(text: &str, byte: usize) -> usize {
    text.as_bytes()[..byte.min(text.len())]
        .iter()
        .filter(|&&b| b == b'\n')
        .count()
}

/// Parse plain `git diff` text directly, no subprocess. For tests and any
/// future caller that already holds diff output.
pub fn parse_diff(text: &str) -> Diff {
    Diff::new(text.to_string())
}

/// One file's worktree-or-staged diff.
pub fn file_diff(
    repo: &Repository,
    path: &Path,
    side: DiffSide,
    opts: &DiffOpts,
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
            Some(0) | Some(1) => {}
            _ => return Err(GitError::DiffFailed(stderr(&ni))),
        }
        return Ok(Diff::new(String::from_utf8_lossy(&ni.stdout).into_owned()));
    }

    Ok(Diff::new(text))
}

/// A commit against its first parent (`git show`). Empty-tree diff for the root
/// commit; first-parent diff for a merge (`-m --first-parent`). `hash` is a
/// `CommitEntry::full_hash`.
pub fn commit_diff(repo: &Repository, hash: &str, opts: &DiffOpts) -> GitResult<Diff> {
    let workdir = workdir(repo)?;

    let out = DiffCmd::base("show", opts)
        .arg("-m")
        .arg("--first-parent")
        .arg("-p")
        .arg(hash.to_string())
        .run(workdir)?;

    if !out.status.success() {
        let err = stderr(&out);
        if err.contains("bad object")
            || err.contains("unknown revision")
            || err.contains("ambiguous argument")
        {
            return Err(GitError::NoSuchCommit(hash.to_string()));
        }
        return Err(GitError::DiffFailed(err));
    }
    Ok(Diff::new(String::from_utf8_lossy(&out.stdout).into_owned()))
}

/// Is `path` untracked (worktree-new) in `repo`? Decides whether an empty
/// `git diff` means "nothing changed" or "needs the `--no-index` fallback".
fn is_untracked(repo: &Repository, path: &Path) -> bool {
    repo.status_file(path)
        .map(|s| s.contains(git2::Status::WT_NEW))
        .unwrap_or(false)
}

fn workdir(repo: &Repository) -> GitResult<&Path> {
    repo.workdir()
        .ok_or_else(|| GitError::DiffFailed("bare repository has no working tree".to_string()))
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).trim().to_string()
}

/// Shared argv builder. The flag set lives here once so an `--ext-diff` /
/// `-c diff.external=` addition later is a one-line change.
struct DiffCmd {
    args: Vec<String>,
}

impl DiffCmd {
    fn base(sub: &str, opts: &DiffOpts) -> Self {
        let mut args = vec![
            sub.to_string(),
            "--no-ext-diff".to_string(),
            "--color=never".to_string(),
            format!("--unified={}", opts.context),
            format!("--find-renames={}%", opts.rename_threshold),
            "--submodule".to_string(),
        ];
        if opts.ignore_whitespace {
            args.push("--ignore-all-space".to_string());
        }
        Self { args }
    }

    fn arg(mut self, a: impl Into<String>) -> Self {
        self.args.push(a.into());
        self
    }

    fn run(self, workdir: &Path) -> GitResult<Output> {
        Command::new("git")
            .arg("-C")
            .arg(workdir)
            .args(&self.args)
            .output()
            .map_err(|e| GitError::DiffFailed(format!("cannot run git: {e}")))
    }
}
