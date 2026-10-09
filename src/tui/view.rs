//! The read-only questions the screens and the tests ask of `App`.

use std::ops::Range;

use crate::git::diff::DiffSide;
use crate::tui::App;
use crate::tui::components::diff::Mode;

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
