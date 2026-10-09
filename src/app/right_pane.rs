//! The right column: what it shows for the selection (an image, a diff), where
//! it is scrolled, and the line cursor used to stage hunks and lines. Building
//! the diff and handling its keys stay with the `App` methods in `diff_query`,
//! `image_query` and `staging`; this is the state they read and write.

use ratatui::layout::Rect;
use ratatui_image::picker::Picker;

use super::diff_query::RightKey;
use super::theme;
use super::{DiffCursor, DiffView};

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
            DiffView::BranchLog(log) => log.commits.iter().map(theme::branch_log_block_lines).sum(),
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
