//! The read-only questions the screens and the tests ask of `App`.

use std::ops::Range;

use crate::git::diff::DiffSide;
use crate::tui::App;
use crate::tui::components::diff::{CommitPopupView, Mode};
use crate::tui::components::popups::Popup;

impl App {
    /// Cursor state for the right-pane render: `(side, cursor line, V-select
    /// range)` while `Mode::Diff` is up, else `None`.
    pub fn diff_cursor(&self) -> Option<(DiffSide, usize, Option<Range<usize>>)> {
        (self.nav.mode == Mode::Diff).then(|| self.right.cursor_view())
    }

    /// Right-pane title suffix while `Mode::Diff` is up: `hunk 1/3` or
    /// `lines 41-42`/`line 41`, so it is obvious what `<space>` will hit.
    pub fn diff_granule_hint(&self) -> Option<String> {
        if self.nav.mode != Mode::Diff {
            return None;
        }
        self.right.granule_hint()
    }
}

impl App {
    /// The credential prompt as data, when it is up.
    pub fn askpass_popup(&self) -> Option<CommitPopupView<'_>> {
        let Some(Popup::Askpass(ask)) = self.modal.popup() else {
            return None;
        };
        Some(CommitPopupView {
            title: ask.prompt.trim_end().trim_end_matches(':'),
            input: &ask.shown,
            description: None,
            summary_focused: false,
            overlay_state: None,
            lines: ask.shown.lines(),
            cursor: ask.shown.cursor(),
            toggles: None,
            author: None,
            hints: "Send: Enter | Cancel: Esc",
        })
    }
}
