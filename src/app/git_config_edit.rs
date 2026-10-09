//! What the keys do in `App` for `git_config_edit`: the glue between the interface, the git code and the app's state.

use crate::app::App;
use crate::app::state::confirm::{ConfirmAction, ConfirmPrompt};
use crate::app::state::context_menu::NameKind;
use crate::app::state::menu::{MenuAction, MenuItem, MenuState};
use crate::app::state::popup::Popup;
use crate::git::config::{Scope, ValueKind, WriteScope, display_value, is_secret_key};
use crate::git::config_edit::ConfigOp;
use crate::git::config_edit::GlobalResume;
use crate::git::config_edit::PickTarget;
use crate::git::config_edit::scope_label;
use crate::git::config_edit::scope_name;
use crate::git::config_edit::scope_of;
use crate::git::config_edit::truthy;
use crate::git::config_keys::{KeyType, lookup};
use crate::ui::widgets::text_input::TextInput;
use ratatui::crossterm::event::KeyCode;
use ratatui::crossterm::event::KeyEvent;

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
