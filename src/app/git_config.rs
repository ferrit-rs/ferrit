//! The git config screen's state and keys (`docs/PLAN_14_GIT_CONFIG.md`, G3):
//! the cached listing, the filter, the selection and the scope writes go to.
//! Drawing is `screens/git_config.rs`; every read and write is `Repo::config*`,
//! so git stays the owner of the file format.

use super::{App, FullScreen, KeyCode, KeyEvent, MouseEvent, MouseEventKind};
use crate::domain::git::config::{ConfigEntry, ConfigView, Scope, WriteScope, display_value};

/// Rows `PgUp` / `PgDn` move.
const PAGE: isize = 10;
/// Rows a wheel notch moves the selection.
const WHEEL_ROWS: isize = 3;

/// One listed value and what the listing says about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigRow {
    pub entry: ConfigEntry,
    /// Read from a file the scope's own file includes.
    pub included: bool,
    /// A higher scope sets the same key, so git does not use this value.
    pub shadowed: bool,
    /// The key is set at several scopes and this is the value git uses.
    pub winner: bool,
}

/// A row's identity across a re-read: key, scope and value.
type RowId = (String, Scope, String);

#[derive(Debug)]
pub struct GitConfigScreen {
    view: ConfigView,
    /// The listing after the filter, ordered by key (git's order within a key).
    pub rows: Vec<ConfigRow>,
    pub filter: String,
    /// Typing goes to the filter.
    pub filtering: bool,
    pub selected: usize,
    /// The file a write lands in: local or global, `s` flips it.
    pub scope: WriteScope,
    /// What the last action did, for the footer.
    pub note: Option<String>,
}

impl Default for GitConfigScreen {
    fn default() -> Self {
        Self {
            view: ConfigView::default(),
            rows: Vec::new(),
            filter: String::new(),
            filtering: false,
            selected: 0,
            scope: WriteScope::Local,
            note: None,
        }
    }
}

impl GitConfigScreen {
    /// Every value git knows, filter or not.
    pub fn total(&self) -> usize {
        self.view.entries.len()
    }

    pub fn selected_row(&self) -> Option<&ConfigRow> {
        self.rows.get(self.selected)
    }

    fn selected_id(&self) -> Option<RowId> {
        self.selected_row()
            .map(|r| (r.entry.key.clone(), r.entry.scope, r.entry.value.clone()))
    }

    /// Take a fresh listing and rebuild the rows, keeping the selection on the
    /// same value, else on the same key, else near where it was.
    pub(super) fn set_view(&mut self, view: ConfigView) {
        let before = self.selected_id();
        self.view = view;
        self.rebuild(before);
    }

    pub(super) fn set_filter(&mut self, filter: String) {
        let before = self.selected_id();
        self.filter = filter;
        self.rebuild(before);
    }

    fn rebuild(&mut self, before: Option<RowId>) {
        let needle = self.filter.to_lowercase();
        let mut order: Vec<&ConfigEntry> = self.view.entries.iter().collect();
        order.sort_by(|a, b| a.key.cmp(&b.key));
        self.rows = order
            .into_iter()
            .filter(|e| {
                needle.is_empty()
                    || e.key.to_lowercase().contains(&needle)
                    || display_value(&e.key, &e.value)
                        .to_lowercase()
                        .contains(&needle)
            })
            .map(|entry| {
                let same_key = self.view.entries.iter().filter(|e| e.key == entry.key);
                let shadowed = same_key.clone().any(|e| e.scope > entry.scope);
                let several_scopes = same_key.clone().any(|e| e.scope != entry.scope);
                ConfigRow {
                    entry: entry.clone(),
                    included: self.view.is_included(entry),
                    shadowed,
                    winner: several_scopes && !shadowed,
                }
            })
            .collect();
        let found = before.and_then(|(key, scope, value)| {
            self.rows
                .iter()
                .position(|r| {
                    r.entry.key == key && r.entry.scope == scope && r.entry.value == value
                })
                .or_else(|| self.rows.iter().position(|r| r.entry.key == key))
        });
        self.selected =
            found.unwrap_or_else(|| self.selected.min(self.rows.len().saturating_sub(1)));
    }

    fn move_by(&mut self, rows: isize) {
        let last = self.rows.len().saturating_sub(1);
        self.selected = self.selected.saturating_add_signed(rows).min(last);
    }
}

impl App {
    /// The git config screen's state, for the screen that draws it and for tests.
    pub fn git_config(&self) -> &GitConfigScreen {
        &self.git_config
    }

    /// Show the git config screen over the panes, with a fresh listing.
    pub fn open_git_config(&mut self) {
        if self.reread_git_config() {
            self.git_config.filtering = false;
            self.git_config.note = None;
            self.full_screen = FullScreen::GitConfig;
        }
    }

    pub fn close_git_config(&mut self) {
        self.full_screen = FullScreen::None;
        self.git_config.filtering = false;
        self.git_config.set_filter(String::new());
    }

    /// `git config --list` again; `false` (after an error toast) when git
    /// could not answer or there is no repository.
    pub(super) fn reread_git_config(&mut self) -> bool {
        let Some(repo) = &self.repo else {
            self.report_error("no repository: git config needs one");
            return false;
        };
        match repo.config() {
            Ok(view) => {
                self.git_config.set_view(view);
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
    pub(super) fn git_config_key(&mut self, key: KeyEvent) {
        if self.git_config.filtering {
            self.git_config_filter_key(key);
            return;
        }
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') => self.close_git_config(),
            KeyCode::Char('j') | KeyCode::Down => self.git_config.move_by(1),
            KeyCode::Char('k') | KeyCode::Up => self.git_config.move_by(-1),
            KeyCode::PageDown => self.git_config.move_by(PAGE),
            KeyCode::PageUp => self.git_config.move_by(-PAGE),
            KeyCode::Home => self.git_config.selected = 0,
            KeyCode::End => {
                self.git_config.selected = self.git_config.rows.len().saturating_sub(1);
            },
            KeyCode::Char('/') => self.git_config.filtering = true,
            KeyCode::Char('s') => self.toggle_git_config_scope(),
            KeyCode::Char('r') => {
                self.reread_git_config();
            },
            _ => {},
        }
    }

    /// `Esc` clears the filter and leaves it, `Enter` leaves it as typed.
    fn git_config_filter_key(&mut self, key: KeyEvent) {
        let mut text = self.git_config.filter.clone();
        match key.code {
            KeyCode::Esc => {
                self.git_config.filtering = false;
                self.git_config.set_filter(String::new());
                return;
            },
            KeyCode::Enter => {
                self.git_config.filtering = false;
                return;
            },
            KeyCode::Backspace => {
                text.pop();
            },
            KeyCode::Char(c) => text.push(c),
            _ => return,
        }
        self.git_config.set_filter(text);
    }

    fn toggle_git_config_scope(&mut self) {
        self.git_config.scope = match self.git_config.scope {
            WriteScope::Local => WriteScope::Global,
            WriteScope::Global | WriteScope::Worktree => WriteScope::Local,
        };
    }

    /// Only the wheel does anything: it moves the selection.
    pub(super) fn git_config_mouse(&mut self, ev: MouseEvent) {
        match ev.kind {
            MouseEventKind::ScrollUp => self.git_config.move_by(-WHEEL_ROWS),
            MouseEventKind::ScrollDown => self.git_config.move_by(WHEEL_ROWS),
            _ => {},
        }
    }
}
