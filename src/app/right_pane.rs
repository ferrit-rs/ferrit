//! The right column: what it shows for the selection (an image, a diff), where
//! it is scrolled, and the line cursor used to stage hunks and lines. Building
//! the diff and handling its keys stay with the `App` methods in `diff_query`,
//! `image_query` and `staging`; this is the state they read and write.

use std::ops::Range;

use ratatui::layout::Rect;
use ratatui::text::Text;
use ratatui_image::picker::Picker;

use super::diff_query::RightKey;
use super::render_state::RenderedDiff;
use super::{DiffCursor, DiffView, hunk_content_id, hunk_lines_for, row_lines};
use crate::domain::git;
use crate::domain::git::diff::DiffSide;
use crate::theme::palette::Palette;

pub(crate) struct RightPane {
    /// Terminal graphics backend for the image preview. Starts on half-blocks
    /// (works everywhere); `detect_graphics()` upgrades it to sixel / kitty /
    /// iterm2 when the real terminal supports one.
    pub(super) picker: Picker,
    /// Diff for the current selection, behind any image preview. Rebuilt on
    /// nav and on background `Refresh`.
    pub(super) diff: DiffView,
    /// What `diff` currently describes. `None` when no diff applies.
    pub(super) key: Option<RightKey>,
    /// First visible line of the diff. Kept across a `Refresh` of an unchanged
    /// selection; reset to 0 when the selection changes.
    pub(super) scroll: usize,
    /// Inner height of the diff box, reported by each frame (`Landed`). Drives the viewport-aware scroll clamp and the page steps. 0
    /// before the first draw: the clamp is then permissive by one screen and
    /// the next frame corrects it.
    pub(super) viewport: usize,
    /// Whole right-pane rect from the last frame (`Landed`), for routing the mouse wheel
    /// to the diff (over the right column) or the selection (over the left).
    pub(super) area: Rect,
    /// The line cursor, meaningful only in `Mode::Diff`.
    pub(super) cursor: DiffCursor,
}

impl RightPane {
    pub(super) fn new() -> Self {
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
    pub(super) const fn is_diff(&self) -> bool {
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
                .map(row_lines::branch_log_block_lines)
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
    pub(super) fn clamp_scroll(&mut self) {
        self.scroll = self.scroll.min(self.max_scroll());
    }

    /// Move the viewport by `delta` lines, clamped so it stops with the last
    /// line at the bottom of the pane. `isize::MIN` / `isize::MAX` snap to the
    /// top / bottom.
    pub(super) fn scroll_by(&mut self, delta: isize) {
        let mag = delta.unsigned_abs();
        self.scroll = if delta >= 0 {
            self.scroll.saturating_add(mag).min(self.max_scroll())
        } else {
            self.scroll.saturating_sub(mag)
        };
    }

    pub(super) fn set_scroll(&mut self, line: usize) {
        self.scroll = line;
        self.clamp_scroll();
    }

    pub(super) fn set_viewport(&mut self, rows: usize) {
        self.viewport = rows;
        self.clamp_scroll();
    }
}

impl RightPane {
    /// Jump `right_scroll` to the next (`dir > 0`) or previous hunk / file
    /// header, lazygit's `]` / `[`. `diff --git` headers for a commit diff;
    /// a no-op on the Files split, which has two diffs and no single anchor
    /// list to jump through.
    pub(super) fn jump_anchor(&mut self, dir: isize) {
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
    pub(super) fn ensure_cursor_visible(&mut self) {
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
    pub(super) fn resync_cursor(&mut self) -> bool {
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
    pub(super) fn rendered_diff(
        &self,
        palette: &Palette,
        cache: &mut Option<RenderedDiff>,
        focus: Option<&Range<usize>>,
        width: usize,
    ) -> Option<(Text<'static>, usize, git::diff::DiffStat)> {
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
                        || row_lines::render_diff(palette, diff, focus, width),
                        |formatted| row_lines::render_delta(&formatted, width),
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
