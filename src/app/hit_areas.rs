//! Where the last frame put things that can be clicked, and the scroll
//! bookkeeping a click needs to map back to a row. Drawing writes these, the
//! mouse handlers read them, so a click is routed by what the user saw.
//! `Rect::ZERO` and empty lists before the first draw.

use enum_map::EnumMap;
use ratatui::layout::Rect;

use super::settings::SettingsHits;
use crate::app::pane::Pane;
use crate::keybindings::hints;

#[derive(Default)]
pub(crate) struct HitAreas {
    /// Each left pane's bordered rect, for routing a click to the pane it
    /// landed in.
    pub(crate) left: EnumMap<Pane, Rect>,
    /// `ListState::offset` for each left pane, copied back by
    /// `ui::draw_left_column` after `render_stateful_widget` moves it to keep
    /// the selection on screen. Lets a click in a scrolled list map to the
    /// right row. Only valid post-render; 0 before the first draw.
    pub(crate) list_offset: EnumMap<Pane, usize>,
    /// Per left pane, the selected row a wheel scroll left behind: while the
    /// selection is still that row, the view stays where the wheel put it,
    /// even with the selection off screen (lazygit). Any other selection
    /// re-attaches the view to it, so no key or click has to clear this.
    pub(crate) view_detached_at: EnumMap<Pane, Option<usize>>,
    /// The configured Git author in the bottom info panel.
    pub(crate) author: Rect,
    /// The visible Dashboard trigger beside the author.
    pub(crate) dashboard: Rect,
    /// Where the keybar was drawn and what each part of it runs when clicked.
    pub(crate) keybar: Rect,
    pub(crate) keybar_hits: Vec<hints::KeybarHit>,
    /// The settings sheet's clickable parts.
    pub(crate) settings: SettingsHits,
}

impl HitAreas {
    pub(crate) fn list_offset(&self, pane: Pane) -> usize {
        self.list_offset[pane]
    }

    pub(crate) fn set_list_offset(&mut self, pane: Pane, offset: usize) {
        self.list_offset[pane] = offset;
    }

    /// Whether a wheel scroll left `pane`'s view away from `selected`, its
    /// selected row: the detachment ends when the selection moves.
    pub(crate) fn view_detached(&self, pane: Pane, selected: usize) -> bool {
        self.view_detached_at[pane] == Some(selected)
    }

    /// Scroll `pane`'s list by `rows` (negative is up) and keep its selection
    /// where it is, which may leave it off screen. `draw_left_column` clamps
    /// the offset to the list's length on the next frame.
    pub(crate) fn scroll_list(&mut self, pane: Pane, selected: usize, rows: isize) {
        self.view_detached_at[pane] = Some(selected);
        self.list_offset[pane] = self.list_offset[pane].saturating_add_signed(rows);
    }
}
