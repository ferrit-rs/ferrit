//! The right column: the diff, the image, the line cursor, what loads them and how they are drawn.

use crate::git;
use crate::git::apply::Granule;
use crate::git::diff::{DiffOpts, DiffSide};
use crate::git::error::{GitError, GitResult};
use crate::git::image::preview::Preview;
use crate::git::port::GitPort;
use crate::theme::palette::Palette;
use crate::tui::components::create_remote::CreateRemoteView;
use crate::tui::components::panes::Pane;
use crate::tui::components::welcome::welcome_lines;
use crate::tui::draw::{Landed, RenderState, RenderedDiff};
use crate::tui::error::AppError;
use crate::tui::widgets::panel::Panel;
use crate::tui::widgets::scroll_bar::ScrollBar;
use crate::tui::widgets::text_input::TextInput;
use crate::tui::widgets::tui_overlay::state::OverlayState;
use crate::tui::{App, mock, row_lines};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Clear, Paragraph, Wrap};
use ratatui_image::picker::Picker;
use ratatui_image::{Resize, StatefulImage};
use std::ops::Range;
use std::path::{Path, PathBuf};

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
pub(crate) fn hunk_lines_for(diff: &git::diff::Diff) -> Vec<HunkLines> {
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
pub(crate) fn selectable_lines(diff: &git::diff::Diff) -> Vec<usize> {
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
pub(crate) fn hunk_id_at(diff: &git::diff::Diff, line: usize) -> Option<u64> {
    let hl = hunk_lines_for(diff)
        .into_iter()
        .find(|hl| hl.lines.contains(&line))?;
    Some(hunk_content_id(diff, hl.hunk_index))
}

/// Stable id for hunk `hunk_index` of `diff`: a hash of its header + body
/// text, so a background refresh can re-find the same hunk even once
/// staging moved a *different* hunk out from under it (gitu's `Item.id`).
pub(crate) fn hunk_content_id(diff: &git::diff::Diff, hunk_index: usize) -> u64 {
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

/// What the right pane shows behind the image preview. A second cached,
/// rebuilt-on-nav value alongside `preview`, not a replacement: an image
/// selection still wins. See `docs/PLAN_3_DIFF_VIEW.md`.
#[derive(Debug, Clone, Default)]
pub enum DiffView {
    /// Status / Stash focused: no real diff, the mock text shows.
    #[default]
    None,
    /// Read failed, or nothing to show. A dim single line, never a panic.
    Note(String),
    /// Files pane: one file's `git diff`, both sides at once (lazygit's own
    /// Unstaged Changes / Staged Changes split).
    Files(FilesDiff),
    /// Commits pane, or a drilled branch's log: one commit's metadata and
    /// `git show` diff.
    Commit(git::model::CommitEntry, git::diff::Diff),
    /// Stash pane: the selected entry's `git stash show -p`, scrolled and
    /// rendered like a commit diff (`docs/PLAN_10_STASH.md`).
    Stash(git::model::StashEntry, git::diff::Diff),
    /// Branches pane, not drilled in: the selected branch's own log, shown
    /// passively (no Enter needed), lazygit's live branch -> log preview.
    BranchLog(BranchLog),
}

/// A Files-pane selection's two sides at once, lazygit's own Unstaged
/// Changes / Staged Changes split: a file half-staged shows real content in
/// both, a file entirely on one side shows an empty diff on the other.
#[derive(Debug, Clone)]
pub struct FilesDiff {
    pub unstaged: git::diff::Diff,
    pub staged: git::diff::Diff,
}

/// A branch's own commit log for the passive `DiffView::BranchLog` preview.
#[derive(Debug, Clone)]
pub struct BranchLog {
    pub branch: String,
    pub commits: Vec<git::model::CommitEntry>,
}

/// Read-only view of an editor popup for `ui::draw_commit_popup`
/// (`docs/PLAN_7_COMMIT.md`), reused as-is for the new-branch popup
/// (`docs/PLAN_8_BRANCHES.md`) — same shape, different title/footer.
/// Borrows the draft's lines, so it is cheap to build fresh every frame
/// rather than cached.
pub struct CommitPopupView<'a> {
    pub title: &'a str,
    /// Commit subject, or the whole single-field input for another popup.
    pub input: &'a TextInput,
    /// Commit body editor; absent for the new-branch input.
    pub description: Option<&'a TextInput>,
    pub summary_focused: bool,
    pub overlay_state: Option<&'a mut OverlayState>,
    /// Compatibility view of the component's text lines.
    pub lines: &'a [String],
    /// `(row, character column)` cursor position.
    pub cursor: (usize, usize),
    /// `Some((sign_off, no_verify))` for the commit popup's toggle line;
    /// `None` for the new-branch popup, which has nothing to toggle.
    pub toggles: Option<(bool, bool)>,
    /// The commit popup's author line (`author: Name <email>`); `None` for the
    /// other popups and for a reword, which keeps the commit's own author.
    pub author: Option<String>,
    /// Footer key hints, e.g. `"Commit: Ctrl-S | ... | Cancel: Esc"`.
    pub hints: &'static str,
}

/// The one active popup, ready for rendering. Explicit variants keep popup
/// precedence in one `match` instead of chaining `if let` checks.
pub enum PopupView<'a> {
    Commit(CommitPopupView<'a>),
    /// The "stage everything?" question. Carries the backdrop's animation only when
    /// the view is for drawing (`popup_view_with`).
    CommitAllConfirm(Option<&'a mut OverlayState>),
    NewBranch(CommitPopupView<'a>),
    Stash(CommitPopupView<'a>),
    Name(CommitPopupView<'a>),
    CommandLog(CommandLogView),
    Menu(MenuView),
    Upstream(CommitPopupView<'a>),
    Askpass(CommitPopupView<'a>),
    CreateRemote(CreateRemoteView<'a>),
    Note(&'a str),
}

/// The `@` viewer's render data: every recorded command and how far the view
/// is scrolled up from the newest.
#[derive(Debug)]
pub struct CommandLogView {
    pub records: Vec<git::command_log::CommandRecord>,
    pub from_bottom: usize,
}

/// A menu's render data: the title, one line per row, and the highlighted
/// row.
#[derive(Debug)]
pub struct MenuView {
    pub title: String,
    pub rows: Vec<String>,
    pub selected: usize,
    /// What the highlighted row does; empty when the menu has no hints.
    pub hint: &'static str,
}

impl DiffView {
    /// The first file the stash `oid` touches, when this view shows that stash:
    /// where a restore puts the selection.
    pub(crate) fn first_stash_file(&self, oid: &str) -> Option<PathBuf> {
        match self {
            Self::Stash(entry, diff) if entry.oid == oid => diff
                .files
                .first()
                .and_then(|f| diff.text.get(f.new_path.clone()))
                .map(PathBuf::from),
            _ => None,
        }
    }
}

#[derive(Default)]
pub(crate) struct DiffQueryState {
    pub(crate) in_flight: bool,
    pub(crate) pending: Option<(RightKey, u64)>,
    pub(crate) generation: u64,
    pub(crate) refresh_requested: bool,
}

/// Identity of selected right-pane content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RightKey {
    File { path: PathBuf },
    Commit { full_hash: String },
    BranchLog { branch: String },
    Stash { oid: String, header: String },
}

#[derive(Debug)]
pub(crate) enum DiffQueryResult {
    File {
        unstaged: git::diff::Diff,
        staged: git::diff::Diff,
    },
    Commit(git::diff::Diff),
    BranchLog(Vec<git::model::CommitEntry>),
    Stash(git::diff::Diff),
}

/// Hidden event payload for a selected-diff worker completion.
#[doc(hidden)]
#[derive(Debug)]
pub struct DiffCompletion {
    pub(crate) key: RightKey,
    pub(crate) generation: u64,
    pub(crate) result: Result<DiffQueryResult, AppError>,
}

pub(crate) fn load_diff(
    repo: &dyn GitPort,
    key: &RightKey,
    opts: DiffOpts,
) -> Result<DiffQueryResult, GitError> {
    match key {
        RightKey::File { path } => {
            // The root directory row has an empty path: `git diff -- .` is every file.
            let path = if path.as_os_str().is_empty() {
                Path::new(".")
            } else {
                path.as_path()
            };
            Ok(DiffQueryResult::File {
                unstaged: repo.file_diff(path, DiffSide::Worktree, opts)?,
                staged: repo.file_diff(path, DiffSide::Staged, opts)?,
            })
        },
        RightKey::Commit { full_hash } => repo
            .commit_diff(full_hash, opts)
            .map(DiffQueryResult::Commit),
        RightKey::BranchLog { branch } => repo.branch_log(branch).map(DiffQueryResult::BranchLog),
        RightKey::Stash { oid, header } => repo
            .stash_diff(oid, header, opts)
            .map(DiffQueryResult::Stash),
    }
}

/// Image worker result, applied only if selection and generation still match.
#[doc(hidden)]
#[derive(Debug)]
pub struct ImageCompletion {
    pub(crate) path: PathBuf,
    pub(crate) generation: u64,
    pub(crate) result: Result<::image::DynamicImage, AppError>,
}

/// Why a file preview could not show an image.
#[derive(Debug, thiserror::Error)]
pub enum ImageError {
    #[error(transparent)]
    Open(#[from] GitError),
    #[error("[image] {}  ({source})", .path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: GitError,
    },
    #[error("[image] {}  (no bytes)", .path.display())]
    Empty { path: PathBuf },
    #[error("[image] {}  ({bytes} bytes)  decode failed: {source}", .path.display())]
    Decode {
        path: PathBuf,
        bytes: usize,
        #[source]
        source: ::image::ImageError,
    },
}

pub(crate) fn load_image(
    repo: GitResult<Box<dyn GitPort>>,
    image_path: &Path,
) -> Result<::image::DynamicImage, ImageError> {
    let repo = repo?;
    let bytes = repo
        .blob_bytes(image_path, git::blob::Rev::Workdir)
        .map_err(|source| ImageError::Read {
            path: image_path.to_path_buf(),
            source,
        })?;
    if bytes.is_empty() {
        return Err(ImageError::Empty {
            path: image_path.to_path_buf(),
        });
    }
    ::image::load_from_memory(&bytes).map_err(|source| ImageError::Decode {
        path: image_path.to_path_buf(),
        bytes: bytes.len(),
        source,
    })
}

pub(crate) fn draw_files_columns(
    frame: &mut Frame<'_>,
    app: &App,
    landed: &mut Landed,
    area: Rect,
) {
    landed.right_area = Some(area);
    let palette = app.palette();

    let DiffView::Files(files) = app.diff_view() else {
        return;
    };
    let unstaged = files.unstaged.clone();
    let staged = files.staged.clone();

    let [left, right] =
        Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)]).areas(area);

    let scroll = app.right_scroll();
    let cursor = app.diff_cursor();
    let hint = app.diff_granule_hint();
    let unstaged_cursor = diff_cursor_for(cursor.clone(), DiffSide::Worktree);
    let staged_cursor = diff_cursor_for(cursor, DiffSide::Staged);

    draw_diff_column(
        frame,
        left,
        &diff_column_title(
            " Unstaged Changes ",
            unstaged_cursor.is_some(),
            hint.as_deref(),
        ),
        &unstaged,
        scroll,
        unstaged_cursor,
        &palette,
    );
    let viewport = draw_diff_column(
        frame,
        right,
        &diff_column_title(" Staged Changes ", staged_cursor.is_some(), hint.as_deref()),
        &staged,
        scroll,
        staged_cursor,
        &palette,
    );
    landed.right_viewport = Some(viewport);
}

/// One-sided file changes use a single full-width panel, matching lazygit's
/// default `gui.splitDiff: auto` behavior. Pick staged when no worktree diff.
pub(crate) fn draw_single_file_diff(
    frame: &mut Frame<'_>,
    app: &App,
    landed: &mut Landed,
    area: Rect,
) {
    landed.right_area = Some(area);
    let palette = app.palette();
    let DiffView::Files(files) = app.diff_view() else {
        return;
    };
    let (side, diff, title) = if files.unstaged.text.trim().is_empty() {
        (DiffSide::Staged, files.staged.clone(), " Staged Changes ")
    } else {
        (
            DiffSide::Worktree,
            files.unstaged.clone(),
            " Unstaged Changes ",
        )
    };
    let cursor = diff_cursor_for(app.diff_cursor(), side);
    let scroll = app.right_scroll();
    let block = Panel::new()
        .title(Line::styled(title, Style::new().fg(palette.idle)))
        .border_style(Style::new().fg(palette.idle))
        .block();
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let [stat_row, diff_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(inner);
    frame.render_widget(
        Paragraph::new(row_lines::stat_line(&palette, diff.stat())),
        stat_row,
    );
    let mut text = diff.delta_output(diff_area.width as usize).map_or_else(
        || row_lines::render_diff(&palette, &diff, None, diff_area.width as usize),
        |formatted| row_lines::render_delta(&formatted, diff_area.width as usize),
    );
    overlay_diff_cursor(&mut text, cursor, diff_area.width as usize, &palette);
    let total = text.lines.len();
    frame.render_widget(
        Paragraph::new(text).scroll((u16::try_from(scroll).unwrap_or(u16::MAX), 0)),
        diff_area,
    );
    landed.right_viewport = Some(diff_area.height as usize);
    ScrollBar::new(total, diff_area.height as usize, scroll).render(frame, diff_area);
}

/// `app.diff_cursor()`'s `(line, V-select range)` for `side`, or `None` when
/// the cursor is on the other side (or `Mode::Diff` isn't up at all).
fn diff_cursor_for(
    cursor: Option<(DiffSide, usize, Option<Range<usize>>)>,
    side: DiffSide,
) -> Option<(usize, Option<Range<usize>>)> {
    let (cursor_side, line, selection) = cursor?;
    (cursor_side == side).then_some((line, selection))
}

/// A Files-split column title, with the `Mode::Diff` granule hint appended
/// (`hunk 1/3` / `lines 41-42`) when `active` — the cursor's own column.
fn diff_column_title(base: &str, active: bool, hint: Option<&str>) -> String {
    match (active, hint) {
        (true, Some(hint)) => format!("{} ({hint}) ", base.trim_end()),
        _ => base.to_owned(),
    }
}

/// One column of the Files split (`draw_files_columns`): border, stat line,
/// diff body scrolled to the shared `scroll` line, and a scrollbar when it
/// overflows. Returns the diff body's own height for the caller's shared
/// viewport. An empty side (nothing staged, or nothing left unstaged) just
/// shows a `0 files changed` stat and a blank body — the common half-staged
/// case is the one this exists for, not worth a special case.
///
/// `cursor`, when this column is the `Mode::Diff` cursor's own side, is
/// `(cursor line, V-select range)` — both indices into `diff`'s own lines,
/// applied as a post-render overlay (`overlay_diff_cursor`) so it works the
/// same whether the body came from `row_lines::render_diff` or from delta.
fn draw_diff_column(
    frame: &mut Frame<'_>,
    area: Rect,
    title: &str,
    diff: &git::diff::Diff,
    scroll: usize,
    cursor: Option<(usize, Option<Range<usize>>)>,
    palette: &Palette,
) -> usize {
    let block = Panel::new()
        .title(Line::styled(
            title.to_owned(),
            Style::new().fg(palette.idle),
        ))
        .border_style(Style::new().fg(palette.idle))
        .block();
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let [stat_row, diff_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(inner);
    frame.render_widget(
        Paragraph::new(row_lines::stat_line(palette, diff.stat())),
        stat_row,
    );

    let mut text = diff.delta_output(diff_area.width as usize).map_or_else(
        || row_lines::render_diff(palette, diff, None, diff_area.width as usize),
        |formatted| row_lines::render_delta(&formatted, diff_area.width as usize),
    );
    overlay_diff_cursor(&mut text, cursor, diff_area.width as usize, palette);
    let total = text.lines.len();
    let panel = Paragraph::new(text).scroll((u16::try_from(scroll).unwrap_or(u16::MAX), 0));
    frame.render_widget(panel, diff_area);

    let viewport = diff_area.height as usize;
    ScrollBar::new(total, viewport, scroll).render(frame, diff_area);
    viewport
}

/// Paint the `Mode::Diff` cursor onto an already-rendered diff body: a
/// full-width reversed bar on the cursor line, and the palette's selection colour
/// background across a V-selection. Applied after rendering, not woven into
/// `row_lines::render_diff`, so it works identically over that native path and
/// over delta's ANSI-derived one.
fn overlay_diff_cursor(
    text: &mut Text<'static>,
    cursor: Option<(usize, Option<Range<usize>>)>,
    width: usize,
    palette: &Palette,
) {
    let Some((line, selection)) = cursor else {
        return;
    };
    if let Some(range) = selection {
        for i in range {
            if let Some(l) = text.lines.get_mut(i) {
                pad_line(l, width);
                for span in &mut l.spans {
                    span.style = span.style.bg(palette.selection);
                }
            }
        }
    }
    if let Some(l) = text.lines.get_mut(line) {
        pad_line(l, width);
        for span in &mut l.spans {
            span.style = span.style.add_modifier(Modifier::REVERSED);
        }
    }
}

/// Pad `line` to `width` with a blank trailing span so a full-line
/// background/reverse overlay covers the whole row, not just its text.
fn pad_line(line: &mut Line<'static>, width: usize) {
    let padding = width.saturating_sub(line.width());
    if padding > 0 {
        line.spans.push(Span::raw(" ".repeat(padding)));
    }
}

pub(crate) fn draw_right_pane(
    frame: &mut Frame<'_>,
    app: &App,
    render: &mut RenderState,
    landed: &mut Landed,
    area: Rect,
) {
    let palette = &app.palette();
    // Remembered for mouse-wheel routing: a wheel event over this rect scrolls
    // the diff, one over the left column moves the selection.
    landed.right_area = Some(area);

    let focused = Style::new()
        .fg(app.theme.config.color())
        .add_modifier(Modifier::BOLD);
    let idle = Style::new().fg(palette.idle);
    // Branches normally previews nothing (" Log "); once drilled into a
    // branch's commit list, a selected row shows a real diff, so the title
    // matches what the Commits pane calls the same view: " Patch ".
    let right_title = if app.nav.focus == Pane::Branches
        && matches!(app.diff_view(), DiffView::Commit(..))
    {
        " Patch "
    } else if app.nav.focus == Pane::Files && !app.is_mock() && app.row_count(Pane::Files) == 0 {
        // Nothing changed: lazygit's "Diff" pane says so, instead of keeping
        // the "Unstaged changes" title over an empty box.
        " Diff "
    } else {
        app.nav.focus.right_title()
    };
    let border = if app.right_focused() { focused } else { idle };

    // An image selection takes over the right pane; otherwise it is mock text.
    match &mut render.preview {
        Preview::Image(proto) => {
            // Same shape as `render_resized_image` in the ratatui-image demo:
            // draw the border, then hand `StatefulImage` the inner area and a
            // `&mut StatefulProtocol` so it resizes + re-encodes to fit.
            let block = Panel::new()
                .title(Line::styled(" Preview ", focused))
                .border_style(border)
                .block();
            let inner = block.inner(area);
            frame.render_widget(block, area);
            frame.render_stateful_widget(
                StatefulImage::new().resize(Resize::Fit(None)),
                inner,
                proto.as_mut(),
            );
            return;
        },
        Preview::Note(msg) => {
            // Blank every cell first: if the previous frame was an image, its
            // sixel / iTerm2 pixels sit under these cells and a short paragraph
            // would not overwrite the rows below it.
            frame.render_widget(Clear, area);
            let panel = Paragraph::new(msg.as_str())
                .block(
                    Panel::new()
                        .title(Line::styled(right_title, focused))
                        .border_style(border)
                        .block(),
                )
                .wrap(Wrap { trim: false });
            frame.render_widget(panel, area);
            return;
        },
        Preview::None => {},
    }

    // Same reason as the `Note` branch: clear any leftover graphics pixels
    // from a previous image frame before drawing the (often short) text pane.
    frame.render_widget(Clear, area);

    let block = Panel::new()
        .title(Line::styled(right_title, focused))
        .border_style(border)
        .block();

    // Real `git show` output (a commit, or a drilled branch's commit): git-
    // native colouring, vertical scroll from `app.right_scroll()`, a reverse-
    // highlight on the file header a `]` / `[` jump last landed on, and a
    // scrollbar when it overflows. A Files selection never reaches here: it
    // gets its own two-column split (`draw_files_columns`) before this
    // function is even called.
    let scroll = app.right_scroll();
    if let DiffView::Commit(_, diff) | DiffView::Stash(_, diff) = app.diff_view() {
        let diff_area = block.inner(area);
        let anchors = diff.file_lines();
        let raw_total = diff.text.lines().count();
        let focus = anchors.iter().position(|&l| l == scroll).map(|i| {
            let end = anchors.get(i + 1).copied().unwrap_or(raw_total);
            scroll..end
        });
        let Some((text, total, _stat)) = app.right.rendered_diff(
            &app.prefs.palette,
            &mut render.diff_cache,
            focus.as_ref(),
            diff_area.width as usize,
        ) else {
            return;
        };
        frame.render_widget(block, area);

        let raw_max = raw_total.saturating_sub(diff_area.height as usize);
        let display_max = total.saturating_sub(diff_area.height as usize);
        let display_scroll = if raw_max == 0 {
            0
        } else {
            scroll
                .min(raw_max)
                .saturating_mul(display_max)
                .checked_div(raw_max)
                .unwrap_or_default()
        };
        let panel =
            Paragraph::new(text).scroll((u16::try_from(display_scroll).unwrap_or(u16::MAX), 0));
        frame.render_widget(panel, diff_area);
        let viewport = diff_area.height as usize;
        ScrollBar::new(total, viewport, display_scroll).render(frame, diff_area);
        landed.right_viewport = Some(diff_area.height as usize);
        return;
    }

    if let DiffView::Note(msg) = app.diff_view() {
        let panel = Paragraph::new(Line::styled(
            msg.clone(),
            Style::new().fg(palette.idle).add_modifier(Modifier::DIM),
        ))
        .block(block)
        .wrap(Wrap { trim: false });
        frame.render_widget(panel, area);
        return;
    }

    // Branches focused, not drilled in: the selected branch's own commits,
    // shown passively (lazygit's live branch -> log preview, no Enter
    // needed) as multi-line `git log`-style blocks (`row_lines::branch_log_block`)
    // rather than the compact one-line rows the Commits pane uses — there is
    // a whole pane's width to spend here. No gutter/stat/hunk-jump, that
    // treatment is for an actual diff once Enter drills into a specific
    // commit, but it does scroll like one (`right_is_diff`), so J/K,
    // PageUp/Down and the wheel move this list instead of leaking through to
    // the Branches selection.
    if let DiffView::BranchLog(log) = app.diff_view() {
        let inner = block.inner(area);
        let lines: Vec<Line<'static>> = if log.commits.is_empty() {
            vec![Line::raw("no commits yet")]
        } else {
            log.commits
                .iter()
                .flat_map(|commit| row_lines::branch_log_block(palette, commit))
                .collect()
        };
        let total = lines.len();
        let scroll = app.right_scroll();
        frame.render_widget(block, area);
        let panel = Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .scroll((u16::try_from(scroll).unwrap_or(u16::MAX), 0));
        frame.render_widget(panel, inner);
        let viewport = inner.height as usize;
        ScrollBar::new(total, viewport, scroll).render(frame, inner);
        landed.right_viewport = Some(viewport);
        return;
    }

    // Status: lazygit's welcome screen, not repo data — same in mock and on
    // a real repo, so this comes before the mock/real split below.
    if app.nav.focus == Pane::Status {
        let panel = Paragraph::new(welcome_lines(
            area.width,
            area.height,
            app.theme.config.color(),
            palette,
        ))
        .block(block)
        .wrap(Wrap { trim: false });
        frame.render_widget(panel, area);
        return;
    }

    // `App::mock()`: the sample text. A real repo with nothing selected (no
    // files, no commits) just leaves the pane blank.
    if !app.is_mock() {
        let empty_files = app.nav.focus == Pane::Files && app.row_count(Pane::Files) == 0;
        let empty_stash = app.nav.focus == Pane::Stash && app.row_count(Pane::Stash) == 0;
        let text = if empty_files {
            "No changed files"
        } else if empty_stash {
            "No stash entries"
        } else {
            ""
        };
        frame.render_widget(Paragraph::new(text).block(block), area);
        return;
    }

    // Status already returned above (the welcome screen shows in mock too).
    // Branches has no mock body either: `App::mock()` has no repo, so there
    // is nothing to preview or drill into (G7); the mock path matches that
    // by leaving it blank rather than showing a fake sample.
    let body = match app.nav.focus {
        Pane::Status | Pane::Branches => "",
        Pane::Files => mock::RIGHT_DIFF,
        Pane::Commits => mock::RIGHT_COMMIT,
        Pane::Stash => mock::RIGHT_STASH,
    };

    let text: Text<'_> = match app.nav.focus {
        Pane::Files | Pane::Commits => row_lines::diff_lines(palette, body, None),
        _ => body.into(),
    };

    let panel = Paragraph::new(text).block(block).wrap(Wrap { trim: false });
    frame.render_widget(panel, area);
}
