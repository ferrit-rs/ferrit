//! Dashboard feature: statistics state and terminal projection.

pub mod chart_palette;
pub mod state;
pub mod view;

mod charts;
mod tables;
mod text;

use crate::ui::components::settings::state::SettingsSheet;

/// The drawer and what it can hold. One drawer state means one animation and,
/// by construction, one sheet at a time.
#[derive(Default)]
pub(crate) struct Sheets {
    /// Which sheet the drawer holds while it is not closed.
    pub(crate) kind: Sheet,
    pub(crate) settings: SettingsSheet,
    pub(crate) dashboard: state::Dashboard,
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
