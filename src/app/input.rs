//! Keyboard and mouse input dispatch.

use super::sheet::Sheet;
use super::{
    App, FullScreen, KeyCode, KeyEvent, KeyModifiers, Mode, MouseButton, MouseEvent,
    MouseEventKind, PANES, Pane, Position,
};
use crate::app::hints;

const KEY_CONFIRM_YES: char = 'y';
const KEY_CONFIRM_NO: char = 'n';

/// Rows a wheel tick scrolls a left pane's list: lazygit's `scrollHeight`.
const LIST_WHEEL_ROWS: isize = 2;

impl App {
    pub(super) fn on_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.should_quit = true;
            return;
        }

        // `Esc` closes the error toast, when no popup or question needs the key.
        if key.code == KeyCode::Esc && self.dismiss_toast() {
            return;
        }

        // A popup (commit message box, or a dismissible note) owns all
        // input while it is up, same idea as the help overlay below but
        // richer (`docs/PLAN_7_COMMIT.md`).
        if self.popup.is_some() {
            self.popup_key(key);
            return;
        }

        // A key-bar confirmation swallows every key but its own answer (Enter
        // or `y` confirms, so Enter never reaches the pane), same as the
        // help overlay below.
        if self.pending_confirm.is_some() {
            match key.code {
                KeyCode::Enter | KeyCode::Char(KEY_CONFIRM_YES | 'Y') => self.run_confirm(),
                KeyCode::Char(KEY_CONFIRM_NO | 'N') | KeyCode::Esc => {
                    self.pending_confirm = None;
                },
                _ => {},
            }
            self.update_right_pane();
            return;
        }

        if self.help.is_visible() {
            if self.help.open {
                self.help_key(key);
            }
            return;
        }

        // A full-screen view owns the keys after the overlays above.
        if self.full_screen == FullScreen::GitConfig {
            self.git_config_key(key);
            return;
        }
        if self.full_screen == FullScreen::Welcome {
            self.welcome_key(key);
            return;
        }

        if !self.sheet_overlay.is_closed() {
            match self.sheet {
                Sheet::Settings => self.settings_key(key),
                Sheet::Dashboard => self.dashboard_key(key),
            }
            return;
        }

        // Everything else is a keymap lookup (`app::keymap`): the diff cursor
        // keys, the right-pane scroll keys, then the per-pane and global ones.
        self.dispatch_key(key);
    }

    /// Every key while the help screen is up: `/` searches, `j` / `k` and the
    /// arrows scroll, `PgUp` / `PgDn` a page, `Home` / `End` the ends; `?`,
    /// `q` and `Esc` close it. Not remappable, like the other overlays.
    fn help_key(&mut self, key: KeyEvent) {
        if self.help.is_searching() {
            self.help.search_key(key);
            return;
        }
        if key.code == KeyCode::Char('/') {
            self.help.start_search();
            return;
        }
        if matches!(key.code, KeyCode::Char('?' | 'q') | KeyCode::Esc) {
            self.help.dismiss();
            return;
        }
        let total = hints::filter_help_lines(&self.help_lines(), &self.help.query().text()).len();
        self.help.scroll_key(key.code, total);
    }

    /// A left click focuses the pane it lands in and, when it lands on a
    /// list row, moves that pane's selection cursor there too (lazygit's
    /// `HandleClick`, steps 3 / 4 / 5 / 7); on a Files directory row, it
    /// also toggles it collapsed/expanded, same as `Enter`. Any click
    /// dismisses the help overlay first, and a click on a keybar hint runs its
    /// action. A right click opens the row's `x` menu; middle click, drag and
    /// move are no-ops.
    pub(super) fn on_mouse(&mut self, ev: MouseEvent) {
        // `[ui] mouse = false` never enables mouse capture; a terminal that
        // sends events anyway still gets no reaction from ferrit.
        if !self.config.ui.mouse {
            return;
        }
        if matches!(ev.kind, MouseEventKind::Moved) {
            let over_author = self.sheet_overlay.is_closed()
                && self
                    .author_click_area
                    .contains(Position::new(ev.column, ev.row));
            let over_dashboard = self.sheet_overlay.is_closed()
                && self
                    .dashboard_click_area
                    .contains(Position::new(ev.column, ev.row));
            self.mouse_pointer.request(over_author || over_dashboard);
            return;
        }

        // The panes are not on screen: their areas from the last frame must not
        // answer clicks. The wheel scrolls the dashboard.
        if self.full_screen == FullScreen::GitConfig {
            self.git_config_mouse(ev);
            return;
        }
        // Nothing to click without a repository.
        if self.full_screen == FullScreen::Welcome {
            return;
        }

        if !self.sheet_overlay.is_closed() {
            match self.sheet {
                Sheet::Settings => self.settings_mouse(ev),
                Sheet::Dashboard => self.dashboard_mouse(ev),
            }
            return;
        }

        match ev.kind {
            MouseEventKind::ScrollDown => return self.wheel(ev, 1),
            MouseEventKind::ScrollUp => return self.wheel(ev, -1),
            MouseEventKind::Down(MouseButton::Left) => {},
            MouseEventKind::Down(MouseButton::Right) => return self.right_click(ev.column, ev.row),
            _ => return, // middle click, drag, move
        }

        if self.help.is_visible() {
            self.help.dismiss(); // any click dismisses the overlay
            return;
        }

        if self.popup.is_none()
            && self.pending_confirm.is_none()
            && self.keybar_area.contains(Position::new(ev.column, ev.row))
        {
            let column = ev.column - self.keybar_area.x;
            let clicked = self
                .keybar_hits
                .iter()
                .find(|hit| (hit.start..hit.end).contains(&column))
                .map(|hit| hit.action);
            if let Some(action) = clicked {
                self.run_action(action);
                self.update_right_pane();
            }
            return;
        }

        if self
            .author_click_area
            .contains(Position::new(ev.column, ev.row))
        {
            self.mouse_pointer.request(false);
            self.open_sheet(Sheet::Settings);
            return;
        }

        if self
            .dashboard_click_area
            .contains(Position::new(ev.column, ev.row))
        {
            self.mouse_pointer.request(false);
            self.open_dashboard();
            return;
        }

        if let Some(pane) = self.pane_at(ev.column, ev.row) {
            self.right_focused = false; // a left click always returns focus left
            self.mode = Mode::Nav; // a click is a Nav-mode gesture, not diff-cursor movement
            let landed = self.click_pane(pane, ev.row);
            // lazygit toggles a Files directory row on click, not just on
            // Enter — the whole row is the target, not just its arrow
            // glyph, same as it already is for plain selection.
            if landed && pane == Pane::Files {
                self.toggle_files_dir();
            }
            self.update_right_pane(); // step 7: rebuild for the new focus/selection
        } else if self.right_area.contains(Position::new(ev.column, ev.row)) {
            self.right_focused = true;
        }
        // else: command log / keybar / gap. no-op.
    }

    /// Which left pane a screen cell is in, `None` for the right pane, the
    /// command log, the keybar or an inter-pane gap.
    pub(super) fn pane_at(&self, col: u16, row: u16) -> Option<Pane> {
        let point = Position::new(col, row);
        PANES
            .into_iter()
            .find(|&pane| self.left_areas[pane].contains(point))
    }

    /// Focus `pane`, then move its cursor to `screen_row` if that row maps
    /// to a real entry. Returns whether the cursor moved: `false` for the
    /// border / title row and for a click past the last entry.
    pub(super) fn click_pane(&mut self, pane: Pane, screen_row: u16) -> bool {
        self.focus = pane; // focus first, even on the border or past the tail
        let Some(idx) = self.click_row(pane, screen_row) else {
            return false;
        };
        self.selection[pane] = idx;
        true
    }

    /// Screen row -> model index for a left pane. `None` for the border /
    /// title row, or a click past the last entry.
    pub(super) fn click_row(&self, pane: Pane, screen_row: u16) -> Option<usize> {
        let area = self.left_areas[pane];
        let inner_row = screen_row.checked_sub(area.y.saturating_add(1))?;
        let idx = self.list_offset[pane].saturating_add(usize::from(inner_row));
        (idx < self.row_count(pane)).then_some(idx)
    }

    /// Mouse wheel over the right column scrolls the diff (lazygit's "wheel
    /// over the main view"); over a left pane it scrolls that pane's list.
    pub(super) fn wheel(&mut self, ev: MouseEvent, step: isize) {
        let a = self.right_area;
        let over_right = ev.column >= a.x && ev.column < a.x.saturating_add(a.width);
        if over_right && self.right_is_diff() {
            self.scroll_right(step * isize::from(self.config.ui.wheel_step));
            return;
        }
        // Over a left pane the wheel scrolls that pane's view, wherever the
        // focus is, and leaves the focus, the selection and the right pane
        // alone (lazygit). Anywhere else there is nothing to scroll.
        if let Some(pane) = self.pane_at(ev.column, ev.row) {
            self.scroll_list(pane, step * LIST_WHEEL_ROWS);
        }
    }

    pub(super) fn pane_offset(&self, delta: usize) -> Pane {
        let idx = (self.focus.index() + delta) % PANES.len();
        PANES.get(idx).copied().unwrap_or(self.focus)
    }

    pub(super) fn select_down(&mut self) {
        let last = self.row_count(self.focus).saturating_sub(1);
        let cursor = &mut self.selection[self.focus];
        *cursor = (*cursor + 1).min(last);
    }

    pub(super) fn select_up(&mut self) {
        let cursor = &mut self.selection[self.focus];
        *cursor = cursor.saturating_sub(1);
    }
}
