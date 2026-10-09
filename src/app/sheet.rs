//! What the keys do in `App` for `sheet`: the glue between the interface, the git code and the app's state.

use crate::app::App;
use crate::interface::components::tui_overlay::state::OverlayState;
use crate::interface::state::sheet::Sheet;

impl App {
    /// Open `sheet` in the drawer, ready for its first frame.
    pub(crate) fn open_sheet(&mut self, sheet: Sheet) {
        self.sheets.kind = sheet;
        match sheet {
            Sheet::Settings => self.prepare_settings_sheet(),
            Sheet::Dashboard => self.prepare_dashboard_sheet(),
        }
        self.render.sheet.open();
    }

    /// Slide the drawer out.
    pub(crate) fn close_sheet(&mut self) {
        self.render.sheet.close();
    }

    /// Whether the dashboard is up: sliding in or in, not on its way out.
    #[must_use]
    pub fn dashboard_is_open(&self) -> bool {
        self.dashboard_open_in(&self.render.sheet)
    }

    /// `dashboard_is_open` for a drawer animation held elsewhere: drawing owns the
    /// render state while it runs, so it asks with its own.
    pub(crate) fn dashboard_open_in(&self, drawer: &OverlayState) -> bool {
        self.sheets.kind == Sheet::Dashboard && !drawer.is_closed() && !drawer.is_closing()
    }

    /// Whether a sheet is on screen or sliding.
    #[must_use]
    pub fn sheet_is_open(&self) -> bool {
        !self.render.sheet.is_closed()
    }
}
