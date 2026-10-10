//! The git config screen: `keys`.

use crate::ui::components::git_config::catalog::{KeyType, lookup};
use crate::ui::components::git_config::edit::{
    ConfigOp, GlobalResume, PickTarget, scope_label, scope_name, scope_of, truthy,
};
use crate::ui::components::git_config::screen::GitConfigScreen;
use crate::ui::components::menu::open_name;
use crate::ui::components::menu::{MenuAction, MenuItem, MenuState, NameKind};
use crate::ui::components::popups::{ConfirmAction, ConfirmPrompt, Popup};
use crate::ui::error::AppError;
use crate::ui::event::Event;
use ferrit_domain::config::{Scope, ValueKind, WriteScope, display_value, is_secret_key};
use ferrit_domain::port::GitPort;
use ferrit_tui::widgets::text_input::TextInput;
use ratatui::crossterm::event::{KeyCode, KeyEvent, MouseEvent, MouseEventKind};

pub(crate) const PAGE: isize = 10;
pub(crate) const WHEEL_ROWS: isize = 3;

/// The git config screen with the parts of the app it needs: the repository to
/// read and write, and `events` for what else follows.
pub(crate) struct GitConfig<'a> {
    /// The screen's own state.
    pub(crate) screen: &'a mut GitConfigScreen,
    /// The repository, if there is one.
    pub(crate) repo: Option<&'a dyn GitPort>,
    /// What the screen asks of the rest of the app.
    pub(crate) events: Vec<Event>,
}

impl GitConfig<'_> {
    /// Show the git config screen over the panes, with a fresh listing.
    pub(crate) fn open(&mut self) {
        if self.reread() {
            self.screen.filtering = false;
            self.screen.note = None;
            self.events.push(Event::ShowGitConfig);
        }
    }

    pub(crate) fn close(&mut self) {
        self.events.push(Event::HideGitConfig);
        self.screen.filtering = false;
        self.screen.set_filter(String::new());
    }

    /// `git config --list` again; `false` (after an error toast) when git could
    /// not answer or there is no repository.
    pub(crate) fn reread(&mut self) -> bool {
        let Some(repo) = self.repo else {
            self.events.push(Event::Report(AppError::NoRepository));
            return false;
        };
        match repo.config() {
            Ok(view) => {
                self.screen.set_view(view);
                true
            },
            Err(e) => {
                self.events.push(Event::Report(e.into()));
                false
            },
        }
    }

    /// Every key while the screen is up (after the popups, a pending
    /// confirmation and the help overlay, which own input before it).
    /// `toggles` is whether the key is the one that opens the screen, which
    /// closes it whatever it is bound to.
    pub(crate) fn key(&mut self, key: KeyEvent, toggles: bool) {
        if self.screen.filtering {
            self.filter_key(key);
            return;
        }
        if toggles {
            self.close();
            return;
        }
        match key.code {
            // A kept filter goes first; the next `Esc` leaves.
            KeyCode::Esc if !self.screen.filter.is_empty() => {
                self.screen.set_filter(String::new());
            },
            KeyCode::Esc | KeyCode::Char('q') => self.close(),
            KeyCode::Char('j') | KeyCode::Down => self.screen.move_by(1),
            KeyCode::Char('k') | KeyCode::Up => self.screen.move_by(-1),
            KeyCode::PageDown => self.screen.move_by(PAGE),
            KeyCode::PageUp => self.screen.move_by(-PAGE),
            KeyCode::Home => self.screen.selected = 0,
            KeyCode::End => {
                self.screen.selected = self.screen.rows.len().saturating_sub(1);
            },
            KeyCode::Char('/') => self.screen.filtering = true,
            KeyCode::Char('s') => self.toggle_scope(),
            KeyCode::Char('r') => {
                self.reread();
            },
            _ => {
                self.edit_key(key);
            },
        }
    }

    /// `Esc` clears the filter and leaves it, `Enter` leaves it as typed.
    fn filter_key(&mut self, key: KeyEvent) {
        let mut text = self.screen.filter.clone();
        match key.code {
            KeyCode::Esc => {
                self.screen.filtering = false;
                self.screen.set_filter(String::new());
                return;
            },
            KeyCode::Enter => {
                self.screen.filtering = false;
                return;
            },
            KeyCode::Backspace => {
                text.pop();
            },
            KeyCode::Char(c) => text.push(c),
            _ => return,
        }
        self.screen.set_filter(text);
    }

    fn toggle_scope(&mut self) {
        self.screen.scope = match self.screen.scope {
            WriteScope::Local => WriteScope::Global,
            WriteScope::Global | WriteScope::Worktree => WriteScope::Local,
        };
    }

    /// Only the wheel does anything: it moves the selection.
    pub(crate) fn mouse(&mut self, ev: MouseEvent) {
        match ev.kind {
            MouseEventKind::ScrollUp => self.screen.move_by(-WHEEL_ROWS),
            MouseEventKind::ScrollDown => self.screen.move_by(WHEEL_ROWS),
            _ => {},
        }
    }
}

pub(crate) const BOOL_VALUES: &[&str] = &["true", "false"];

impl GitConfig<'_> {
    /// `e` / `Enter`: edit the selected value in the write scope.
    pub(crate) fn edit_value(&mut self) {
        let Some(row) = self.screen.selected_row().cloned() else {
            return;
        };
        let key = row.entry.key.clone();
        if let Some(reason) = git_config_read_only(&row.entry.key, row.entry.scope) {
            self.screen.note = Some(reason);
            return;
        }
        if self.ask_before_global_write(GlobalResume::Edit) {
            return;
        }
        let scope = self.screen.scope;
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
            self.screen.pick = Some(PickTarget {
                key,
                kind: kind.value_kind(),
                replacing,
                values,
            });
            self.events.push(Event::OpenPopup(Popup::Menu(MenuState {
                title,
                items,
                selected,
            })));
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
        self.events.push(open_name(
            NameKind::ConfigValue(ConfigOp::Set {
                key,
                value: String::new(),
                kind,
                replacing,
            }),
            title,
            input,
        ));
    }

    /// `Space`: flip a known boolean key.
    pub(crate) fn toggle_bool(&mut self) {
        let Some(row) = self.screen.selected_row().cloned() else {
            return;
        };
        let key = row.entry.key.clone();
        if lookup(&key).map(|k| k.kind) != Some(KeyType::Bool) {
            self.screen.note = Some(format!("{key} is not a boolean: Enter edits it"));
            return;
        }
        if let Some(reason) = git_config_read_only(&key, row.entry.scope) {
            self.screen.note = Some(reason);
            return;
        }
        if self.ask_before_global_write(GlobalResume::Toggle) {
            return;
        }
        let scope = self.screen.scope;
        let replacing = self.replacing_in(scope, &key, row.entry.scope, &row.entry.value);
        let value = if truthy(&row.entry.value) {
            "false"
        } else {
            "true"
        };
        self.perform(&ConfigOp::Set {
            key,
            value: value.to_owned(),
            kind: ValueKind::Bool,
            replacing,
        });
    }

    /// `a`: ask for a key, then its value.
    pub(crate) fn add_key(&mut self) {
        if self.ask_before_global_write(GlobalResume::Add) {
            return;
        }
        let scope = self.screen.scope;
        self.events.push(open_name(
            NameKind::ConfigKey,
            format!("New key ({})", scope_name(scope)),
            TextInput::default(),
        ));
    }

    /// `d`: unset the selected value in the write scope, after asking.
    pub(crate) fn unset_value(&mut self) {
        let Some(row) = self.screen.selected_row().cloned() else {
            return;
        };
        let key = row.entry.key.clone();
        if let Some(reason) = git_config_read_only(&key, row.entry.scope) {
            self.screen.note = Some(reason);
            return;
        }
        let scope = self.screen.scope;
        if scope_of(row.entry.scope) != Some(scope) {
            self.screen.note = Some(format!(
                "{key} is not set in {}: press s to switch the scope",
                scope_name(scope)
            ));
            return;
        }
        if row.included {
            self.screen.note = Some(format!(
                "{key} comes from an included file: edit that file directly"
            ));
            return;
        }
        let shown = display_value(&key, &row.entry.value);
        let file = if scope == WriteScope::Global && !self.screen.global_confirmed {
            format!(" ({})", self.global_file_label())
        } else {
            String::new()
        };
        let value = (!row.entry.value.is_empty()).then(|| row.entry.value.clone());
        self.events.push(Event::Ask(ConfirmPrompt {
            message: format!("unset {key} = {shown} in {}{file}?", scope_name(scope)),
            action: ConfirmAction::ConfigUnset(ConfigOp::Unset { key, value }),
        }));
    }

    /// `y` on the unset question. Asking named the file, so for the global scope
    /// this is also the session's global confirmation.
    pub(crate) fn confirm_unset(&mut self, op: &ConfigOp) {
        if self.screen.scope == WriteScope::Global {
            self.screen.global_confirmed = true;
        }
        self.perform(op);
    }

    /// The first write to the global file of a session asks once, naming the
    /// file. `true` when it asked (the caller stops; `resume` carries on after
    /// the yes).
    fn ask_before_global_write(&mut self, resume: GlobalResume) -> bool {
        if self.screen.scope != WriteScope::Global || self.screen.global_confirmed {
            return false;
        }
        let message = format!(
            "write to {}? Asked once this session.",
            self.global_file_label()
        );
        self.events.push(Event::Ask(ConfirmPrompt {
            message,
            action: ConfirmAction::ConfigGlobal(resume),
        }));
        true
    }

    /// `y` on the global question: remember it, then do what was asked.
    pub(crate) fn resume(&mut self, resume: GlobalResume) {
        self.screen.global_confirmed = true;
        match resume {
            GlobalResume::Edit => self.edit_value(),
            GlobalResume::Toggle => self.toggle_bool(),
            GlobalResume::Add => self.add_key(),
        }
    }

    /// The global file as the user would write it: `~/.gitconfig`.
    fn global_file_label(&self) -> String {
        let Some(path) = self.screen.global_file() else {
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
    pub(crate) fn pick_value(&mut self, index: usize) {
        let Some(target) = self.screen.pick.take() else {
            return;
        };
        let Some(value) = target.values.get(index) else {
            return;
        };
        self.perform(&ConfigOp::Set {
            key: target.key,
            value: (*value).to_owned(),
            kind: target.kind,
            replacing: target.replacing,
        });
    }

    /// `Enter` in a config popup. `true` when the popup should close.
    pub(crate) fn submit_name(&mut self, kind: &NameKind, text: &str) -> bool {
        match kind {
            NameKind::ConfigKey => {
                let key = text.trim().to_owned();
                if key.is_empty() {
                    self.events
                        .push(Event::Notice("a key needs a name".to_owned()));
                    return false;
                }
                let scope = self.screen.scope;
                let exists = self.screen.rows.iter().any(|r| {
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
                self.events.push(open_name(
                    NameKind::ConfigValue(op),
                    format!("{key} ({})", scope_name(scope)),
                    TextInput::default(),
                ));
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
                self.perform(&op)
            },
            _ => true,
        }
    }

    /// The value to change when `key` holds several in `target`: the selected one,
    /// if it lives there.
    fn replacing_in(
        &self,
        target: WriteScope,
        key: &str,
        row_scope: Scope,
        value: &str,
    ) -> Option<String> {
        let held = self
            .screen
            .rows
            .iter()
            .filter(|r| r.entry.key == key && scope_of(r.entry.scope) == Some(target))
            .count();
        (held > 1 && scope_of(row_scope) == Some(target)).then(|| value.to_owned())
    }

    /// Run one change, re-read, and say what happened. `false` when git refused
    /// (its message is in the toast and nothing changed).
    pub(crate) fn perform(&mut self, op: &ConfigOp) -> bool {
        let Some(repo) = self.repo else {
            return false;
        };
        let scope = self.screen.scope;
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
            self.events.push(Event::Report(e.into()));
            return false;
        }
        let (ConfigOp::Set { key, .. } | ConfigOp::Add { key, .. } | ConfigOp::Unset { key, .. }) =
            op;
        self.reread();
        let note = match op {
            ConfigOp::Unset { .. } => {
                let wins = self.screen.effective(key).map(|e| {
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
        self.screen.note = Some(note);
        self.events.push(Event::Refresh);
        true
    }

    /// Keys of the config screen's edit actions, called from `key`.
    pub(crate) fn edit_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Char('e') | KeyCode::Enter => self.edit_value(),
            KeyCode::Char(' ') => self.toggle_bool(),
            KeyCode::Char('a') => self.add_key(),
            KeyCode::Char('d') => self.unset_value(),
            _ => return false,
        }
        true
    }
}

/// Why this value cannot be edited here, if it cannot.
pub(crate) fn git_config_read_only(key: &str, scope: Scope) -> Option<String> {
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
