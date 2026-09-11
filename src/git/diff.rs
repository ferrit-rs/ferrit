//! `git diff` / `git show` run as a subprocess, lazygit style.
//!
//! lazygit never computes diffs with a library: it shells out and renders the
//! output, so the user's `git config` (`diff.algorithm`, `diff.noprefix`, ...)
//! is honoured for free. ferrit does the same. See `docs/PLAN_3_DIFF_VIEW.md`.
//!
//! Nothing here imports `ratatui`. The parser (`parse.rs`) hands back byte
//! `Range`s over one owned `String`, exactly like gitu's public `Diff`.

use std::ops::Range;
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

    /// Old/new line number for every line of `text`; `None` on a line that has
    /// no number of its own (file/hunk headers, `\ No newline at end of file`).
    /// An addition carries only a new number, a deletion only an old one,
    /// context carries both. Same derivation as lazygit's patch line-number
    /// gutter (`commands/patch/parse.go`), from the counters each hunk header
    /// already gives us (`HunkMeta::old_start`/`new_start`).
    pub fn line_numbers(&self) -> Vec<(Option<u32>, Option<u32>)> {
        let mut out = vec![(None, None); self.text.lines().count()];
        for hunk in self.files.iter().flat_map(|f| &f.hunks) {
            let mut old = hunk.old_start;
            let mut new = hunk.new_start;
            let start_line = line_of(&self.text, hunk.body.start);
            let body = self.text.get(hunk.body.clone()).unwrap_or_default();
            for (i, line) in body.lines().enumerate() {
                let Some(slot) = out.get_mut(start_line + i) else {
                    continue;
                };
                *slot = match line.as_bytes().first() {
                    Some(b'+') => {
                        let n = new;
                        new += 1;
                        (None, Some(n))
                    },
                    Some(b'-') => {
                        let n = old;
                        old += 1;
                        (Some(n), None)
                    },
                    Some(b'\\') => (None, None),
                    _ => {
                        let n = (old, new);
                        old += 1;
                        new += 1;
                        (Some(n.0), Some(n.1))
                    },
                };
            }
        }
        out
    }

    /// Byte range, within a line's content *after* its leading `+`/`-`, that
    /// actually changed, for a line that pairs with a corresponding line on
    /// the other side of a modification. `None` for context lines, headers,
    /// and an unpaired excess line in an unequal-count add/remove block.
    ///
    /// Pairing follows lazygit / diff-highlight: a contiguous run of `-`
    /// lines directly followed by a contiguous run of `+` lines is one
    /// "change block"; the i-th removed line pairs with the i-th added line
    /// (extra lines on the longer side are left unpaired). Each pair is then
    /// trimmed of its common prefix and suffix (by `char`, not byte, to stay
    /// on UTF-8 boundaries) to isolate what changed, the same technique a
    /// word-diff highlight needs, without pulling in a diff library.
    pub fn word_diff_ranges(&self) -> Vec<Option<Range<usize>>> {
        let lines: Vec<&str> = self.text.lines().collect();
        let mut out = vec![None; lines.len()];
        let mut i = 0;
        while i < lines.len() {
            if !lines.get(i).is_some_and(|l| l.starts_with('-')) {
                i += 1;
                continue;
            }
            let mut del_end = i;
            while lines.get(del_end).is_some_and(|l| l.starts_with('-')) {
                del_end += 1;
            }
            let add_start = del_end;
            let mut add_end = add_start;
            while lines.get(add_end).is_some_and(|l| l.starts_with('+')) {
                add_end += 1;
            }
            let pairs = (del_end - i).min(add_end - add_start);
            for p in 0..pairs {
                let old_body = lines.get(i + p).and_then(|l| l.get(1..)).unwrap_or_default();
                let new_body = lines
                    .get(add_start + p)
                    .and_then(|l| l.get(1..))
                    .unwrap_or_default();
                let (prefix, suffix) = common_affixes(old_body, new_body);
                if let Some(slot) = out.get_mut(i + p) {
                    *slot = Some(prefix..old_body.len() - suffix);
                }
                if let Some(slot) = out.get_mut(add_start + p) {
                    *slot = Some(prefix..new_body.len() - suffix);
                }
            }
            i = add_end.max(del_end);
        }
        out
    }

    /// Files changed, insertions and deletions, lazygit/git-shortstat style.
    /// Derived from `line_numbers()`: a line with only a new number is an
    /// insertion, a line with only an old number is a deletion.
    pub fn stat(&self) -> DiffStat {
        let mut insertions = 0;
        let mut deletions = 0;
        for (old, new) in self.line_numbers() {
            match (old, new) {
                (None, Some(_)) => insertions += 1,
                (Some(_), None) => deletions += 1,
                _ => {},
            }
        }
        DiffStat {
            files: self.files.len(),
            insertions,
            deletions,
        }
    }
}

/// Shortstat summary for the stat line above a diff: `N file(s) changed, X
/// insertion(s)(+), Y deletion(s)(-)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiffStat {
    pub files: usize,
    pub insertions: usize,
    pub deletions: usize,
}

/// Length, in bytes, of the common prefix and (non-overlapping) common
/// suffix of `a` and `b`, split on `char` boundaries. `a.len() - suffix` and
/// `b.len() - suffix` are always `>= prefix`, so `prefix..len - suffix` is
/// always a valid range into either string.
fn common_affixes(a: &str, b: &str) -> (usize, usize) {
    let prefix = a
        .char_indices()
        .zip(b.chars())
        .take_while(|&((_, ca), cb)| ca == cb)
        .last()
        .map_or(0, |((i, ca), _)| i + ca.len_utf8());

    let a_rest = a.get(prefix..).unwrap_or_default();
    let b_rest = b.get(prefix..).unwrap_or_default();

    let suffix = a_rest
        .chars()
        .rev()
        .zip(b_rest.chars().rev())
        .take_while(|(ca, cb)| ca == cb)
        .map(|(ca, _)| ca.len_utf8())
        .sum();

    (prefix, suffix)
}

fn line_of(text: &str, byte: usize) -> usize {
    let end = byte.min(text.len());
    #[expect(
        clippy::naive_bytecount,
        reason = "counts newlines in a header-length prefix; a bytecount dep is overkill for one call"
    )]
    text.as_bytes()
        .get(..end)
        .unwrap_or_default()
        .iter()
        .filter(|&&b| b == b'\n')
        .count()
}

/// Parse plain `git diff` text directly, no subprocess. For tests and any
/// future caller that already holds diff output.
pub fn parse_diff(text: &str) -> Diff {
    Diff::new(text.to_owned())
}

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

    let out = DiffCmd::base("show", opts)
        .arg("-m")
        .arg("--first-parent")
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

/// Is `path` untracked (worktree-new) in `repo`? Decides whether an empty
/// `git diff` means "nothing changed" or "needs the `--no-index` fallback".
fn is_untracked(repo: &Repository, path: &Path) -> bool {
    repo.status_file(path)
        .is_ok_and(|s| s.contains(git2::Status::WT_NEW))
}

fn workdir(repo: &Repository) -> GitResult<&Path> {
    repo.workdir()
        .ok_or_else(|| GitError::DiffFailed("bare repository has no working tree".to_owned()))
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).trim().to_owned()
}

/// Shared argv builder. The flag set lives here once so an `--ext-diff` /
/// `-c diff.external=` addition later is a one-line change.
struct DiffCmd {
    args: Vec<String>,
}

impl DiffCmd {
    fn base(sub: &str, opts: DiffOpts) -> Self {
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
