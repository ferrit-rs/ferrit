//! The side sheet: one drawer that slides in from the right over the dimmed
//! panes, shared by every sheet ferrit has (`docs/PLAN_19_DASHBOARD_SHEET.md`).
//! One drawer state means one animation and, by construction, one sheet at a
//! time. What a sheet shows and does with the keys is its own module
//! (`settings`); this one only says which it is and opens and closes it.

use super::dashboard;
use crate::config::settings::SettingsSheet;

/// The drawer and what it can hold. One drawer state means one animation and,
/// by construction, one sheet at a time.
#[derive(Default)]
pub(crate) struct Sheets {
    /// Which sheet the drawer holds while it is not closed.
    pub(crate) kind: Sheet,
    pub(crate) settings: SettingsSheet,
    pub(crate) dashboard: dashboard::Dashboard,
}

/// What the drawer holds. Kept while it slides out, so the last frames of the
/// close are still the sheet that was up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum Sheet {
    /// Ferrit's own settings, opened by a click on the author's name.
    #[default]
    Settings,
    /// The repository dashboard, opened by `D` (`docs/PLAN_13_DASHBOARD.md`).
    Dashboard,
}
