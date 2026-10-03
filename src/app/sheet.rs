//! The side sheet: one drawer that slides in from the right over the dimmed
//! panes, shared by every sheet ferrit has (`docs/PLAN_19_DASHBOARD_SHEET.md`).
//! One drawer state means one animation and, by construction, one sheet at a
//! time. What a sheet shows and does with the keys is its own module
//! (`settings`); this one only says which it is and opens and closes it.

use super::App;

/// What the drawer holds. Kept while it slides out, so the last frames of the
/// close are still the sheet that was up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Sheet {
    /// Ferrit's own settings, opened by a click on the author's name.
    #[default]
    Settings,
}

impl App {
    /// Open `sheet` in the drawer, ready for its first frame.
    pub(super) fn open_sheet(&mut self, sheet: Sheet) {
        self.sheet = sheet;
        match sheet {
            Sheet::Settings => self.prepare_settings_sheet(),
        }
        self.sheet_overlay.open();
    }

    /// Slide the drawer out.
    pub(super) fn close_sheet(&mut self) {
        self.sheet_overlay.close();
    }

    /// Whether a sheet is on screen or sliding.
    #[must_use]
    pub fn sheet_is_open(&self) -> bool {
        !self.sheet_overlay.is_closed()
    }
}
