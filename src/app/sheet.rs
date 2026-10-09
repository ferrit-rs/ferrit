//! The side sheet: one drawer that slides in from the right over the dimmed
//! panes, shared by every sheet ferrit has (`docs/PLAN_19_DASHBOARD_SHEET.md`).
//! One drawer state means one animation and, by construction, one sheet at a
//! time. What a sheet shows and does with the keys is its own module
//! (`settings`); this one only says which it is and opens and closes it.

use super::settings::SettingsSheet;
use super::{App, dashboard};
use crate::components::tui_overlay::state::OverlayState;

/// The drawer and what it can hold. One drawer state means one animation and,
/// by construction, one sheet at a time.
#[derive(Default)]
pub struct Sheets {
    /// Which sheet the drawer holds while it is not closed.
    pub(super) kind: Sheet,
    pub(super) settings: SettingsSheet,
    pub(super) dashboard: dashboard::Dashboard,
}

/// What the drawer holds. Kept while it slides out, so the last frames of the
/// close are still the sheet that was up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Sheet {
    /// Ferrit's own settings, opened by a click on the author's name.
    #[default]
    Settings,
    /// The repository dashboard, opened by `D` (`docs/PLAN_13_DASHBOARD.md`).
    Dashboard,
}

impl App {
    /// Open `sheet` in the drawer, ready for its first frame.
    pub(super) fn open_sheet(&mut self, sheet: Sheet) {
        self.sheets.kind = sheet;
        match sheet {
            Sheet::Settings => self.prepare_settings_sheet(),
            Sheet::Dashboard => self.prepare_dashboard_sheet(),
        }
        self.render.sheet.open();
    }

    /// Slide the drawer out.
    pub(super) fn close_sheet(&mut self) {
        self.render.sheet.close();
    }

    /// Whether the dashboard is up: sliding in or in, not on its way out.
    #[must_use]
    pub fn dashboard_is_open(&self) -> bool {
        self.dashboard_open_in(&self.render.sheet)
    }

    /// `dashboard_is_open` for a drawer animation held elsewhere: drawing owns the
    /// render state while it runs, so it asks with its own.
    pub(super) fn dashboard_open_in(&self, drawer: &OverlayState) -> bool {
        self.sheets.kind == Sheet::Dashboard && !drawer.is_closed() && !drawer.is_closing()
    }

    /// Whether a sheet is on screen or sliding.
    #[must_use]
    pub fn sheet_is_open(&self) -> bool {
        !self.render.sheet.is_closed()
    }
}
