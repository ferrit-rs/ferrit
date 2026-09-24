//! Keyboard and mouse input dispatch.

use super::theme_config::ThemeMode;
use super::{
    App, KeyCode, KeyEvent, KeyModifiers, Mode, MouseButton, MouseEvent, MouseEventKind, PANES,
    Pane, Position, WHEEL_LINES, events, git,
};

const KEY_THEME_PALETTE: char = 'p';
const KEY_THEME_PRESET: char = 't';
const KEY_THEME_RGB: char = 'e';
const KEY_THEME_VIEW: char = 'v';
const KEY_THEME_SAVE: char = 's';
const KEY_GIT_AUTHOR: char = '0';
const KEY_CONFIRM_YES: char = 'y';
const KEY_CONFIRM_NO: char = 'n';
const AUTHOR_KEY_START: char = '1';
const AUTHOR_KEY_END: char = '9';
const KEY_NAV_LEFT: char = 'h';
const KEY_NAV_DOWN: char = 'j';
const KEY_NAV_UP: char = 'k';
const KEY_NAV_RIGHT: char = 'l';
const RGB_CHANNEL_STEP: i16 = 8;
const RGB_CHANNEL_INDEX_STEP: usize = 1;
const RGB_MIN_VALUE: i16 = 0;
const RGB_MAX_VALUE: i16 = 255;
const AUTHOR_CHANGE_CONFIRM_MESSAGE: &str =
    "Use this global Git user for future Ferrit commits? Git config stays unchanged.";
const AUTHOR_RESET_CONFIRM_MESSAGE: &str = "Return to the identity resolved from Git config?";

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
                KeyCode::Char(KEY_CONFIRM_YES | 'Y') => self.run_confirm(),
                KeyCode::Char(KEY_CONFIRM_NO | 'N') | KeyCode::Esc => {
                    self.pending_confirm = None;
                },
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

        if !self.author_overlay.is_closed() {
            if let KeyCode::Char(key @ AUTHOR_KEY_START..=AUTHOR_KEY_END) = key.code {
                let index =
                    usize::try_from(key as u32 - AUTHOR_KEY_START as u32).unwrap_or(usize::MAX);
                if let Some(identity) = self.profile.settings.available_identities().get(index) {
                    self.request_author_selection(Some(identity.clone()));
                }
                return;
            }
            if key.code == KeyCode::Char(KEY_GIT_AUTHOR) {
                if self.selected_author.is_some() {
                    self.request_author_selection(None);
                }
                return;
            }
            if key.code == KeyCode::Char(KEY_THEME_SAVE) {
                self.save_theme();
                return;
            }
            if self.theme_mode == ThemeMode::Palette {
                match key.code {
                    KeyCode::Esc | KeyCode::Char(KEY_THEME_PALETTE) => {
                        self.theme_mode = ThemeMode::Idle;
                    },
                    KeyCode::Left | KeyCode::Char(KEY_NAV_LEFT) => self.move_theme_palette(
                        crate::components::ui::color_picker::PaletteDirection::Left,
                    ),
                    KeyCode::Right | KeyCode::Char(KEY_NAV_RIGHT) => self.move_theme_palette(
                        crate::components::ui::color_picker::PaletteDirection::Right,
                    ),
                    KeyCode::Up | KeyCode::Char(KEY_NAV_UP) => self.move_theme_palette(
                        crate::components::ui::color_picker::PaletteDirection::Up,
                    ),
                    KeyCode::Down | KeyCode::Char(KEY_NAV_DOWN) => self.move_theme_palette(
                        crate::components::ui::color_picker::PaletteDirection::Down,
                    ),
                    KeyCode::Char(KEY_THEME_VIEW) => self.toggle_theme_picker_display(),
                    KeyCode::Enter => self.apply_theme_picker_selection(),
                    _ => {},
                }
                return;
            }
            if key.code == KeyCode::Char(KEY_THEME_PALETTE) {
                self.theme_mode = ThemeMode::Palette;
                self.theme_palette_selected = crate::components::ui::color_picker::nearest_index(
                    self.theme_config.color(),
                    self.theme_picker_display,
                );
                return;
            }
            if key.code == KeyCode::Char(KEY_THEME_PRESET) {
                self.theme_config.preset = self.theme_config.preset.next();
                self.theme_config.accent = None;
                self.sync_theme_picker_selection();
                return;
            }
            if key.code == KeyCode::Char(KEY_THEME_RGB) {
                self.theme_mode = if self.theme_mode == ThemeMode::EditingRgb {
                    ThemeMode::Idle
                } else {
                    ThemeMode::EditingRgb
                };
                return;
            }
            if self.theme_mode == ThemeMode::EditingRgb {
                match key.code {
                    KeyCode::Tab => {
                        self.theme_rgb_channel = (self.theme_rgb_channel + RGB_CHANNEL_INDEX_STEP)
                            % super::theme_config::RGB_CHANNEL_COUNT;
                    },
                    KeyCode::Up | KeyCode::Right => self.adjust_theme_rgb(RGB_CHANNEL_STEP),
                    KeyCode::Down | KeyCode::Left => self.adjust_theme_rgb(-RGB_CHANNEL_STEP),
                    KeyCode::Esc => self.theme_mode = ThemeMode::Idle,
                    _ => {},
                }
                return;
            }
            match key.code {
                KeyCode::Esc => {
                    self.theme_mode = ThemeMode::Idle;
                    self.author_overlay.close();
                },
                KeyCode::Up | KeyCode::Char('k') => {
                    self.profile_scroll = self.profile_scroll.saturating_sub(1);
                },
                KeyCode::Down | KeyCode::Char('j') => {
                    self.profile_scroll = self.profile_scroll.saturating_add(1);
                },
                KeyCode::PageUp => self.profile_scroll = self.profile_scroll.saturating_sub(10),
                KeyCode::PageDown => self.profile_scroll = self.profile_scroll.saturating_add(10),
                KeyCode::Home => self.profile_scroll = 0,
                KeyCode::End => self.profile_scroll = usize::MAX,
                _ => {},
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
            KeyCode::Char('@') => self.open_command_log(),
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
            KeyCode::Char(' ') if self.focus == Pane::Stash => self.restore_selected_stash(false),
            KeyCode::Char('g') if self.focus == Pane::Stash => self.restore_selected_stash(true),
            KeyCode::Char('d') if self.focus == Pane::Stash => self.drop_stash_prompt(),
            KeyCode::Char('s') if self.focus == Pane::Files => self.open_stash_popup(),
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

    fn request_author_selection(
        &mut self,
        identity: Option<crate::domain::profile::settings::Identity>,
    ) {
        let active =
            self.selected_author
                .as_ref()
                .or(self.profile.settings.effective_identity.as_ref());
        if active == identity.as_ref() {
            return;
        }
        let message = if identity.is_some() {
            let name = identity.as_ref().map_or("", |author| author.name.as_str());
            format!("{AUTHOR_CHANGE_CONFIRM_MESSAGE}\nSelected: {name}")
        } else {
            AUTHOR_RESET_CONFIRM_MESSAGE.to_owned()
        };
        self.pending_confirm = Some(super::ConfirmPrompt {
            message,
            action: super::ConfirmAction::SelectAuthor(identity),
        });
    }

    fn save_theme(&mut self) {
        if self.theme_config == self.theme_saved_config {
            return;
        }
        match self.theme_config.save() {
            Ok(()) => {
                self.theme_saved_config = self.theme_config.clone();
                self.report_notice("Theme saved".to_owned());
            },
            Err(error) => self.report_notice(format!("Could not save theme settings: {error}")),
        }
    }

    fn move_theme_palette(
        &mut self,
        direction: crate::components::ui::color_picker::PaletteDirection,
    ) {
        self.theme_palette_selected = crate::components::ui::color_picker::move_selection(
            self.theme_palette_selected,
            direction,
            self.theme_picker_display,
        );
        self.apply_theme_picker_selection();
    }

    fn toggle_theme_picker_display(&mut self) {
        use crate::components::ui::color_picker::ColorPickerDisplay;

        self.theme_picker_display = match self.theme_picker_display {
            ColorPickerDisplay::Palette => ColorPickerDisplay::Spectrum,
            ColorPickerDisplay::Spectrum => ColorPickerDisplay::Palette,
        };
        self.sync_theme_picker_selection();
    }

    fn sync_theme_picker_selection(&mut self) {
        self.theme_palette_selected = crate::components::ui::color_picker::nearest_index(
            self.theme_config.color(),
            self.theme_picker_display,
        );
    }

    fn apply_theme_picker_selection(&mut self) {
        if let Some(color) = crate::components::ui::color_picker::color_at(
            self.theme_picker_display,
            self.theme_palette_selected,
        ) {
            self.theme_config.accent = Some(color);
        }
    }

    fn select_theme_picker_cell(&mut self, column: usize, row: usize) {
        if let Some(selected) = crate::components::ui::color_picker::selection_at(
            self.theme_picker_display,
            column,
            row,
        ) {
            self.theme_mode = ThemeMode::Palette;
            self.theme_palette_selected = selected;
            self.apply_theme_picker_selection();
        }
    }

    fn adjust_theme_rgb(&mut self, delta: i16) {
        let (mut r, mut g, mut b) = match self.theme_config.color() {
            ratatui::style::Color::Rgb(r, g, b) => (r, g, b),
            ratatui::style::Color::Green => (0, 255, 0),
            ratatui::style::Color::Cyan => (0, 255, 255),
            ratatui::style::Color::Magenta => (255, 0, 255),
            ratatui::style::Color::Yellow => (255, 255, 0),
            _ => (0, 255, 0),
        };
        let channel = match self.theme_rgb_channel {
            super::theme_config::RGB_RED_CHANNEL => &mut r,
            super::theme_config::RGB_GREEN_CHANNEL => &mut g,
            _ => &mut b,
        };
        *channel = u8::try_from((i16::from(*channel) + delta).clamp(RGB_MIN_VALUE, RGB_MAX_VALUE))
            .unwrap_or_default();
        self.theme_config.accent = Some(ratatui::style::Color::Rgb(r, g, b));
        self.sync_theme_picker_selection();
    }

    /// A left click focuses the pane it lands in and, when it lands on a
    /// list row, moves that pane's selection cursor there too (lazygit's
    /// `HandleClick`, steps 3 / 4 / 5 / 7); on a Files directory row, it
    /// also toggles it collapsed/expanded, same as `Enter`. Any click
    /// dismisses the help overlay first. Right click, middle click, drag
    /// and move are no-ops for now.
    pub(super) fn on_mouse(&mut self, ev: MouseEvent) {
        if matches!(ev.kind, MouseEventKind::Moved) {
            let over_author = self.author_overlay.is_closed()
                && self
                    .author_click_area
                    .contains(Position::new(ev.column, ev.row));
            self.mouse_pointer.request(over_author);
            return;
        }

        if !self.author_overlay.is_closed() {
            self.mouse_pointer.request(false);
            if matches!(ev.kind, MouseEventKind::Down(MouseButton::Left)) {
                let point = Position::new(ev.column, ev.row);
                if self.profile_hit_areas.save_button.contains(point) {
                    self.save_theme();
                    return;
                }
                if let Some((identity_index, _)) = self
                    .profile_hit_areas
                    .author_cards
                    .iter()
                    .find(|(_, area)| area.contains(point))
                {
                    if let Some(identity) = self
                        .profile
                        .settings
                        .available_identities()
                        .get(*identity_index)
                    {
                        self.request_author_selection(Some(identity.clone()));
                    }
                    return;
                }
                let grid = self.profile_hit_areas.color_grid;
                if grid.contains(point) {
                    let metrics = crate::components::ui::color_picker::grid_metrics(
                        self.theme_picker_display,
                    );
                    let column = usize::from(ev.column.saturating_sub(grid.x)) / metrics.cell_width;
                    let visible_row = usize::from(ev.row.saturating_sub(grid.y));
                    let row = self.profile_hit_areas.color_grid_first_row + visible_row;
                    self.select_theme_picker_cell(column, row);
                    return;
                }
            }
            match ev.kind {
                MouseEventKind::ScrollUp => {
                    self.profile_scroll = self.profile_scroll.saturating_sub(3);
                },
                MouseEventKind::ScrollDown => {
                    self.profile_scroll = self.profile_scroll.saturating_add(3);
                },
                MouseEventKind::Down(MouseButton::Left)
                    if !self
                        .author_overlay
                        .overlay_rect()
                        .is_some_and(|rect| rect.contains(Position::new(ev.column, ev.row))) =>
                {
                    self.author_overlay.close();
                },
                _ => {},
            }
            return;
        }

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

        if self
            .author_click_area
            .contains(Position::new(ev.column, ev.row))
        {
            self.mouse_pointer.request(false);
            self.author_overlay.open();
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
