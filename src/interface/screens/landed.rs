//! What a frame learned while it was drawn: where each pane landed, how far a
//! list is scrolled, how big the diff viewport is, what the key bar can be
//! clicked on. The mouse code routes a click by these, so they have to reach
//! `App`; they used to be written into it from fifteen places in the middle of
//! the layout code. The draw functions now only fill a `Landed`, and
//! `App::land` applies it once, after the frame (`docs/PLAN_24_DRAW_VIEW.md`).

use ratatui::layout::Rect;

use crate::config::settings_sheet::SettingsHits;
use crate::interface::panes::pane::Pane;
use crate::keybindings::hints;

/// `None` and empty mean "this frame did not touch it": the previous value stays.
#[derive(Default)]
pub(crate) struct Landed {
    /// A left pane's bordered rect.
    pub(crate) left: Vec<(Pane, Rect)>,
    /// A left pane's list offset after ratatui scrolled it to keep the selection in view.
    pub(crate) list_offset: Vec<(Pane, usize)>,
    /// The whole right column.
    pub(crate) right_area: Option<Rect>,
    /// Inner height of the right pane's diff box.
    pub(crate) right_viewport: Option<usize>,
    /// The author's name in the info panel (`Rect::ZERO` when not drawn).
    pub(crate) author: Option<Rect>,
    /// The Dashboard trigger beside it.
    pub(crate) dashboard: Option<Rect>,
    /// The key bar's rect and what each part of it runs.
    pub(crate) keybar: Option<(Rect, Vec<hints::KeybarHit>)>,
    /// The settings sheet's clickable parts.
    pub(crate) settings_hits: Option<SettingsHits>,
    /// The settings sheet's scroll after keeping the selected row in view, and
    /// whether that following has now been done.
    pub(crate) settings_scroll: Option<(usize, bool)>,
    /// The git config screen's first visible row.
    pub(crate) git_config_offset: Option<usize>,
    /// How many help lines fit.
    pub(crate) help_rows: Option<usize>,
    /// How far the dashboard page scrolls, at most.
    pub(crate) dashboard_max_scroll: Option<usize>,
}
