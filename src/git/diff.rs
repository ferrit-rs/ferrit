//! `git diff` / `git show` run as a subprocess, lazygit style.
//!
//! lazygit never computes diffs with a library: it shells out and renders the
//! output, so the user's `git config` (`diff.algorithm`, `diff.noprefix`, ...)
//! is honoured for free. ferrit does the same. See `docs/PLAN_3_DIFF_VIEW.md`.
//!
//! Nothing here imports `ratatui`. The parser (`parse.rs`) hands back byte
//! `Range`s over one owned `String`, exactly like gitu's public `Diff`.
//!
//! This file holds the types and the pure functions. The code that reads with
//! `git2` or runs `git` is `crate::git::repo::read`.

use self::parse::FileMeta;
use std::io::Write as _;
use std::path::Path;
use std::process::{Command, Stdio};

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
    /// The diff as git printed it. The ranges in `files` index into this.
    pub text: String,
    /// One entry per file the diff touches.
    pub files: Vec<FileMeta>,
}

impl Diff {
    pub(crate) fn new(text: String) -> Self {
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
    /// Missing delta is normal; callers fall back to ferrit's native renderer,
    /// which is also what `FERRIT_NO_DELTA` (set by the test runner) asks for.
    pub fn delta_output(&self, width: usize) -> Option<String> {
        if std::env::var_os("FERRIT_NO_DELTA").is_some() {
            return None;
        }
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
    /// Files changed.
    pub files: usize,
    /// Lines added.
    pub insertions: usize,
    /// Lines removed.
    pub deletions: usize,
}

pub(crate) fn line_of(text: &str, byte: usize) -> usize {
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

/// Which version of a path's bytes to read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rev {
    /// The file as it currently sits in the working directory.
    Workdir,
    /// The blob recorded in HEAD's tree.
    Head,
}
