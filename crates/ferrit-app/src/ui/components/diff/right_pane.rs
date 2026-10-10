//! The right column: `right_pane`.

use crate::ui::components::diff::queries::RightKey;
use crate::ui::components::diff::views::DiffView;
use crate::ui::draw::RenderedDiff;
use crate::ui::row_lines;
use ferrit_domain::apply::Granule;
use ferrit_domain::diff::DiffSide;
use ferrit_tui::theme::palette::Palette;
use ratatui::layout::Rect;
use ratatui::text::Text;
use ratatui_image::picker::Picker;
use std::ops::Range;

pub(crate) struct RightPane {
    /// Terminal graphics backend for the image preview. Starts on half-blocks
    /// (works everywhere); `detect_graphics()` upgrades it to sixel / kitty /
    /// iterm2 when the real terminal supports one.
    pub(crate) picker: Picker,
    /// Diff for the current selection, behind any image preview. Rebuilt on
    /// nav and on background `Refresh`.
    pub(crate) diff: DiffView,
    /// What `diff` currently describes. `None` when no diff applies.
    pub(crate) key: Option<RightKey>,
    /// First visible line of the diff. Kept across a `Refresh` of an unchanged
    /// selection; reset to 0 when the selection changes.
    pub(crate) scroll: usize,
    /// Inner height of the diff box, reported by each frame (`Landed`). Drives the viewport-aware scroll clamp and the page steps. 0
    /// before the first draw: the clamp is then permissive by one screen and
    /// the next frame corrects it.
    pub(crate) viewport: usize,
    /// Whole right-pane rect from the last frame (`Landed`), for routing the mouse wheel
    /// to the diff (over the right column) or the selection (over the left).
    pub(crate) area: Rect,
    /// The line cursor, meaningful only in `Mode::Diff`.
    pub(crate) cursor: DiffCursor,
}

impl RightPane {
    pub(crate) fn new() -> Self {
        Self {
            picker: Picker::halfblocks(),
            diff: DiffView::None,
            key: None,
            scroll: 0,
            viewport: 0,
            area: Rect::ZERO,
            cursor: DiffCursor::default(),
        }
    }
}

impl RightPane {
    /// Is the right pane scrollable right now: a real diff, or a branch's log
    /// preview? The scroll keys and the wheel are inert over an image, a
    /// `Note`, and the mock bodies; without this, they leak through to the
    /// left pane's own selection instead (moving the wrong thing).
    pub(crate) const fn is_diff(&self) -> bool {
        matches!(
            self.diff,
            DiffView::Files(_)
                | DiffView::Commit(..)
                | DiffView::Stash(..)
                | DiffView::BranchLog(_)
        )
    }

    /// Line count of the current diff text, 0 for `None` / `Note`.
    fn diff_line_count(&self) -> usize {
        match &self.diff {
            // Both columns share one scroll; the taller sets how far it goes.
            DiffView::Files(f) => f
                .unstaged
                .text
                .lines()
                .count()
                .max(f.staged.text.lines().count()),
            DiffView::Commit(_, d) | DiffView::Stash(_, d) => d.text.lines().count(),
            DiffView::BranchLog(log) => log
                .commits
                .iter()
                .map(row_lines::rows::branch_log_block_lines)
                .sum(),
            DiffView::None | DiffView::Note(_) => 0,
        }
    }

    /// Largest first-visible line that still fills the viewport: the last diff
    /// line lands at the bottom of the pane, never above it. Falls back to
    /// "line count minus one screen" until the first draw sets a real height.
    fn max_scroll(&self) -> usize {
        self.diff_line_count().saturating_sub(self.viewport.max(1))
    }

    /// Clamp the scroll into `0..=max_scroll()`.
    pub(crate) fn clamp_scroll(&mut self) {
        self.scroll = self.scroll.min(self.max_scroll());
    }

    /// Move the viewport by `delta` lines, clamped so it stops with the last
    /// line at the bottom of the pane. `isize::MIN` / `isize::MAX` snap to the
    /// top / bottom.
    pub(crate) fn scroll_by(&mut self, delta: isize) {
        let mag = delta.unsigned_abs();
        self.scroll = if delta >= 0 {
            self.scroll.saturating_add(mag).min(self.max_scroll())
        } else {
            self.scroll.saturating_sub(mag)
        };
    }

    pub(crate) fn set_scroll(&mut self, line: usize) {
        self.scroll = line;
        self.clamp_scroll();
    }

    pub(crate) fn set_viewport(&mut self, rows: usize) {
        self.viewport = rows;
        self.clamp_scroll();
    }
}

impl RightPane {
    /// Jump `right_scroll` to the next (`dir > 0`) or previous hunk / file
    /// header, lazygit's `]` / `[`. `diff --git` headers for a commit diff;
    /// a no-op on the Files split, which has two diffs and no single anchor
    /// list to jump through.
    pub(crate) fn jump_anchor(&mut self, dir: isize) {
        let anchors = match &self.diff {
            DiffView::Commit(_, d) | DiffView::Stash(_, d) => d.file_lines(),
            DiffView::None | DiffView::Note(_) | DiffView::BranchLog(_) | DiffView::Files(_) => {
                return;
            },
        };
        let cur = self.scroll;
        let target = if dir > 0 {
            anchors.iter().find(|&&l| l > cur).copied()
        } else {
            anchors.iter().rev().find(|&&l| l < cur).copied()
        };
        if let Some(line) = target {
            self.scroll = line;
            self.clamp_scroll();
        }
    }

    /// Scroll the shared Files-split viewport so `cursor.line` stays on
    /// screen, the same "a jump always lands visibly" rule phase 3's `]` /
    /// `[` already follows.
    pub(crate) fn ensure_cursor_visible(&mut self) {
        let viewport = self.viewport.max(1);
        if self.cursor.line < self.scroll {
            self.scroll = self.cursor.line;
        } else if self.cursor.line >= self.scroll + viewport {
            self.scroll = self.cursor.line + 1 - viewport;
        }
        self.clamp_scroll();
    }

    /// Called after *every* key while `Mode::Diff` is up (`update_diff` runs
    /// on every keystroke, not just after a stage), so a plain `j`/`k` must
    /// come through untouched: only re-find the cursor when the line it
    /// names actually stopped being valid — the hunk it was on shrank out
    /// from under it (a line-level stage) or moved off this side entirely
    /// (a whole-hunk stage), `docs/PLAN_6_STAGING.md` "After the apply:
    /// refresh, keep your place". A still-selectable line, hunk unchanged,
    /// is left exactly where it was.
    ///
    /// Gone -> clamp to the nearest remaining hunk on the *same* side, or
    /// drop to `Mode::Nav` once that side has no more changes to show at
    /// all — even if the other side now does; switching sides on the user's
    /// behalf would silently change what the next `<space>` does.
    pub(crate) fn resync_cursor(&mut self) -> bool {
        let DiffView::Files(files) = &self.diff else {
            return false;
        };
        let diff = match self.cursor.side {
            DiffSide::Worktree => &files.unstaged,
            DiffSide::Staged => &files.staged,
        };
        let hunks = hunk_lines_for(diff);

        if let Some(hl) = hunks
            .iter()
            .find(|hl| hunk_content_id(diff, hl.hunk_index) == self.cursor.hunk_id)
        {
            if hl.selectable.contains(&self.cursor.line) {
                self.cursor.anchor = self.cursor.anchor.filter(|a| hl.lines.contains(a));
                self.ensure_cursor_visible();
                return true;
            }
            if let Some(&line) = hl.selectable.first() {
                self.cursor.line = line;
                self.cursor.anchor = None;
                self.ensure_cursor_visible();
                return true;
            }
        }

        if let Some(hl) = hunks.iter().find(|hl| !hl.selectable.is_empty())
            && let Some(&line) = hl.selectable.first()
        {
            self.cursor.line = line;
            self.cursor.anchor = None;
            self.cursor.hunk_id = hunk_content_id(diff, hl.hunk_index);
            self.ensure_cursor_visible();
            true
        } else {
            false
        }
    }

    /// Return cached styled diff. Cache invalidates on selection, diff text,
    /// focus range, or pane width; pure scrolling reuses `Text`. Only a
    /// commit diff goes through this cache: it is keyed for one `Diff` at a
    /// time, and the Files split renders its two sides directly instead
    /// (`ui::draw_files_columns`).
    pub(crate) fn rendered_diff(
        &self,
        palette: &Palette,
        cache: &mut Option<RenderedDiff>,
        focus: Option<&Range<usize>>,
        width: usize,
    ) -> Option<(Text<'static>, usize, ferrit_domain::diff::DiffStat)> {
        let key = &self.key;
        match &self.diff {
            DiffView::Commit(_, diff) | DiffView::Stash(_, diff) => {
                let cache_hit = cache.as_ref().is_some_and(|cached| {
                    cached.key.as_ref() == key.as_ref()
                        && cached.source == diff.text
                        && cached.focus.as_ref() == focus
                        && cached.width == width
                });
                if !cache_hit {
                    let text = diff.delta_output(width).map_or_else(
                        || row_lines::diff::render_diff(palette, diff, focus, width),
                        |formatted| row_lines::diff::render_delta(&formatted, width),
                    );
                    *cache = Some(RenderedDiff {
                        key: key.clone(),
                        source: diff.text.clone(),
                        focus: focus.cloned(),
                        width,
                        text,
                    });
                }
                cache
                    .as_ref()
                    .map(|cached| (cached.text.clone(), cached.text.lines.len(), diff.stat()))
            },
            DiffView::None | DiffView::Note(_) | DiffView::BranchLog(_) | DiffView::Files(_) => {
                None
            },
        }
    }
}

/// The line cursor of the Files split (`Mode::Diff`): where it is, what it can
/// move over, and which hunk or lines a stage would act on.
impl RightPane {
    /// The `Diff` the cursor currently lives in (the active side), or `None`
    /// without a real Files split.
    pub(crate) fn cursor_diff(&self) -> Option<&ferrit_domain::diff::Diff> {
        let DiffView::Files(files) = &self.diff else {
            return None;
        };
        Some(match self.cursor.side {
            DiffSide::Worktree => &files.unstaged,
            DiffSide::Staged => &files.staged,
        })
    }

    /// Put the cursor on the first selectable line of a file's diff, starting
    /// on the worktree side when it has a change there (worktree changes lead,
    /// so a half-staged file's cursor starts where there is still something to
    /// stage). `false`, and nothing moved, when neither side has a selectable
    /// line (binary, a pure rename, no change at all).
    pub(crate) fn start_cursor(&mut self, has_worktree_change: bool) -> bool {
        let DiffView::Files(files) = &self.diff else {
            return false;
        };
        let side = if has_worktree_change {
            DiffSide::Worktree
        } else {
            DiffSide::Staged
        };
        let diff = match side {
            DiffSide::Worktree => &files.unstaged,
            DiffSide::Staged => &files.staged,
        };
        let hunks = hunk_lines_for(diff);
        let Some(hunk) = hunks.iter().find(|hl| !hl.selectable.is_empty()) else {
            return false;
        };
        let Some(&line) = hunk.selectable.first() else {
            return false;
        };
        self.cursor = DiffCursor {
            side,
            line,
            anchor: None,
            hunk_id: hunk_content_id(diff, hunk.hunk_index),
        };
        self.ensure_cursor_visible();
        true
    }

    /// Move the cursor one selectable line down (`dir > 0`) or up: context
    /// lines are unselectable (`docs/PLAN_6_STAGING.md`).
    pub(crate) fn move_cursor(&mut self, dir: isize) {
        let Some(diff) = self.cursor_diff() else {
            return;
        };
        let lines = selectable_lines(diff);
        let Some(pos) = lines.iter().position(|&l| l == self.cursor.line) else {
            return;
        };
        let next = if dir > 0 {
            pos.saturating_add(1).min(lines.len().saturating_sub(1))
        } else {
            pos.saturating_sub(1)
        };
        let Some(&line) = lines.get(next) else {
            return;
        };
        self.place_cursor(line);
    }

    /// Move the cursor to the next (`dir > 0`) or previous hunk's first
    /// selectable line. Unlike `jump_anchor`, which scrolls a single commit
    /// diff, this moves the cursor itself.
    pub(crate) fn jump_cursor_hunk(&mut self, dir: isize) {
        let Some(diff) = self.cursor_diff() else {
            return;
        };
        let starts: Vec<usize> = hunk_lines_for(diff)
            .iter()
            .filter_map(|hl| hl.selectable.first().copied())
            .collect();
        let cur = self.cursor.line;
        let target = if dir > 0 {
            starts.iter().find(|&&l| l > cur).copied()
        } else {
            starts.iter().rev().find(|&&l| l < cur).copied()
        };
        if let Some(line) = target {
            self.place_cursor(line);
        }
    }

    /// Crossing into a different hunk re-tags `hunk_id` right away, or the very
    /// next keystroke's `resync_cursor` (which runs on every key, not just a
    /// stage) reads the stale id, decides the old hunk "lost" this line, and
    /// snaps the cursor straight back to it.
    fn place_cursor(&mut self, line: usize) {
        let hunk_id = self.cursor_diff().and_then(|diff| hunk_id_at(diff, line));
        self.cursor.line = line;
        if let Some(id) = hunk_id {
            self.cursor.hunk_id = id;
        }
        self.ensure_cursor_visible();
    }

    /// Start or clear a line V-selection.
    pub(crate) fn toggle_anchor(&mut self) {
        self.cursor.anchor = if self.cursor.anchor.is_some() {
            None
        } else {
            Some(self.cursor.line)
        };
    }

    /// The cursor for the render: `(side, cursor line, V-select range)`. The
    /// range is inclusive-exclusive (`a..b`) over `side`'s own `Diff::text`.
    pub(crate) fn cursor_view(&self) -> (DiffSide, usize, Option<Range<usize>>) {
        let range = self
            .cursor
            .anchor
            .map(|a| a.min(self.cursor.line)..a.max(self.cursor.line) + 1);
        (self.cursor.side, self.cursor.line, range)
    }

    /// Title suffix: `hunk 1/3`, `lines 41-42` or `line 41`, so it is obvious
    /// what a stage will hit.
    pub(crate) fn granule_hint(&self) -> Option<String> {
        let diff = self.cursor_diff()?;
        let hunks = hunk_lines_for(diff);
        let total = hunks.len();
        let current = hunks
            .iter()
            .position(|hl| hl.lines.contains(&self.cursor.line))?;
        Some(match self.cursor.anchor {
            Some(anchor) => {
                let lo = anchor.min(self.cursor.line) + 1;
                let hi = anchor.max(self.cursor.line) + 1;
                if lo == hi {
                    format!("line {lo}")
                } else {
                    format!("lines {lo}-{hi}")
                }
            },
            None => format!("hunk {}/{total}", current + 1),
        })
    }

    /// What a stage or a discard would act on right now: the V-selected lines
    /// when there is a selection, else the whole hunk under the cursor
    /// (`docs/PLAN_6_STAGING.md` "Granule resolution"). `None` when the
    /// cursor's hunk cannot be found, or a V-selection covers no `+`/`-` line
    /// (only context was under it: a no-op, not an empty patch).
    pub(crate) fn current_granule(&self) -> Option<Granule> {
        let diff = self.cursor_diff()?;
        let file = diff.files.first()?;
        let hunks = hunk_lines_for(diff);
        let hl = hunks
            .iter()
            .find(|hl| hl.lines.contains(&self.cursor.line))?;
        let hunk = file.hunks.get(hl.hunk_index)?;

        if let Some(anchor) = self.cursor.anchor {
            let lo = anchor.min(self.cursor.line);
            let hi = anchor.max(self.cursor.line);
            let lines: Vec<usize> = hl
                .selectable
                .iter()
                .filter(|&&l| (lo..=hi).contains(&l))
                .map(|&l| l - hl.lines.start)
                .collect();
            if lines.is_empty() {
                return None;
            }
            Some(Granule::Lines {
                file_header: diff.text.get(file.header.clone())?.to_owned(),
                hunk_header: diff.text.get(hunk.header.clone())?.to_owned(),
                hunk_body: diff.text.get(hunk.body.clone())?.to_owned(),
                lines,
            })
        } else {
            let patch = diff.text.get(file.header.start..hunk.body.end)?.to_owned();
            Some(Granule::Hunk { patch })
        }
    }
}

/// Where keystrokes go while a Files diff is up. `Nav` is phase 1..5
/// behaviour unchanged; `Diff` is `docs/PLAN_6_STAGING.md`'s "focus the diff
/// to stage within it", scoped to the Files pane — the only one with
/// anything to stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Mode {
    #[default]
    Nav,
    Diff,
}

/// The right-pane diff cursor, meaningful only in `Mode::Diff`. `line` and
/// `anchor` are indices into `side`'s own `Diff::text` lines: the Files
/// split always shows at most one file per side, so there is no "flatten
/// every file's hunks" step, just the diff's own line numbering.
#[derive(Debug, Clone, Default)]
pub(crate) struct DiffCursor {
    pub(crate) side: DiffSide,
    pub(crate) line: usize,
    /// V-select anchor. `None` is a single line, `Some(a)` is the range
    /// `a..=line` (order-independent: whichever end moves).
    pub(crate) anchor: Option<usize>,
    /// Content hash of the hunk the cursor sits in (header + body text), so
    /// a background refresh can re-find the same hunk even if surrounding
    /// hunks changed line count. gitu hashes the same way for its `Item.id`.
    pub(crate) hunk_id: u64,
}

/// One hunk's body as global (whole-`Diff::text`) line indices, plus which
/// of those lines are selectable (`+`/`-`; context is read but never
/// chosen). Built fresh per diff-mode operation from the current `Diff` —
/// cheap at working-tree sizes, the same "no cache" choice `files_tree_rows`
/// already makes.
pub(crate) struct HunkLines {
    pub(crate) hunk_index: usize,
    pub(crate) lines: Range<usize>,
    pub(crate) selectable: Vec<usize>,
}

/// Body-line ranges (global `diff.text` line indices) for every hunk of a
/// single-file `Diff`, plus which of those lines are selectable.
pub(crate) fn hunk_lines_for(diff: &ferrit_domain::diff::Diff) -> Vec<HunkLines> {
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
pub(crate) fn selectable_lines(diff: &ferrit_domain::diff::Diff) -> Vec<usize> {
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
pub(crate) fn hunk_id_at(diff: &ferrit_domain::diff::Diff, line: usize) -> Option<u64> {
    let hl = hunk_lines_for(diff)
        .into_iter()
        .find(|hl| hl.lines.contains(&line))?;
    Some(hunk_content_id(diff, hl.hunk_index))
}

/// Stable id for hunk `hunk_index` of `diff`: a hash of its header + body
/// text, so a background refresh can re-find the same hunk even once
/// staging moved a *different* hunk out from under it (gitu's `Item.id`).
pub(crate) fn hunk_content_id(diff: &ferrit_domain::diff::Diff, hunk_index: usize) -> u64 {
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
