//! Where the last frame put things that can be clicked, and the scroll
//! bookkeeping a click needs to map back to a row. Drawing writes these, the
//! mouse handlers read them, so a click is routed by what the user saw.
//! `Rect::ZERO` and empty lists before the first draw.

use enum_map::EnumMap;
use ratatui::layout::Rect;

use super::settings::SettingsHits;
use super::{Pane, hints};

#[derive(Default)]
pub struct HitAreas {
    /// Each left pane's bordered rect, for routing a click to the pane it
    /// landed in.
    pub(super) left: EnumMap<Pane, Rect>,
    /// `ListState::offset` for each left pane, copied back by
    /// `ui::draw_left_column` after `render_stateful_widget` moves it to keep
    /// the selection on screen. Lets a click in a scrolled list map to the
    /// right row. Only valid post-render; 0 before the first draw.
    pub(super) list_offset: EnumMap<Pane, usize>,
    /// Per left pane, the selected row a wheel scroll left behind: while the
    /// selection is still that row, the view stays where the wheel put it,
    /// even with the selection off screen (lazygit). Any other selection
    /// re-attaches the view to it, so no key or click has to clear this.
    pub(super) view_detached_at: EnumMap<Pane, Option<usize>>,
    /// The configured Git author in the bottom info panel.
    pub(super) author: Rect,
    /// The visible Dashboard trigger beside the author.
    pub(super) dashboard: Rect,
    /// Where the keybar was drawn and what each part of it runs when clicked.
    pub(super) keybar: Rect,
    pub(super) keybar_hits: Vec<hints::KeybarHit>,
    /// The settings sheet's clickable parts.
    pub(super) settings: SettingsHits,
}
