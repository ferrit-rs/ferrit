//! The right column: what it shows for the selection (an image, a diff), where
//! it is scrolled, and the line cursor used to stage hunks and lines. Building
//! the diff and handling its keys stay with the `App` methods in `diff_query`,
//! `image_query` and `staging`; this is the state they read and write.

use ratatui::layout::Rect;
use ratatui_image::picker::Picker;

use super::diff_query::RightKey;
use super::{DiffCursor, DiffView};

pub struct RightPane {
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
