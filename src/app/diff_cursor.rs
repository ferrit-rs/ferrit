//! The line cursor of the diff, its keyboard mode and the hunks it moves through.

use crate::git;
use crate::git::diff::DiffSide;
use std::ops::Range;

/// Where keystrokes go while a Files diff is up. `Nav` is phase 1..5
/// behaviour unchanged; `Diff` is `docs/PLAN_6_STAGING.md`'s "focus the diff
/// to stage within it", scoped to the Files pane — the only one with
/// anything to stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) enum Mode {
    #[default]
    Nav,
    Diff,
}

/// The right-pane diff cursor, meaningful only in `Mode::Diff`. `line` and
/// `anchor` are indices into `side`'s own `Diff::text` lines: the Files
/// split always shows at most one file per side, so there is no "flatten
/// every file's hunks" step, just the diff's own line numbering.
#[derive(Debug, Clone, Default)]
pub(super) struct DiffCursor {
    pub(super) side: DiffSide,
    pub(super) line: usize,
    /// V-select anchor. `None` is a single line, `Some(a)` is the range
    /// `a..=line` (order-independent: whichever end moves).
    pub(super) anchor: Option<usize>,
    /// Content hash of the hunk the cursor sits in (header + body text), so
    /// a background refresh can re-find the same hunk even if surrounding
    /// hunks changed line count. gitu hashes the same way for its `Item.id`.
    pub(super) hunk_id: u64,
}

/// What `<space>` / `d` act on in `Mode::Diff`: the whole hunk under the
/// cursor, or a V-selected subset of its `+`/`-` lines.
pub(super) enum Granule {
    Hunk {
        patch: String,
    },
    Lines {
        file_header: String,
        hunk_header: String,
        hunk_body: String,
        lines: Vec<usize>,
    },
}

/// One hunk's body as global (whole-`Diff::text`) line indices, plus which
/// of those lines are selectable (`+`/`-`; context is read but never
/// chosen). Built fresh per diff-mode operation from the current `Diff` —
/// cheap at working-tree sizes, the same "no cache" choice `files_tree_rows`
/// already makes.
pub(super) struct HunkLines {
    pub(super) hunk_index: usize,
    pub(super) lines: Range<usize>,
    pub(super) selectable: Vec<usize>,
}

/// Body-line ranges (global `diff.text` line indices) for every hunk of a
/// single-file `Diff`, plus which of those lines are selectable.
pub(super) fn hunk_lines_for(diff: &git::diff::Diff) -> Vec<HunkLines> {
    let Some(file) = diff.files.first() else {
        return Vec::new();
    };
    let lines: Vec<&str> = diff.text.lines().collect();
    let headers = diff.hunk_lines();
    file.hunks
        .iter()
        .enumerate()
        .map(|(hunk_index, hunk)| {
            let body_start = headers.get(hunk_index).map_or(0, |&l| l + 1);
            let body_len = diff
                .text
                .get(hunk.body.clone())
                .unwrap_or_default()
                .lines()
                .count();
            let range = body_start..body_start + body_len;
            let selectable = range
                .clone()
                .filter(|&l| {
                    matches!(
                        lines.get(l).and_then(|s| s.as_bytes().first()),
                        Some(b'+' | b'-')
                    )
                })
                .collect();
            HunkLines {
                hunk_index,
                lines: range,
                selectable,
            }
        })
        .collect()
}

/// Every selectable line across every hunk of `diff`, in order. `j` / `k` in
/// `Mode::Diff` step through this list, skipping context lines entirely.
pub(super) fn selectable_lines(diff: &git::diff::Diff) -> Vec<usize> {
    hunk_lines_for(diff)
        .into_iter()
        .flat_map(|hl| hl.selectable)
        .collect()
}

/// The content id of whichever hunk contains global line `line`, or `None`
/// if it falls outside every hunk (should not happen for a selectable
/// line). Used to keep `DiffCursor::hunk_id` pointing at the hunk the
/// cursor is actually on whenever it moves, so `resync_diff_cursor` (which
/// runs after *every* key, not just a stage) does not mistake "moved to a
/// different hunk" for "the old hunk vanished" and snap back to it.
pub(super) fn hunk_id_at(diff: &git::diff::Diff, line: usize) -> Option<u64> {
    let hl = hunk_lines_for(diff)
        .into_iter()
        .find(|hl| hl.lines.contains(&line))?;
    Some(hunk_content_id(diff, hl.hunk_index))
}

/// Stable id for hunk `hunk_index` of `diff`: a hash of its header + body
/// text, so a background refresh can re-find the same hunk even once
/// staging moved a *different* hunk out from under it (gitu's `Item.id`).
pub(super) fn hunk_content_id(diff: &git::diff::Diff, hunk_index: usize) -> u64 {
    use std::hash::{Hash as _, Hasher as _};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    if let Some(hunk) = diff.files.first().and_then(|f| f.hunks.get(hunk_index)) {
        diff.text
            .get(hunk.header.start..hunk.body.end)
            .unwrap_or_default()
            .hash(&mut hasher);
    }
    hasher.finish()
}
