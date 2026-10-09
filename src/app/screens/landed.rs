//! What a frame learned while it was drawn: where each pane landed, how far a
//! list is scrolled, how big the diff viewport is, what the key bar can be
//! clicked on. The mouse code routes a click by these, so they have to reach
//! `App`; they used to be written into it from fifteen places in the middle of
//! the layout code. The draw functions now only fill a `Landed`, and
//! `App::land` applies it once, after the frame (`docs/PLAN_24_DRAW_VIEW.md`).

use ratatui::layout::Rect;

use crate::app::settings::SettingsHits;
use crate::app::{App, Pane, hints};

/// `None` and empty mean "this frame did not touch it": the previous value stays.
#[derive(Default)]
pub(super) struct Landed {
    /// A left pane's bordered rect.
    pub(super) left: Vec<(Pane, Rect)>,
    /// A left pane's list offset after ratatui scrolled it to keep the selection in view.
    pub(super) list_offset: Vec<(Pane, usize)>,
    /// The whole right column.
    pub(super) right_area: Option<Rect>,
    /// Inner height of the right pane's diff box.
    pub(super) right_viewport: Option<usize>,
    /// The author's name in the info panel (`Rect::ZERO` when not drawn).
    pub(super) author: Option<Rect>,
    /// The Dashboard trigger beside it.
    pub(super) dashboard: Option<Rect>,
    /// The key bar's rect and what each part of it runs.
    pub(super) keybar: Option<(Rect, Vec<hints::KeybarHit>)>,
    /// The settings sheet's clickable parts.
    pub(super) settings_hits: Option<SettingsHits>,
    /// The settings sheet's scroll after keeping the selected row in view, and
    /// whether that following has now been done.
    pub(super) settings_scroll: Option<(usize, bool)>,
    /// The git config screen's first visible row.
    pub(super) git_config_offset: Option<usize>,
    /// How many help lines fit.
    pub(super) help_rows: Option<usize>,
    /// How far the dashboard page scrolls, at most.
    pub(super) dashboard_max_scroll: Option<usize>,
}

impl App {
    /// Take in what a frame learned.
    pub(super) fn land(&mut self, landed: Landed) {
        for &(pane, rect) in &landed.left {
            self.hits.left[pane] = rect;
            // A wheel scroll leaves the view where it put it only while the
            // selection stays on the row it left behind.
            if self.hits.view_detached_at[pane] != Some(self.nav.selection[pane]) {
                self.hits.view_detached_at[pane] = None;
            }
        }
        for (pane, offset) in landed.list_offset {
            self.hits.list_offset[pane] = offset;
        }
        if let Some(area) = landed.right_area {
            self.right.area = area;
        }
        if let Some(rows) = landed.right_viewport {
            self.set_right_viewport(rows);
        }
        if let Some(area) = landed.author {
            self.hits.author = area;
        }
        if let Some(area) = landed.dashboard {
            self.hits.dashboard = area;
        }
        if let Some((area, hits)) = landed.keybar {
            self.hits.keybar = area;
            self.hits.keybar_hits = hits;
        }
        if let Some(hits) = landed.settings_hits {
            self.hits.settings = hits;
        }
        if let Some((scroll, followed)) = landed.settings_scroll {
            self.sheets.settings.scroll = scroll;
            if followed {
                self.sheets.settings.follow = false;
            }
        }
        if let Some(offset) = landed.git_config_offset {
            self.set_git_config_offset(offset);
        }
        if let Some(rows) = landed.help_rows {
            self.help.set_rows(rows);
        }
        if let Some(max) = landed.dashboard_max_scroll {
            self.clamp_dashboard_scroll(max);
        }
    }
}
