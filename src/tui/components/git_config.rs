//! The git config screen: browse, edit, add, unset.

use crate::git::config::{
    ConfigEntry, ConfigView, Origin, Scope, ValueKind, WriteScope, display_value, is_secret_key,
};
use crate::git::config_edit::{
    ConfigOp, GlobalResume, PickTarget, scope_label, scope_name, scope_of, truthy,
};
use crate::git::config_keys::{KeyType, lookup};
use crate::theme::palette::Palette;
use crate::tui::App;
use crate::tui::components::menu::{MenuAction, MenuItem, MenuState, NameKind};
use crate::tui::components::popups::{ConfirmAction, ConfirmPrompt, Popup};
use crate::tui::draw::FullScreen;
use crate::tui::error::AppError;
use crate::tui::keymap::{Action, Context, KeyBinding};
use crate::tui::widgets::cut::cut_end;
use crate::tui::widgets::panel::Panel;
use crate::tui::widgets::scroll_bar::ScrollBar;
use crate::tui::widgets::text_input::TextInput;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent, MouseEvent, MouseEventKind};
use ratatui::layout::{Margin, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Clear, Paragraph};
use unicode_width::UnicodeWidthStr;

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

const BOOL_VALUES: &[&str] = &["true", "false"];

impl App {
    /// `e` / `Enter`: edit the selected value in the write scope.
    pub(crate) fn edit_git_config_value(&mut self) {
        let Some(row) = self.full_screens.git_config.selected_row().cloned() else {
            return;
        };
        let key = row.entry.key.clone();
        if let Some(reason) = Self::git_config_read_only(&row.entry.key, row.entry.scope) {
            self.full_screens.git_config.note = Some(reason);
            return;
        }
        if self.ask_before_global_write(GlobalResume::Edit) {
            return;
        }
        let scope = self.full_screens.git_config.scope;
        let replacing = self.replacing_in(scope, &row.entry.key, row.entry.scope, &row.entry.value);
        let known = lookup(&key).map(|k| k.kind);
        let title = format!("{key} ({})", scope_name(scope));
        if let Some(kind @ (KeyType::Bool | KeyType::Enum(_))) = known {
            let values = match kind {
                KeyType::Enum(values) => values,
                _ => BOOL_VALUES,
            };
            let items = values
                .iter()
                .zip('1'..)
                .map(|(label, shortcut)| MenuItem {
                    label,
                    shortcut,
                    action: MenuAction::ConfigValue(usize::from(shortcut as u8 - b'1')),
                    hint: "",
                })
                .collect();
            let selected = values
                .iter()
                .position(|v| *v == row.entry.value)
                .unwrap_or(0);
            self.full_screens.git_config.pick = Some(PickTarget {
                key,
                kind: kind.value_kind(),
                replacing,
                values,
            });
            self.modal.open_popup(Popup::Menu(MenuState {
                title,
                items,
                selected,
            }));
            return;
        }
        let secret = is_secret_key(&key);
        let kind = known.map_or(ValueKind::Text, KeyType::value_kind);
        let (title, input) = if secret {
            (
                format!("{title}: new value, the old one stays hidden"),
                TextInput::default(),
            )
        } else {
            (title, TextInput::from_text(&row.entry.value))
        };
        self.open_name(
            NameKind::ConfigValue(ConfigOp::Set {
                key,
                value: String::new(),
                kind,
                replacing,
            }),
            title,
            input,
        );
    }

    /// `Space`: flip a known boolean key.
    pub(crate) fn toggle_git_config_bool(&mut self) {
        let Some(row) = self.full_screens.git_config.selected_row().cloned() else {
            return;
        };
        let key = row.entry.key.clone();
        if lookup(&key).map(|k| k.kind) != Some(KeyType::Bool) {
            self.full_screens.git_config.note =
                Some(format!("{key} is not a boolean: Enter edits it"));
            return;
        }
        if let Some(reason) = Self::git_config_read_only(&key, row.entry.scope) {
            self.full_screens.git_config.note = Some(reason);
            return;
        }
        if self.ask_before_global_write(GlobalResume::Toggle) {
            return;
        }
        let scope = self.full_screens.git_config.scope;
        let replacing = self.replacing_in(scope, &key, row.entry.scope, &row.entry.value);
        let value = if truthy(&row.entry.value) {
            "false"
        } else {
            "true"
        };
        self.perform_git_config_op(&ConfigOp::Set {
            key,
            value: value.to_owned(),
            kind: ValueKind::Bool,
            replacing,
        });
    }

    /// `a`: ask for a key, then its value.
    pub(crate) fn add_git_config_key(&mut self) {
        if self.ask_before_global_write(GlobalResume::Add) {
            return;
        }
        let scope = self.full_screens.git_config.scope;
        self.open_name(
            NameKind::ConfigKey,
            format!("New key ({})", scope_name(scope)),
            TextInput::default(),
        );
    }

    /// `d`: unset the selected value in the write scope, after asking.
    pub(crate) fn unset_git_config_value(&mut self) {
        let Some(row) = self.full_screens.git_config.selected_row().cloned() else {
            return;
        };
        let key = row.entry.key.clone();
        if let Some(reason) = Self::git_config_read_only(&key, row.entry.scope) {
            self.full_screens.git_config.note = Some(reason);
            return;
        }
        let scope = self.full_screens.git_config.scope;
        if scope_of(row.entry.scope) != Some(scope) {
            self.full_screens.git_config.note = Some(format!(
                "{key} is not set in {}: press s to switch the scope",
                scope_name(scope)
            ));
            return;
        }
        if row.included {
            self.full_screens.git_config.note = Some(format!(
                "{key} comes from an included file: edit that file directly"
            ));
            return;
        }
        let shown = display_value(&key, &row.entry.value);
        let file = if scope == WriteScope::Global && !self.full_screens.git_config.global_confirmed
        {
            format!(" ({})", self.global_file_label())
        } else {
            String::new()
        };
        let value = (!row.entry.value.is_empty()).then(|| row.entry.value.clone());
        self.modal.ask(ConfirmPrompt {
            message: format!("unset {key} = {shown} in {}{file}?", scope_name(scope)),
            action: ConfirmAction::ConfigUnset(ConfigOp::Unset { key, value }),
        });
    }

    /// `y` on the unset question. Asking named the file, so for the global
    /// scope this is also the session's global confirmation.
    pub(crate) fn confirm_git_config_unset(&mut self, op: &ConfigOp) {
        if self.full_screens.git_config.scope == WriteScope::Global {
            self.full_screens.git_config.global_confirmed = true;
        }
        self.perform_git_config_op(op);
    }

    /// The first write to the global file of a session asks once, naming the
    /// file. `true` when it asked (the caller stops; `resume_git_config_edit`
    /// carries on after the yes).
    fn ask_before_global_write(&mut self, resume: GlobalResume) -> bool {
        if self.full_screens.git_config.scope != WriteScope::Global
            || self.full_screens.git_config.global_confirmed
        {
            return false;
        }
        self.modal.ask(ConfirmPrompt {
            message: format!(
                "write to {}? Asked once this session.",
                self.global_file_label()
            ),
            action: ConfirmAction::ConfigGlobal(resume),
        });
        true
    }

    /// `y` on the global question: remember it, then do what was asked.
    pub(crate) fn resume_git_config_edit(&mut self, resume: GlobalResume) {
        self.full_screens.git_config.global_confirmed = true;
        match resume {
            GlobalResume::Edit => self.edit_git_config_value(),
            GlobalResume::Toggle => self.toggle_git_config_bool(),
            GlobalResume::Add => self.add_git_config_key(),
        }
    }

    /// The global file as the user would write it: `~/.gitconfig`.
    fn global_file_label(&self) -> String {
        let Some(path) = self.full_screens.git_config.global_file() else {
            return "~/.gitconfig".to_owned();
        };
        std::env::var_os("HOME")
            .and_then(|home| path.strip_prefix(home).ok())
            .map_or_else(
                || path.display().to_string(),
                |rest| format!("~/{}", rest.display()),
            )
    }

    /// A menu row of the allowed values was chosen.
    pub(crate) fn pick_config_value(&mut self, index: usize) {
        let Some(target) = self.full_screens.git_config.pick.take() else {
            return;
        };
        let Some(value) = target.values.get(index) else {
            return;
        };
        self.perform_git_config_op(&ConfigOp::Set {
            key: target.key,
            value: (*value).to_owned(),
            kind: target.kind,
            replacing: target.replacing,
        });
    }

    /// `Enter` in a config popup. `true` when the popup should close.
    pub(crate) fn submit_git_config_name(&mut self, kind: &NameKind, text: &str) -> bool {
        match kind {
            NameKind::ConfigKey => {
                let key = text.trim().to_owned();
                if key.is_empty() {
                    self.report_notice("a key needs a name");
                    return false;
                }
                let scope = self.full_screens.git_config.scope;
                let exists = self.full_screens.git_config.rows.iter().any(|r| {
                    r.entry.key.eq_ignore_ascii_case(&key) && scope_of(r.entry.scope) == Some(scope)
                });
                let known = lookup(&key).map(|k| k.kind);
                let value_kind = known.map_or(ValueKind::Text, KeyType::value_kind);
                let op = if exists {
                    ConfigOp::Add {
                        key: key.clone(),
                        value: String::new(),
                        kind: value_kind,
                    }
                } else {
                    ConfigOp::Set {
                        key: key.clone(),
                        value: String::new(),
                        kind: value_kind,
                        replacing: None,
                    }
                };
                self.open_name(
                    NameKind::ConfigValue(op),
                    format!("{key} ({})", scope_name(scope)),
                    TextInput::default(),
                );
                false
            },
            NameKind::ConfigValue(op) => {
                let op = match op.clone() {
                    ConfigOp::Set {
                        key,
                        kind,
                        replacing,
                        ..
                    } => ConfigOp::Set {
                        key,
                        value: text.to_owned(),
                        kind,
                        replacing,
                    },
                    ConfigOp::Add { key, kind, .. } => ConfigOp::Add {
                        key,
                        value: text.to_owned(),
                        kind,
                    },
                    // Never typed into a popup: it has its own question.
                    unset @ ConfigOp::Unset { .. } => unset,
                };
                self.perform_git_config_op(&op)
            },
            _ => true,
        }
    }

    /// Why this value cannot be edited here, if it cannot.
    fn git_config_read_only(key: &str, scope: Scope) -> Option<String> {
        let lower = key.to_ascii_lowercase();
        if lower == "include.path" || lower.starts_with("includeif.") {
            return Some("an include: edit that file directly".to_owned());
        }
        match scope {
            Scope::System => Some("system, read-only".to_owned()),
            Scope::Command => Some("set on the command line, read-only".to_owned()),
            Scope::Global | Scope::Local | Scope::Worktree => None,
        }
    }

    /// The value to change when `key` holds several in `target`: the selected
    /// one, if it lives there.
    fn replacing_in(
        &self,
        target: WriteScope,
        key: &str,
        row_scope: Scope,
        value: &str,
    ) -> Option<String> {
        let held = self
            .full_screens
            .git_config
            .rows
            .iter()
            .filter(|r| r.entry.key == key && scope_of(r.entry.scope) == Some(target))
            .count();
        (held > 1 && scope_of(row_scope) == Some(target)).then(|| value.to_owned())
    }

    /// Run one change, re-read, and say what happened. `false` when git refused
    /// (its message is in the toast and nothing changed).
    pub(crate) fn perform_git_config_op(&mut self, op: &ConfigOp) -> bool {
        let Some(repo) = &self.repo else {
            return false;
        };
        let scope = self.full_screens.git_config.scope;
        let result = match op {
            ConfigOp::Set {
                key,
                value,
                kind,
                replacing: Some(old),
            } => repo.config_replace_value(scope, key, value, old, *kind),
            ConfigOp::Set {
                key,
                value,
                kind,
                replacing: None,
            } => repo.config_set(scope, key, value, *kind),
            ConfigOp::Add { key, value, kind } => repo.config_add(scope, key, value, *kind),
            ConfigOp::Unset {
                key,
                value: Some(old),
            } => repo.config_unset_value(scope, key, old),
            ConfigOp::Unset { key, value: None } => repo.config_unset(scope, key),
        };
        if let Err(e) = result {
            self.report_error(e);
            return false;
        }
        let (ConfigOp::Set { key, .. } | ConfigOp::Add { key, .. } | ConfigOp::Unset { key, .. }) =
            op;
        self.reread_git_config();
        let note = match op {
            ConfigOp::Unset { .. } => {
                let wins = self.full_screens.git_config.effective(key).map(|e| {
                    format!(
                        "{} value {} now wins",
                        scope_label(e.scope),
                        display_value(key, &e.value)
                    )
                });
                format!(
                    "unset {key} in {}; {}",
                    scope_name(scope),
                    wins.unwrap_or_else(|| "no value left".to_owned())
                )
            },
            ConfigOp::Set { .. } | ConfigOp::Add { .. } => {
                format!("{key} changed in {}", scope_name(scope))
            },
        };
        self.full_screens.git_config.note = Some(note);
        self.request_refresh();
        true
    }

    /// Keys of the config screen's edit actions, called from `git_config_key`.
    pub(crate) fn git_config_edit_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Char('e') | KeyCode::Enter => self.edit_git_config_value(),
            KeyCode::Char(' ') => self.toggle_git_config_bool(),
            KeyCode::Char('a') => self.add_git_config_key(),
            KeyCode::Char('d') => self.unset_git_config_value(),
            _ => return false,
        }
        true
    }
}

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
    pub(crate) view: ConfigView,
    /// The listing after the filter, ordered by key (git's order within a key).
    pub rows: Vec<ConfigRow>,
    pub filter: String,
    /// Typing goes to the filter.
    pub filtering: bool,
    pub selected: usize,
    /// The first list item of the last frame, kept so the view scrolls only
    /// when the selection leaves it.
    pub(crate) offset: usize,
    /// The file a write lands in: local or global, `s` flips it.
    pub scope: WriteScope,
    /// What the last action did, for the footer.
    pub note: Option<String>,
    /// The allowed-values menu that is up, and what choosing a row sets.
    pub(crate) pick: Option<PickTarget>,
    /// The first write to the global file was confirmed: it is asked once a session.
    pub(crate) global_confirmed: bool,
}

impl Default for GitConfigScreen {
    fn default() -> Self {
        Self {
            view: ConfigView::default(),
            rows: Vec::new(),
            filter: String::new(),
            filtering: false,
            selected: 0,
            offset: 0,
            scope: WriteScope::Local,
            note: None,
            pick: None,
            global_confirmed: false,
        }
    }
}

impl GitConfigScreen {
    /// Every value git knows, filter or not.
    pub fn total(&self) -> usize {
        self.view.entries.len()
    }

    pub const fn offset(&self) -> usize {
        self.offset
    }

    /// The value git uses for `key`.
    pub fn effective(&self, key: &str) -> Option<&ConfigEntry> {
        self.view.effective(key)
    }

    /// The file the global scope reads and writes, as the listing shows it.
    pub(crate) fn global_file(&self) -> Option<&std::path::Path> {
        self.view
            .entries
            .iter()
            .find(|e| e.scope == Scope::Global)
            .and_then(|e| match &e.origin {
                Origin::File(path) => Some(path.as_path()),
                Origin::CommandLine | Origin::Other(_) => None,
            })
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
    pub(crate) fn set_view(&mut self, view: ConfigView) {
        let before = self.selected_id();
        self.view = view;
        self.rebuild(before, false);
    }

    pub(crate) fn set_filter(&mut self, filter: String) {
        let before = self.selected_id();
        self.filter = filter;
        self.rebuild(before, true);
    }

    /// `top`: when the selected value is gone from the rows, go to the first
    /// one (a new filter) instead of staying near where it was (a re-read).
    fn rebuild(&mut self, before: Option<RowId>, top: bool) {
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
        self.selected = found.unwrap_or_else(|| {
            if top {
                0
            } else {
                self.selected.min(self.rows.len().saturating_sub(1))
            }
        });
    }

    pub(crate) fn move_by(&mut self, rows: isize) {
        let last = self.rows.len().saturating_sub(1);
        self.selected = self.selected.saturating_add_signed(rows).min(last);
    }
}

/// Full markers from this width.
pub(crate) const WIDE: u16 = 100;
/// Compact markers from this width; under it none.
pub(crate) const NARROW: u16 = 60;
/// Cells the full marker may take.
const MARKER_WIDE: usize = 26;
/// Cells the compact marker may take.
const MARKER_COMPACT: usize = 6;
/// Longest key column.
const KEY_MAX: usize = 40;
/// Shortest key column worth showing.
const KEY_MIN: usize = 12;

/// What the screen draws, all of it given.
#[derive(Debug)]
pub(crate) struct View<'a> {
    pub rows: &'a [ConfigRow],
    pub selected: usize,
    /// The first item (a row or a section rule) of the last frame.
    pub offset: usize,
    pub scope: WriteScope,
    /// Values git knows, filter or not.
    pub total: usize,
    pub filter: &'a str,
    /// Typing goes to the filter: it shows a caret.
    pub filtering: bool,
    /// What the last action did.
    pub note: Option<&'a str>,
    pub palette: Palette,
}

/// A line of the list: a section rule or the row at this index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Item<'a> {
    Rule(&'a str),
    Row(usize),
}

/// The part of a key before its first dot: `core` for `core.editor`.
fn section(key: &str) -> &str {
    key.split_once('.').map_or(key, |(section, _)| section)
}

fn items(rows: &[ConfigRow]) -> Vec<Item<'_>> {
    let mut out = Vec::with_capacity(rows.len() * 2);
    let mut last = None;
    for (i, row) in rows.iter().enumerate() {
        let name = section(&row.entry.key);
        if last != Some(name) {
            out.push(Item::Rule(name));
            last = Some(name);
        }
        out.push(Item::Row(i));
    }
    out
}

const fn scope_letter(scope: Scope) -> char {
    match scope {
        Scope::System => 'S',
        Scope::Global => 'G',
        Scope::Local => 'L',
        Scope::Worktree => 'W',
        Scope::Command => 'C',
    }
}

const fn scope_word(scope: Scope) -> &'static str {
    match scope {
        Scope::System => "system",
        Scope::Global => "global",
        Scope::Local => "local",
        Scope::Worktree => "worktree",
        Scope::Command => "command",
    }
}

/// What the marker column says about a row.
fn marker(row: &ConfigRow, full: bool) -> String {
    let mut parts = Vec::new();
    if row.winner {
        parts.push(if full {
            format!("← wins ({})", scope_word(row.entry.scope))
        } else {
            "←".to_owned()
        });
    }
    if row.included {
        parts.push(if full { "inherited" } else { "inh" }.to_owned());
    }
    match row.entry.scope {
        Scope::System | Scope::Command => {
            parts.push(if full {
                format!("{}, r/o", scope_word(row.entry.scope))
            } else {
                "r/o".to_owned()
            });
        },
        Scope::Global | Scope::Local | Scope::Worktree => {},
    }
    parts.join(", ")
}

/// The row's own colours: a local override green, another winner yellow, a
/// shadowed value dim.
fn row_style(row: &ConfigRow, palette: &Palette) -> Style {
    if row.shadowed {
        Style::new().add_modifier(Modifier::DIM)
    } else if row.winner && row.entry.scope == Scope::Local {
        Style::new().fg(palette.add)
    } else if row.winner {
        Style::new().fg(palette.warn)
    } else {
        Style::new()
    }
}

fn pad(text: &str, width: usize) -> String {
    let cut = cut_end(text, width);
    let used = UnicodeWidthStr::width(cut.as_str());
    format!("{cut}{}", " ".repeat(width.saturating_sub(used)))
}

/// `── core ─────…` to `width` cells.
fn rule(name: &str, width: usize) -> String {
    let head = format!("── {name} ");
    let used = UnicodeWidthStr::width(head.as_str());
    format!("{head}{}", "─".repeat(width.saturating_sub(used)))
}

fn row_line(row: &ConfigRow, selected: bool, columns: Columns, palette: &Palette) -> Line<'static> {
    let key = pad(&row.entry.key, columns.key);
    let shown = display_value(&row.entry.key, &row.entry.value).replace('\n', "⏎");
    let (value, empty) = if shown.is_empty() {
        ("(empty)".to_owned(), true)
    } else {
        (shown, false)
    };
    let value = pad(&value, columns.value);
    let marker = pad(&marker(row, columns.full), columns.marker);
    let base = row_style(row, palette);
    let text = format!(
        "{} {key} {value}{}{marker}",
        scope_letter(row.entry.scope),
        if columns.marker == 0 { "" } else { " " },
    );
    if selected {
        let style = Style::new().bg(palette.selection).fg(palette.selection_fg);
        return Line::styled(pad(&text, columns.width), style);
    }
    let dim = Style::new().add_modifier(Modifier::DIM);
    let mut spans = vec![
        Span::styled(format!("{} ", scope_letter(row.entry.scope)), base),
        Span::styled(format!("{key} "), base),
        Span::styled(value, if empty { dim } else { base }),
    ];
    if columns.marker > 0 {
        spans.push(Span::styled(
            format!(" {marker}"),
            if row.shadowed { dim } else { base },
        ));
    }
    Line::from(spans)
}

/// Cell widths of the columns for one frame.
#[derive(Debug, Clone, Copy)]
struct Columns {
    width: usize,
    key: usize,
    value: usize,
    marker: usize,
    full: bool,
}

fn columns(rows: &[ConfigRow], width: u16) -> Columns {
    let total = usize::from(width);
    let marker = if width >= WIDE {
        MARKER_WIDE
    } else if width >= NARROW {
        MARKER_COMPACT
    } else {
        0
    };
    let longest = rows
        .iter()
        .map(|r| UnicodeWidthStr::width(r.entry.key.as_str()))
        .max()
        .unwrap_or(0);
    let gaps = 2 + 1 + usize::from(marker > 0);
    let room = total.saturating_sub(gaps + marker);
    let key = longest.clamp(KEY_MIN, KEY_MAX).min(room / 2).max(1);
    Columns {
        width: total,
        key,
        value: room.saturating_sub(key),
        marker,
        full: width >= WIDE,
    }
}

/// The panel's title: `Git config ─ scope for changes: [L]ocal ─ 41 keys ─ filter: pull`.
fn title(view: &View<'_>) -> Line<'static> {
    let accent = Style::new().fg(view.palette.focus);
    let dim = Style::new().add_modifier(Modifier::DIM);
    let scope = match view.scope {
        WriteScope::Local => "[L]ocal",
        WriteScope::Global => "[G]lobal",
        WriteScope::Worktree => "[W]orktree",
    };
    let count = if view.filter.is_empty() {
        format!("{} keys", view.total)
    } else {
        format!("{} of {} keys", view.rows.len(), view.total)
    };
    let mut spans = vec![
        Span::styled(" Git config ", accent.add_modifier(Modifier::BOLD)),
        Span::styled("─ scope for changes: ", dim),
        Span::styled(scope, Style::new().fg(view.palette.key)),
        Span::styled(format!(" ─ {count}"), dim),
    ];
    if view.filtering || !view.filter.is_empty() {
        spans.push(Span::styled(" ─ filter: ", dim));
        spans.push(Span::raw(view.filter.to_owned()));
        if view.filtering {
            spans.push(Span::styled("▏", accent));
        }
    }
    spans.push(Span::raw(" "));
    Line::from(spans)
}

/// Keep the selected item in view, with its section rule when it is the first
/// of the section, and never scroll past the last item.
fn scrolled(items: &[Item<'_>], selected: usize, offset: usize, height: usize) -> usize {
    let at = items
        .iter()
        .position(|i| *i == Item::Row(selected))
        .unwrap_or(0);
    let top = if at > 0 && matches!(items.get(at - 1), Some(Item::Rule(_))) {
        at - 1
    } else {
        at
    };
    let mut offset = offset;
    if top < offset {
        offset = top;
    } else if at >= offset + height {
        offset = at + 1 - height;
    }
    offset.min(items.len().saturating_sub(height))
}

/// Draw the screen into `area`. Returns the offset it settled on, for the app
/// to keep.
pub(crate) fn draw(frame: &mut Frame<'_>, area: Rect, view: &View<'_>) -> usize {
    frame.render_widget(Clear, area);
    let mut panel = Panel::new()
        .title(title(view))
        .border_style(Style::new().fg(view.palette.focus));
    if let Some(note) = view.note {
        panel = panel.bottom_title(Line::styled(
            format!(" {note} "),
            Style::new().add_modifier(Modifier::DIM),
        ));
    }
    let block = panel.block();
    let inner = block.inner(area).inner(Margin::new(1, 0));
    frame.render_widget(block, area);
    if inner.width == 0 || inner.height == 0 {
        return view.offset;
    }

    if view.rows.is_empty() {
        let text = if view.filter.is_empty() {
            "no git config values".to_owned()
        } else {
            format!("no key or value matches \"{}\"", view.filter)
        };
        let line = Line::styled(
            cut_end(&text, usize::from(inner.width)),
            Style::new().add_modifier(Modifier::DIM),
        );
        frame.render_widget(Paragraph::new(line), inner);
        return 0;
    }

    let list = items(view.rows);
    let height = usize::from(inner.height);
    let offset = scrolled(&list, view.selected, view.offset, height);
    let cols = columns(view.rows, inner.width);
    let dim = Style::new().add_modifier(Modifier::DIM);
    let lines: Vec<Line<'static>> = list
        .iter()
        .skip(offset)
        .take(height)
        .filter_map(|item| match *item {
            Item::Rule(name) => Some(Line::styled(rule(name, cols.width), dim)),
            Item::Row(i) => view
                .rows
                .get(i)
                .map(|row| row_line(row, i == view.selected, cols, &view.palette)),
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), inner);
    ScrollBar::new(list.len(), height, offset)
        .style(Style::new().fg(view.palette.focus))
        .render(
            frame,
            Rect::new(area.right().saturating_sub(1), inner.y, 1, inner.height),
        );
    offset
}
