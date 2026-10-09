//! What ratatui needs mutable to show the app, as opposed to the app itself:
//! the slide-in animation of the help, of the side sheet and of the commit
//! popup, and the error toast (which owns its own animation). Key handlers
//! open and close them and the run loop ticks them; drawing renders through
//! them. They are kept apart so drawing can read `&App` and take only these as
//! `&mut` (`docs/PLAN_24_DRAW_VIEW.md`).

use std::time::Duration;

use crate::components::tui_overlay::state::OverlayState;
use crate::components::ui::toast::Toast;

pub(super) struct RenderState {
    /// The help dialog.
    pub(super) help: OverlayState,
    /// The side drawer, whichever sheet it holds.
    pub(super) sheet: OverlayState,
    /// The backdrop of the commit editor and of the "stage everything?" question.
    pub(super) commit: OverlayState,
    /// The bottom-right error notification, dismissed by its `x`, `Esc` or a timeout.
    pub(super) toast: Option<Toast>,
}

impl Default for RenderState {
    fn default() -> Self {
        Self {
            help: OverlayState::new().with_duration(Duration::from_millis(180)),
            sheet: OverlayState::new().with_duration(Duration::from_millis(200)),
            commit: OverlayState::new(),
            toast: None,
        }
    }
}
