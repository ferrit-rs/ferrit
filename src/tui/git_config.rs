//! What the keys do in `App` for `git_config`: the glue between the interface, the git code and the app's state.

use crate::git::config::WriteScope;
use crate::tui::App;
use crate::tui::error::AppError;
use crate::tui::keymap::{Action, Context, KeyBinding};
use crate::tui::state::full_screens::FullScreen;
use crate::tui::state::git_config::GitConfigScreen;
use ratatui::crossterm::event::KeyCode;
use ratatui::crossterm::event::KeyEvent;
use ratatui::crossterm::event::MouseEvent;
use ratatui::crossterm::event::MouseEventKind;

const PAGE: isize = 10;
const WHEEL_ROWS: isize = 3;

impl App {
    /// The git config screen's state, for the screen that draws it and for tests.
    pub fn git_config(&self) -> &GitConfigScreen {
        &self.full_screens.git_config
    }

    /// Show the git config screen over the panes, with a fresh listing.
    pub fn open_git_config(&mut self) {
        if self.reread_git_config() {
            self.full_screens.git_config.filtering = false;
            self.full_screens.git_config.note = None;
            self.full_screens.active = FullScreen::GitConfig;
        }
    }

    /// The renderer's word on where the list starts: keep it.
    pub(crate) const fn set_git_config_offset(&mut self, offset: usize) {
        self.full_screens.git_config.offset = offset;
    }

    pub fn close_git_config(&mut self) {
        self.full_screens.active = FullScreen::None;
        self.full_screens.git_config.filtering = false;
        self.full_screens.git_config.set_filter(String::new());
    }

    /// `git config --list` again; `false` (after an error toast) when git
    /// could not answer or there is no repository.
    pub(crate) fn reread_git_config(&mut self) -> bool {
        let Some(repo) = &self.repo else {
            self.report_error(AppError::NoRepository);
            return false;
        };
        match repo.config() {
            Ok(view) => {
                self.full_screens.git_config.set_view(view);
                true
            },
            Err(e) => {
                self.report_error(e);
                false
            },
        }
    }

    /// Every key while the screen is up (after the popups, a pending
    /// confirmation and the help overlay, which own input before it).
    pub(crate) fn git_config_key(&mut self, key: KeyEvent) {
        if self.full_screens.git_config.filtering {
            self.git_config_filter_key(key);
            return;
        }
        // The key that opens the screen closes it, whatever it is bound to.
        let toggles = self
            .prefs
            .keymap
            .resolve(&[Context::Global], KeyBinding::from_event(key))
            == Some(Action::GitConfig);
        if toggles {
            self.close_git_config();
            return;
        }
        match key.code {
            // A kept filter goes first; the next `Esc` leaves.
            KeyCode::Esc if !self.full_screens.git_config.filter.is_empty() => {
                self.full_screens.git_config.set_filter(String::new());
            },
            KeyCode::Esc | KeyCode::Char('q') => self.close_git_config(),
            KeyCode::Char('j') | KeyCode::Down => self.full_screens.git_config.move_by(1),
            KeyCode::Char('k') | KeyCode::Up => self.full_screens.git_config.move_by(-1),
            KeyCode::PageDown => self.full_screens.git_config.move_by(PAGE),
            KeyCode::PageUp => self.full_screens.git_config.move_by(-PAGE),
            KeyCode::Home => self.full_screens.git_config.selected = 0,
            KeyCode::End => {
                self.full_screens.git_config.selected =
                    self.full_screens.git_config.rows.len().saturating_sub(1);
            },
            KeyCode::Char('/') => self.full_screens.git_config.filtering = true,
            KeyCode::Char('s') => self.toggle_git_config_scope(),
            KeyCode::Char('r') => {
                self.reread_git_config();
            },
            _ => {
                self.git_config_edit_key(key);
            },
        }
    }

    /// `Esc` clears the filter and leaves it, `Enter` leaves it as typed.
    fn git_config_filter_key(&mut self, key: KeyEvent) {
        let mut text = self.full_screens.git_config.filter.clone();
        match key.code {
            KeyCode::Esc => {
                self.full_screens.git_config.filtering = false;
                self.full_screens.git_config.set_filter(String::new());
                return;
            },
            KeyCode::Enter => {
                self.full_screens.git_config.filtering = false;
                return;
            },
            KeyCode::Backspace => {
                text.pop();
            },
            KeyCode::Char(c) => text.push(c),
            _ => return,
        }
        self.full_screens.git_config.set_filter(text);
    }

    fn toggle_git_config_scope(&mut self) {
        self.full_screens.git_config.scope = match self.full_screens.git_config.scope {
            WriteScope::Local => WriteScope::Global,
            WriteScope::Global | WriteScope::Worktree => WriteScope::Local,
        };
    }

    /// Only the wheel does anything: it moves the selection.
    pub(crate) fn git_config_mouse(&mut self, ev: MouseEvent) {
        match ev.kind {
            MouseEventKind::ScrollUp => self.full_screens.git_config.move_by(-WHEEL_ROWS),
            MouseEventKind::ScrollDown => self.full_screens.git_config.move_by(WHEEL_ROWS),
            _ => {},
        }
    }
}
