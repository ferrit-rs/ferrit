//! Keyboard and mouse input dispatch.

use super::{
    App, KeyCode, KeyEvent, KeyModifiers, Mode, MouseButton, MouseEvent, MouseEventKind, PANES,
    Pane, Position, WHEEL_LINES, events, git,
};

impl App {
    pub(super) fn on_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.should_quit = true;
            return;
        }

        // A popup (commit message box, or a dismissible note) owns all
        // input while it is up, same idea as the help overlay below but
        // richer (`docs/PLAN_7_COMMIT.md`).
        if self.popup.is_some() {
            self.popup_key(key);
            return;
        }

        // A discard / branch-delete confirmation swallows every key but its
        // own answer, same as the help overlay below.
        if self.pending_confirm.is_some() {
            match key.code {
                KeyCode::Char('y') => self.run_confirm(),
                KeyCode::Char('n') | KeyCode::Esc => self.pending_confirm = None,
                _ => {},
            }
            self.update_right_pane();
            return;
        }

        if self.show_help {
            if matches!(key.code, KeyCode::Char('?' | 'q') | KeyCode::Esc) {
                self.show_help = false;
            }
            return;
        }

        // `Mode::Diff` keys (Files pane, cursor focused into the diff) take
        // priority; unhandled ones fall through to the ordinary scroll block
        // and the generic match below, same as `Mode::Nav`.
        if self.on_diff_key(key) {
            self.update_right_pane();
            return;
        }

        // Right-pane diff scroll, lazygit's "scroll the main view without
        // leaving the side panel": J / K by a line, PageUp / PageDown by a
        // page, Ctrl-u / Ctrl-d by a half page, < / > to the ends, ] / [
        // between hunks (or files, for a commit). Steps come from the tracked
        // viewport height. None of these change the selection, so they skip
        // the `update_right_pane` rebuild and its diff subprocess. Inert unless
        // the right pane is a real diff.
        if self.right_is_diff() {
            let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
            let half = isize::try_from((self.right_viewport / 2).max(1)).unwrap_or(isize::MAX);
            let page =
                isize::try_from(self.right_viewport.saturating_sub(1).max(1)).unwrap_or(isize::MAX);
            match key.code {
                KeyCode::Char('d') if ctrl => return self.scroll_right(half),
                KeyCode::Char('u') if ctrl => return self.scroll_right(-half),
                KeyCode::Char('J') => return self.scroll_right(1),
                KeyCode::Char('K') => return self.scroll_right(-1),
                KeyCode::PageDown => return self.scroll_right(page),
                KeyCode::PageUp => return self.scroll_right(-page),
                KeyCode::Char('>') => return self.scroll_right(isize::MAX),
                KeyCode::Char('<') => return self.scroll_right(isize::MIN),
                KeyCode::Char(']') => return self.jump_diff_anchor(1),
                KeyCode::Char('[') => return self.jump_diff_anchor(-1),
                _ => {},
            }
        }

        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('?') => self.show_help = true,
            KeyCode::Esc => {
                self.right_focused = false;
                if let Some(drill) = self.branch_drill.take() {
                    self.selection[Pane::Branches] = drill.return_index;
                }
                if let Some(drill) = self.commit_drill.take() {
                    self.selection[Pane::Commits] = drill.return_index;
                }
            },
            KeyCode::Enter => {
                self.enter_branch_log();
                self.enter_commit_files();
                self.toggle_files_dir();
                self.toggle_commit_dir();
                self.enter_diff_mode();
            },
            KeyCode::Char('l') => self.enter_diff_mode(),
            KeyCode::Char(' ') if self.focus == Pane::Branches => self.checkout_selected_branch(),
            KeyCode::Char(' ') => self.stage_selected_file(),
            KeyCode::Char('a') => self.stage_all_files(),
            KeyCode::Char('n') if self.focus == Pane::Branches => self.open_new_branch_popup(),
            KeyCode::Char('u') if self.focus == Pane::Branches => {
                self.fast_forward_selected_branch();
            },
            KeyCode::Char('M') if self.focus == Pane::Branches => self.merge_selected_branch(),
            KeyCode::Char('d') if self.focus == Pane::Branches && self.mode == Mode::Nav => {
                self.delete_branch_prompt();
            },
            KeyCode::Char('d') => self.discard_prompt(),
            KeyCode::Char('c') => self.open_commit(git::commit::CommitKind::Normal),
            KeyCode::Char('A') => self.open_commit(git::commit::CommitKind::Amend),
            KeyCode::Char('w') => self.open_commit(git::commit::CommitKind::Reword),
            KeyCode::Char('r') => self.request_refresh(),
            KeyCode::Char('f') => self.trigger_remote_op(events::RemoteOp::Fetch),
            KeyCode::Char('p') => self.trigger_remote_op(events::RemoteOp::Pull),
            KeyCode::Char('P') => self.push_current_branch(),
            KeyCode::Char(c @ '1'..='5') => {
                if let Some(&pane) = PANES.get(c as usize - '1' as usize) {
                    self.focus = pane;
                }
            },
            KeyCode::Right | KeyCode::Left if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.toggle_branches_tab();
            },
            KeyCode::Tab | KeyCode::Right => self.focus = self.pane_offset(1),
            KeyCode::BackTab | KeyCode::Left => self.focus = self.pane_offset(PANES.len() - 1),
            KeyCode::Char('j') | KeyCode::Down => self.select_down(),
            KeyCode::Char('k') | KeyCode::Up => self.select_up(),
            _ => {},
        }

        // Focus or selection may have moved; keep the right-pane preview in sync.
        self.update_right_pane();
    }

    /// A left click focuses the pane it lands in and, when it lands on a
    /// list row, moves that pane's selection cursor there too (lazygit's
    /// `HandleClick`, steps 3 / 4 / 5 / 7); on a Files directory row, it
    /// also toggles it collapsed/expanded, same as `Enter`. Any click
    /// dismisses the help overlay first. Right click, middle click, drag
    /// and move are no-ops for now.
    pub(super) fn on_mouse(&mut self, ev: MouseEvent) {
        match ev.kind {
            MouseEventKind::ScrollDown => return self.wheel(ev, 1),
            MouseEventKind::ScrollUp => return self.wheel(ev, -1),
            MouseEventKind::Down(MouseButton::Left) => {},
            MouseEventKind::Down(MouseButton::Right) => return, // phase 12: `x` context menu
            _ => return,                                        // middle click, drag, move
        }

        if self.show_help {
            self.show_help = false; // any click dismisses the overlay
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
    /// over the main view"); over the left column it nudges the focused
    /// pane's selection.
    pub(super) fn wheel(&mut self, ev: MouseEvent, step: isize) {
        let a = self.right_area;
        let over_right = ev.column >= a.x && ev.column < a.x.saturating_add(a.width);
        if over_right && self.right_is_diff() {
            self.scroll_right(step * WHEEL_LINES);
            return;
        }
        if step > 0 {
            self.select_down();
        } else {
            self.select_up();
        }
        self.update_right_pane();
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
