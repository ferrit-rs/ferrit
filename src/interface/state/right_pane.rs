//! The right column: what it shows for the selection (an image, a diff), where
//! it is scrolled, and the line cursor used to stage hunks and lines. Building
//! the diff and handling its keys stay with the `App` methods in `diff_query`,
//! `image_query` and `staging`; this is the state they read and write.

use std::ops::Range;

use ratatui::layout::Rect;
use ratatui::text::Text;
use ratatui_image::picker::Picker;

use crate::git;
use crate::git::apply::Granule;
use crate::git::diff::DiffSide;
use crate::interface::screens::row_lines;
use crate::interface::state::diff_cursor::{
    DiffCursor, hunk_content_id, hunk_id_at, hunk_lines_for, selectable_lines,
};
use crate::interface::state::diff_query::RightKey;
use crate::interface::state::render_state::RenderedDiff;
use crate::interface::state::views::DiffView;
use crate::theme::palette::Palette;

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

/// The line cursor of the Files split (`Mode::Diff`): where it is, what it can
/// move over, and which hunk or lines a stage would act on.
impl RightPane {
    /// The `Diff` the cursor currently lives in (the active side), or `None`
    /// without a real Files split.
    pub(crate) fn cursor_diff(&self) -> Option<&git::diff::Diff> {
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
