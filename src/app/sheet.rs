//! The side sheet: one drawer that slides in from the right over the dimmed
//! panes, shared by every sheet ferrit has (`docs/PLAN_19_DASHBOARD_SHEET.md`).
//! One drawer state means one animation and, by construction, one sheet at a
//! time. What a sheet shows and does with the keys is its own module
//! (`settings`); this one only says which it is and opens and closes it.

use std::time::Duration;

use super::settings::SettingsSheet;
use super::{App, dashboard};
use crate::components::tui_overlay::state::OverlayState;

/// The drawer and what it can hold. One drawer state means one animation and,
/// by construction, one sheet at a time.
pub struct Sheets {
    /// Which sheet the drawer holds while it is not closed.
    pub(super) kind: Sheet,
    /// The drawer's slide-in and backdrop.
    pub(crate) overlay: OverlayState,
    pub(super) settings: SettingsSheet,
    pub(super) dashboard: dashboard::Dashboard,
}

impl Default for Sheets {
    fn default() -> Self {
        Self {
            kind: Sheet::default(),
            overlay: OverlayState::new().with_duration(Duration::from_millis(200)),
            settings: SettingsSheet::default(),
            dashboard: dashboard::Dashboard::default(),
        }
    }
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
        self.sheets.overlay.open();
    }

    /// Slide the drawer out.
    pub(super) fn close_sheet(&mut self) {
        self.sheets.overlay.close();
    }

    /// Whether the dashboard is up: sliding in or in, not on its way out.
    #[must_use]
    pub fn dashboard_is_open(&self) -> bool {
        self.sheets.kind == Sheet::Dashboard
            && !self.sheets.overlay.is_closed()
            && !self.sheets.overlay.is_closing()
    }

    /// Whether a sheet is on screen or sliding.
    #[must_use]
    pub fn sheet_is_open(&self) -> bool {
        !self.sheets.overlay.is_closed()
    }
}
