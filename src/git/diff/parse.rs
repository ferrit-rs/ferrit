//! Plain-text (`--color=never`) `git diff` / `git show` scanner.
//!
//! Byte offsets only: every field is a `Range<usize>` into the `text` that was
//! parsed, nothing is copied out. Modelled on gitu's `gitu_diff` parser
//! (`../ferrit-references/tui/gitu/src/gitu_diff.rs`), trimmed to what a
//! read-only view needs. Unrecognised lines are simply not indexed.

use std::ops::Range;

/// How a file changed, from the `diff --git` preamble lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileStatus {
    Added,
    Deleted,
    Modified,
    Renamed,
    Copied,
}

/// One hunk: the `@@ ... @@` header line and the body lines under it, plus the
/// four numbers parsed out of the header (a missing `,count` means 1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HunkMeta {
    /// The `@@ -a,b +c,d @@ ctx` line, without its trailing newline.
    pub header: Range<usize>,
    /// Lines after the header, up to the next hunk or the next file. Used
    /// verbatim as the patch body in phase 4.
    pub body: Range<usize>,
    pub old_start: u32,
    pub old_count: u32,
    pub new_start: u32,
    pub new_count: u32,
}

/// One file section of a diff: `diff --git` line to the line before the next
/// `diff --git` (or end of text).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileMeta {
    /// `diff --git` line through the last preamble line before the first hunk
    /// (`index`, `---`, `+++`, mode / rename lines). `header.start
    /// ..hunk.body.end` is the whole slice a phase-4 hunk patch needs.
    pub header: Range<usize>,
    pub old_path: Range<usize>,
    pub new_path: Range<usize>,
    pub status: FileStatus,
    pub binary: bool,
    pub hunks: Vec<HunkMeta>,
}

/// Split `text` into one `FileMeta` per `diff --git` section.
pub fn parse(text: &str) -> Vec<FileMeta> {
    let starts = section_starts(text);
    starts
        .iter()
        .enumerate()
        .map(|(i, &start)| {
            let end = starts.get(i + 1).copied().unwrap_or(text.len());
            parse_section(text, start..end)
        })
        .collect()
}

/// Byte offset of every line beginning with `diff --git `.
fn section_starts(text: &str) -> Vec<usize> {
    let mut out = Vec::new();
    let mut at_line_start = true;
    for (i, ch) in text.char_indices() {
        if at_line_start && text[i..].starts_with("diff --git ") {
            out.push(i);
        }
        at_line_start = ch == '\n';
    }
    out
}

fn parse_section(text: &str, range: Range<usize>) -> FileMeta {
    let lines: Vec<(usize, &str)> = line_offsets(&text[range.clone()])
        .map(|(o, l)| (range.start + o, l))
        .collect();

    let mut status = FileStatus::Modified;
    let mut binary = false;
    let mut old_path = 0..0;
    let mut new_path = 0..0;
    let mut first_hunk: Option<usize> = None;

    for &(off, line) in &lines {
        if line.starts_with("@@ ") {
            first_hunk = Some(off);
            break;
        }
        if line.starts_with("new file mode") {
            status = FileStatus::Added;
        } else if line.starts_with("deleted file mode") {
            status = FileStatus::Deleted;
        } else if line.starts_with("rename from ") || line.starts_with("rename to ") {
            status = FileStatus::Renamed;
        } else if line.starts_with("copy from ") || line.starts_with("copy to ") {
            status = FileStatus::Copied;
        } else if line.starts_with("Binary files ") {
            binary = true;
        }

        if let Some(rest) = line.strip_prefix("--- ") {
            old_path = path_range(off + 4, rest);
        } else if let Some(rest) = line.strip_prefix("+++ ") {
            new_path = path_range(off + 4, rest);
        }
    }

    // No ---/+++ pair (pure rename, mode change, binary add): take the paths
    // from the `diff --git a/X b/X` line instead.
    if old_path.is_empty()
        && new_path.is_empty()
        && let Some(&(off, line)) = lines.first()
        && let Some(rest) = line.strip_prefix("diff --git ")
    {
        (old_path, new_path) = git_line_paths(off + 11, rest);
    }

    let header = range.start..first_hunk.unwrap_or(range.end);

    let mut hunks = Vec::new();
    if let Some(hstart) = first_hunk {
        let hlines: Vec<(usize, &str)> =
            lines.iter().copied().filter(|&(o, _)| o >= hstart).collect();
        let mut i = 0;
        while i < hlines.len() {
            let (hoff, hline) = hlines[i];
            let mut j = i + 1;
            while j < hlines.len() && !hlines[j].1.starts_with("@@ ") {
                j += 1;
            }
            let body_start = (hoff + hline.len() + 1).min(range.end);
            let body_end = hlines.get(j).map_or(range.end, |&(o, _)| o);
            let (os, oc, ns, nc) = parse_hunk_header(hline);
            hunks.push(HunkMeta {
                header: hoff..hoff + hline.len(),
                body: body_start..body_end.max(body_start),
                old_start: os,
                old_count: oc,
                new_start: ns,
                new_count: nc,
            });
            i = j;
        }
    }

    FileMeta {
        header,
        old_path,
        new_path,
        status,
        binary,
        hunks,
    }
}

/// `(absolute offset, line without trailing \r?\n)` for each line of `s`.
fn line_offsets(s: &str) -> impl Iterator<Item = (usize, &str)> {
    let mut off = 0;
    s.split_inclusive('\n').map(move |chunk| {
        let start = off;
        off += chunk.len();
        let line = chunk.strip_suffix('\n').unwrap_or(chunk);
        (start, line.strip_suffix('\r').unwrap_or(line))
    })
}

/// Range of the path in a `--- `/`+++ ` line, dropping the `a/` or `b/` prefix
/// and any trailing tab-separated timestamp. `base` is where `rest` starts.
fn path_range(base: usize, rest: &str) -> Range<usize> {
    let piece = &rest[..rest.find('\t').unwrap_or(rest.len())];
    let skip = if piece.starts_with("a/") || piece.starts_with("b/") {
        2
    } else {
        0
    };
    base + skip..base + piece.len()
}

/// Ranges of `OLD` and `NEW` in a `diff --git a/OLD b/NEW` tail. `base` is
/// where the tail starts. Falls back to the whole tail if the ` b/` split is
/// not found (unusual quoting).
fn git_line_paths(base: usize, rest: &str) -> (Range<usize>, Range<usize>) {
    match rest.find(" b/") {
        Some(bpos) => {
            let old_skip = if rest.starts_with("a/") { 2 } else { 0 };
            (
                base + old_skip..base + bpos,
                base + bpos + 3..base + rest.len(),
            )
        }
        None => {
            let all = base..base + rest.len();
            (all.clone(), all)
        }
    }
}

/// `@@ -a,b +c,d @@ ctx` -> `(a, b, c, d)`. `,b` / `,d` default to 1. lazygit's
/// regex is `^@@ -(\d+)[^\+]+\+(\d+)[^@]+@@`; this is the hand-rolled version.
fn parse_hunk_header(line: &str) -> (u32, u32, u32, u32) {
    let after_minus = line.find('-').map_or("", |i| &line[i + 1..]);
    let (old_start, rest) = take_u32(after_minus);
    let (old_count, rest) = match rest.strip_prefix(',') {
        Some(r) => take_u32(r),
        None => (1, rest),
    };
    let after_plus = rest.find('+').map_or("", |i| &rest[i + 1..]);
    let (new_start, rest) = take_u32(after_plus);
    let (new_count, _) = match rest.strip_prefix(',') {
        Some(r) => take_u32(r),
        None => (1, rest),
    };
    (old_start, old_count, new_start, new_count)
}

fn take_u32(s: &str) -> (u32, &str) {
    let end = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    (s[..end].parse().unwrap_or(0), &s[end..])
}
