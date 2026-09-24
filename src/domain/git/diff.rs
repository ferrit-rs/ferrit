//! `git diff` / `git show` run as a subprocess, lazygit style.
//!
//! lazygit never computes diffs with a library: it shells out and renders the
//! output, so the user's `git config` (`diff.algorithm`, `diff.noprefix`, ...)
//! is honoured for free. ferrit does the same. See `docs/PLAN_3_DIFF_VIEW.md`.
//!
//! Nothing here imports `ratatui`. The parser (`parse.rs`) hands back byte
//! `Range`s over one owned `String`, exactly like gitu's public `Diff`.

use std::io::Write as _;
use std::path::Path;
use std::process::{Command, Output, Stdio};

use git2::Repository;

use self::parse::FileMeta;
use crate::domain::git::error::{GitError, GitResult};
use crate::domain::git::exec;

pub mod parse;

/// Which pair of trees `git diff` compares. Deliberately not a reuse of
/// `blob::Rev`: that names one version of one path, this names a pair of trees.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DiffSide {
    /// `git diff`: working tree vs index.
    #[default]
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

    /// File extension (no dot) covering each line of `text`, for a
    /// per-language syntax highlighter. `None` on a line outside any file
    /// section, or whose file has no extension. Deletions read the extension
    /// off `old_path` (`new_path` is `/dev/null`); everything else reads it
    /// off `new_path`.
    pub fn line_extensions(&self) -> Vec<Option<String>> {
        let total = self.text.lines().count();
        let mut out = vec![None; total];
        for (i, file) in self.files.iter().enumerate() {
            let new_path = self.text.get(file.new_path.clone()).unwrap_or_default();
            let old_path = self.text.get(file.old_path.clone()).unwrap_or_default();
            let path = if new_path == "/dev/null" {
                old_path
            } else {
                new_path
            };
            let Some(ext) = Path::new(path).extension().and_then(|e| e.to_str()) else {
                continue;
            };
            let start = line_of(&self.text, file.header.start);
            let end = self
                .files
                .get(i + 1)
                .map_or(total, |f| line_of(&self.text, f.header.start));
            for slot in out.get_mut(start..end).into_iter().flatten() {
                *slot = Some(ext.to_owned());
            }
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

    /// Render raw diff through delta's pager-compatible formatter. This keeps
    /// ferrit's parser read-only while matching lazygit's configured diff
    /// appearance: file markers, line gutters, word highlights, and blocks.
    /// Missing delta is normal; callers fall back to ferrit's native renderer.
    pub fn delta_output(&self, width: usize) -> Option<String> {
        let mut child = Command::new("delta")
            .args([
                "--paging=never".to_owned(),
                "--line-numbers".to_owned(),
                format!("--width={width}"),
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        let mut stdin = child.stdin.take()?;
        let text = self.text.clone();
        // A large diff overflows the pipe buffer (~64KB on macOS): writing
        // stdin fully before reading stdout deadlocks once delta blocks on
        // its own full stdout buffer while this thread is still blocked on
        // stdin. Write from a second thread so both pipes drain at once.
        let writer = std::thread::spawn(move || stdin.write_all(text.as_bytes()));
        let output = child.wait_with_output().ok()?;
        let _ = writer.join();
        output
            .status
            .success()
            .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
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

/// A stash entry's patch (`git stash show -p`), untracked files included.
/// `oid` is a `StashEntry::oid`; git accepts a stash-like commit directly,
/// so a shifted `stash@{n}` cannot make this stale.
pub(super) fn stash_diff(repo: &Repository, oid: &str, opts: DiffOpts) -> GitResult<Diff> {
    // `git stash show` wants its own verb before the diff flags.
    let out = DiffCmd::base("stash", opts)
        .after_subcommand("show")
        .arg("-p")
        .arg("--include-untracked")
        .arg(oid.to_owned())
        .run(workdir(repo)?)?;
    if !out.status.success() {
        return Err(GitError::DiffFailed(stderr(&out)));
    }
    Ok(Diff::new(String::from_utf8_lossy(&out.stdout).into_owned()))
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
