//! Routing a key or a click to the part of the app that owns it, and running an action.

use crate::git;
use crate::git::remote::RemoteOp;
use crate::tui::App;
use crate::tui::components::dashboard::Sheet;
use crate::tui::components::diff::right_pane::Mode;
use crate::tui::components::keybar::filter_help_lines;
use crate::tui::components::panes::nav::{PANES, Pane};
use crate::tui::components::popups::Popup;
use crate::tui::components::{
    branches, commits, files, menu, popups, remote as askpass, remote, stash, welcome,
};
use crate::tui::draw::FullScreen;
use crate::tui::event::Event;
use crate::tui::keymap::{Action, Context, KeyBinding};
use ratatui::crossterm::event::{
    KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::Position;

const KEY_CONFIRM_YES: char = 'y';
const KEY_CONFIRM_NO: char = 'n';
const LIST_WHEEL_ROWS: isize = 2;

impl App {
    pub(crate) fn on_key(&mut self, key: KeyEvent) {
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
        if self.modal.popup().is_some() {
            self.popup_key(key);
            return;
        }

        // A key-bar confirmation swallows every key but its own answer (Enter
        // or `y` confirms, so Enter never reaches the pane), same as the
        // help overlay below.
        if self.modal.confirm().is_some() {
            match key.code {
                KeyCode::Enter | KeyCode::Char(KEY_CONFIRM_YES | 'Y') => self.answer_yes(),
                KeyCode::Char(KEY_CONFIRM_NO | 'N') | KeyCode::Esc => {
                    self.modal.cancel_confirm();
                },
                _ => {},
            }
            self.update_right_pane();
            return;
        }

        if self.help.is_visible(&self.render.help) {
            if self.help.open {
                self.help_key(key);
            }
            return;
        }

        // A full-screen view owns the keys after the overlays above.
        if self.full_screens.active == FullScreen::GitConfig {
            let toggles = self
                .prefs
                .keymap
                .resolve(&[Context::Global], KeyBinding::from_event(key))
                == Some(Action::GitConfig);
            let mut config = self.git_config_ctx();
            config.key(key, toggles);
            let events = config.events;
            self.apply(events);
            return;
        }
        if self.full_screens.active == FullScreen::Welcome {
            let events = welcome::key(
                self.full_screens.welcome_selected,
                self.full_screens.welcome_dir.as_deref(),
                key,
            );
            self.apply(events);
            return;
        }

        if !self.render.sheet.is_closed() {
            match self.sheets.kind {
                Sheet::Settings => {
                    let mut settings = self.settings_ctx();
                    settings.key(key);
                    let events = settings.events;
                    self.apply(events);
                },
                Sheet::Dashboard => {
                    let toggles = self
                        .prefs
                        .keymap
                        .resolve(&[Context::Global], KeyBinding::from_event(key))
                        == Some(Action::Dashboard);
                    let events =
                        self.with_dashboard(|dashboard, ctx| dashboard.key(key, toggles, ctx));
                    self.apply(events);
                },
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
            self.help.dismiss(&mut self.render.help);
            return;
        }
        let total = filter_help_lines(&self.help_lines(), &self.help.query().text()).len();
        self.help.scroll_key(key.code, total);
    }

    /// A left click focuses the pane it lands in and, when it lands on a
    /// list row, moves that pane's selection cursor there too (lazygit's
    /// `HandleClick`, steps 3 / 4 / 5 / 7); on a Files directory row, it
    /// also toggles it collapsed/expanded, same as `Enter`. Any click
    /// dismisses the help overlay first, and a click on a keybar hint runs its
    /// action. A right click opens the row's `x` menu; middle click, drag and
    /// move are no-ops.
    pub(crate) fn on_mouse(&mut self, ev: MouseEvent) {
        // `[ui] mouse = false` never enables mouse capture; a terminal that
        // sends events anyway still gets no reaction from ferrit.
        if !self.prefs.config.ui.mouse {
            return;
        }
        if matches!(ev.kind, MouseEventKind::Moved) {
            let over_author = self.render.sheet.is_closed()
                && self.hits.author.contains(Position::new(ev.column, ev.row));
            let over_dashboard = self.render.sheet.is_closed()
                && self
                    .hits
                    .dashboard
                    .contains(Position::new(ev.column, ev.row));
            self.mouse_pointer.request(over_author || over_dashboard);
            return;
        }

        // The panes are not on screen: their areas from the last frame must not
        // answer clicks. The wheel scrolls the dashboard.
        if self.full_screens.active == FullScreen::GitConfig {
            {
                let mut config = self.git_config_ctx();
                config.mouse(ev);
            };
            return;
        }
        // Nothing to click without a repository.
        if self.full_screens.active == FullScreen::Welcome {
            return;
        }

        if !self.render.sheet.is_closed() {
            match self.sheets.kind {
                Sheet::Settings => {
                    let hits = self.hits.settings.clone();
                    let overlay = self.render.sheet.overlay_rect();
                    let mut settings = self.settings_ctx();
                    settings.mouse(ev, &hits, overlay);
                    let events = settings.events;
                    self.apply(events);
                },
                Sheet::Dashboard => {
                    let overlay = self.render.sheet.overlay_rect();
                    let events = self.sheets.dashboard.mouse(ev, overlay);
                    self.apply(events);
                },
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

        if self.help.is_visible(&self.render.help) {
            self.help.dismiss(&mut self.render.help); // any click dismisses the overlay
            return;
        }

        if !self.modal.is_some() && self.hits.keybar.contains(Position::new(ev.column, ev.row)) {
            let column = ev.column - self.hits.keybar.x;
            let clicked = self
                .hits
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

        if self.hits.author.contains(Position::new(ev.column, ev.row)) {
            self.mouse_pointer.request(false);
            self.open_sheet(Sheet::Settings);
            return;
        }

        if self
            .hits
            .dashboard
            .contains(Position::new(ev.column, ev.row))
        {
            self.mouse_pointer.request(false);
            self.open_dashboard();
            return;
        }

        if let Some(pane) = self.pane_at(ev.column, ev.row) {
            self.nav.right_focused = false; // a left click always returns focus left
            self.nav.mode = Mode::Nav; // a click is a Nav-mode gesture, not diff-cursor movement
            let landed = self.click_pane(pane, ev.row);
            // lazygit toggles a Files directory row on click, not just on
            // Enter — the whole row is the target, not just its arrow
            // glyph, same as it already is for plain selection.
            if landed && pane == Pane::Files {
                let events = crate::tui::components::panes::keys::toggle_files_dir(&self.env());
                self.apply(events);
            }
            self.update_right_pane(); // step 7: rebuild for the new focus/selection
        } else if self.right.area.contains(Position::new(ev.column, ev.row)) {
            self.nav.right_focused = true;
        }
        // else: command log / keybar / gap. no-op.
    }

    /// Which left pane a screen cell is in, `None` for the right pane, the
    /// command log, the keybar or an inter-pane gap.
    pub(crate) fn pane_at(&self, col: u16, row: u16) -> Option<Pane> {
        let point = Position::new(col, row);
        PANES
            .into_iter()
            .find(|&pane| self.hits.left[pane].contains(point))
    }

    /// Focus `pane`, then move its cursor to `screen_row` if that row maps
    /// to a real entry. Returns whether the cursor moved: `false` for the
    /// border / title row and for a click past the last entry.
    pub(crate) fn click_pane(&mut self, pane: Pane, screen_row: u16) -> bool {
        self.nav.focus = pane; // focus first, even on the border or past the tail
        let Some(idx) = self.click_row(pane, screen_row) else {
            return false;
        };
        self.nav.selection[pane] = idx;
        true
    }

    /// Screen row -> model index for a left pane. `None` for the border /
    /// title row, or a click past the last entry.
    pub(crate) fn click_row(&self, pane: Pane, screen_row: u16) -> Option<usize> {
        let area = self.hits.left[pane];
        let inner_row = screen_row.checked_sub(area.y.saturating_add(1))?;
        let idx = self.hits.list_offset[pane].saturating_add(usize::from(inner_row));
        (idx < self.row_count(pane)).then_some(idx)
    }

    /// Mouse wheel over the right column scrolls the diff (lazygit's "wheel
    /// over the main view"); over a left pane it scrolls that pane's list.
    pub(crate) fn wheel(&mut self, ev: MouseEvent, step: isize) {
        let a = self.right.area;
        let over_right = ev.column >= a.x && ev.column < a.x.saturating_add(a.width);
        if over_right && self.right.is_diff() {
            self.right
                .scroll_by(step * isize::from(self.prefs.config.ui.wheel_step));
            return;
        }
        // Over a left pane the wheel scrolls that pane's view, wherever the
        // focus is, and leaves the focus, the selection and the right pane
        // alone (lazygit). Anywhere else there is nothing to scroll.
        if let Some(pane) = self.pane_at(ev.column, ev.row) {
            self.hits
                .scroll_list(pane, self.nav.selection[pane], step * LIST_WHEEL_ROWS);
        }
    }

    pub(crate) fn pane_offset(&self, delta: usize) -> Pane {
        let idx = (self.nav.focus.index() + delta) % PANES.len();
        PANES.get(idx).copied().unwrap_or(self.nav.focus)
    }

    pub(crate) fn select_down(&mut self) {
        let last = self.row_count(self.nav.focus).saturating_sub(1);
        let cursor = &mut self.nav.selection[self.nav.focus];
        *cursor = (*cursor + 1).min(last);
    }

    pub(crate) fn select_up(&mut self) {
        let cursor = &mut self.nav.selection[self.nav.focus];
        *cursor = cursor.saturating_sub(1);
    }
}

impl App {
    /// The contexts a key is looked up in, most specific first.
    pub(crate) fn key_contexts(&self) -> Vec<Context> {
        self.scene().key_contexts()
    }

    /// Run the key's action, if it has one. Scrolling the right pane skips
    /// the preview rebuild (and its diff subprocess): it changes no selection.
    /// Every other key ends by re-syncing the preview, since focus or
    /// selection may have moved.
    pub(crate) fn dispatch_key(&mut self, key: KeyEvent) {
        let binding = KeyBinding::from_event(key);
        let action = self.prefs.keymap.resolve(&self.key_contexts(), binding);
        if let Some(action) = action {
            if is_scroll(action) && self.right.is_diff() {
                self.run_scroll(action);
                return;
            }
            self.run_action(action);
        }
        self.update_right_pane();
    }

    fn run_scroll(&mut self, action: Action) {
        let half = isize::try_from((self.right.viewport / 2).max(1)).unwrap_or(isize::MAX);
        let page =
            isize::try_from(self.right.viewport.saturating_sub(1).max(1)).unwrap_or(isize::MAX);
        match action {
            Action::ScrollHalfDown => self.right.scroll_by(half),
            Action::ScrollHalfUp => self.right.scroll_by(-half),
            Action::ScrollLineDown => self.right.scroll_by(1),
            Action::ScrollLineUp => self.right.scroll_by(-1),
            Action::ScrollPageDown => self.right.scroll_by(page),
            Action::ScrollPageUp => self.right.scroll_by(-page),
            Action::ScrollBottom => self.right.scroll_by(isize::MAX),
            Action::ScrollTop => self.right.scroll_by(isize::MIN),
            Action::NextHunk => self.right.jump_anchor(1),
            Action::PrevHunk => self.right.jump_anchor(-1),
            _ => {},
        }
    }

    pub(crate) fn run_action(&mut self, action: Action) {
        match action {
            Action::Quit => self.should_quit = true,
            Action::Help => self.open_help(),
            Action::CommandLog => self.apply(popups::open_command_log(&self.env())),
            Action::Dashboard => self.open_dashboard(),
            Action::GitConfig => self.open_git_config(),
            Action::CreateRemote => self.open_create_remote(),
            Action::OperationMenu => self.apply(menu::open_operation(&self.env())),
            Action::ContextMenu => self.apply(menu::open_context(&self.env())),
            Action::Back => self.go_back(),
            Action::Enter => self.enter_selected(),
            Action::EnterDiff => self.apply(files::enter_diff(&self.env())),
            Action::Refresh => self.request_refresh(),
            Action::Fetch => self.apply(remote::trigger(RemoteOp::Fetch)),
            Action::Pull => self.apply(remote::trigger(RemoteOp::Pull)),
            Action::Push => self.apply(remote::push(&self.env())),
            Action::Commit => self.open_commit(git::commit::CommitKind::Normal),
            Action::Amend => self.open_commit(git::commit::CommitKind::Amend),
            Action::RewordHead => self.open_commit(git::commit::CommitKind::Reword),
            Action::Focus(pane) => self.focus_pane(pane),
            Action::NextPane => self.focus_pane(self.pane_offset(1)),
            Action::PrevPane => self.focus_pane(self.pane_offset(PANES.len() - 1)),
            Action::ToggleBranchesTab => self.nav.toggle_branches_tab(),
            Action::SelectDown => self.select_down(),
            Action::SelectUp => self.select_up(),
            // Only meaningful over a real diff; anywhere else they do nothing.
            Action::ScrollLineDown
            | Action::ScrollLineUp
            | Action::ScrollPageDown
            | Action::ScrollPageUp
            | Action::ScrollHalfDown
            | Action::ScrollHalfUp
            | Action::ScrollTop
            | Action::ScrollBottom
            | Action::NextHunk
            | Action::PrevHunk => {},
            Action::StageFile => self.apply(files::stage_selected(&self.env())),
            Action::StageAll => self.apply(files::stage_all(&self.env())),
            Action::Discard => self.apply(files::discard_prompt(&self.env())),
            Action::StashPush => self.apply(stash::open_popup(&self.env())),
            Action::LeaveDiff => self.apply(vec![Event::LeaveDiff]),
            Action::CursorDown => self.right.move_cursor(1),
            Action::CursorUp => self.right.move_cursor(-1),
            Action::CursorNextHunk => self.right.jump_cursor_hunk(1),
            Action::CursorPrevHunk => self.right.jump_cursor_hunk(-1),
            Action::ToggleSelection => self.right.toggle_anchor(),
            Action::StageCursor => self.apply(files::stage_cursor(&self.env())),
            Action::Checkout => self.apply(branches::checkout(&self.env())),
            Action::NewBranch => self.apply(branches::open_new_popup(&self.env())),
            Action::FastForward => self.apply(branches::fast_forward(&self.env())),
            Action::Merge => self.apply(branches::merge(&self.env())),
            Action::DeleteBranch => self.apply(branches::delete_prompt(&self.env())),
            Action::RewordCommit => self.apply(commits::reword(&self.env())),
            Action::DropCommit => self.apply(commits::drop_prompt(&self.env())),
            Action::Squash => self.apply(commits::fold(&self.env(), false)),
            Action::Fixup => self.apply(commits::fold(&self.env(), true)),
            Action::EditCommit => self.apply(commits::edit(&self.env())),
            Action::NewFixup => self.apply(commits::new_fixup(&self.env())),
            Action::Autosquash => self.apply(commits::autosquash(&self.env())),
            Action::ApplyStash => self.apply(stash::restore_prompt(&self.env(), false)),
            Action::PopStash => self.apply(stash::restore_prompt(&self.env(), true)),
            Action::DropStash => self.apply(stash::drop_prompt(&self.env())),
        }
    }

    /// `Esc` on a pane: back out of a drilled branch or commit, restoring the
    /// cursor it was opened from.
    fn go_back(&mut self) {
        self.nav.right_focused = false;
        if let Some(drill) = self.nav.branch_drill.take() {
            self.nav.selection[Pane::Branches] = drill.return_index;
        }
        if let Some(drill) = self.nav.commit_drill.take() {
            self.nav.selection[Pane::Commits] = drill.return_index;
        }
    }

    /// `Enter` on a row: open a branch's log or a commit's files, else toggle
    /// a directory, else enter the diff.
    fn enter_selected(&mut self) {
        self.apply(branches::enter_log(&self.env()));
        // Opening a commit must not also toggle its first row.
        let opts = self.prefs.diff_opts();
        let (drilled, events) =
            crate::tui::components::panes::keys::enter_commit_files(&self.env(), opts);
        self.apply(events);
        if !drilled {
            self.apply(crate::tui::components::panes::keys::toggle_files_dir(
                &self.env(),
            ));
            self.apply(crate::tui::components::panes::keys::toggle_commit_dir(
                &self.env(),
            ));
            self.apply(files::enter_diff(&self.env()));
        }
    }

    fn focus_pane(&mut self, pane: Pane) {
        self.nav.right_focused = false;
        self.nav.focus = pane;
    }

    pub(crate) fn open_help(&mut self) {
        self.help.show();
    }
}

const fn is_scroll(action: Action) -> bool {
    matches!(
        action,
        Action::ScrollLineDown
            | Action::ScrollLineUp
            | Action::ScrollPageDown
            | Action::ScrollPageUp
            | Action::ScrollHalfDown
            | Action::ScrollHalfUp
            | Action::ScrollTop
            | Action::ScrollBottom
            | Action::NextHunk
            | Action::PrevHunk
    )
}

impl App {
    /// `y` while a question is up: run what it asked.
    fn answer_yes(&mut self) {
        let Some(prompt) = self.modal.take_confirm() else {
            return;
        };
        let events = popups::confirm(prompt.action, &self.env());
        self.apply(events);
    }
}

impl App {
    /// A right-click on a row: focus that pane, move its selection there, then
    /// open its menu. Off any row it does nothing. (A left click also toggles a
    /// directory; this one must not.)
    fn right_click(&mut self, column: u16, row: u16) {
        if self.modal.is_some() || self.help.open {
            return;
        }
        let Some(pane) = self.pane_at(column, row) else {
            return;
        };
        self.nav.right_focused = false;
        self.nav.mode = Mode::Nav;
        if self.click_pane(pane, row) {
            self.update_right_pane();
            self.apply(menu::open_context(&self.env()));
        }
    }
}

impl App {
    /// Every key while a popup is up: to the popup that has its own keys, else to
    /// the text box, note or command log.
    fn popup_key(&mut self, key: KeyEvent) {
        let events = match self.modal.popup_mut() {
            None => return,
            Some(Popup::Commit(_)) => return self.commit_popup_key(key),
            Some(Popup::CommitAllConfirm) => return self.commit_all_confirm_key(key),
            Some(Popup::CreateRemote(_)) => return self.create_remote_key(key),
            Some(Popup::Askpass(ask)) => askpass::key(ask, key),
            Some(Popup::Menu(menu)) => match menu::on_key(menu, key) {
                menu::MenuKey::Stay => Vec::new(),
                menu::MenuKey::Close => vec![Event::ClosePopup],
                menu::MenuKey::Choose(action) => {
                    let mut events = vec![Event::ClosePopup];
                    events.extend(menu::run_action(action, &self.env()));
                    events
                },
            },
            Some(popup) => popups::text_key(popup, key),
        };
        self.apply(events);
    }
}
