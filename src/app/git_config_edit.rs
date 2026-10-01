//! Changing a value on the git config screen (`docs/PLAN_14_GIT_CONFIG.md`,
//! G3): `e` / `Enter` edits the selected value (a text popup, or a menu of the
//! allowed values for a known boolean or enum), `Space` flips a boolean, `a`
//! adds a key. Every change is one `git config` call at the screen's write
//! scope (`Repo::config_*`); git validates the value.

use super::context_menu::NameKind;
use super::menu::{MenuAction, MenuItem, MenuState};
use super::{App, KeyCode, KeyEvent, Popup, TextInput};
use crate::domain::git::config::{Scope, ValueKind, WriteScope, is_secret_key};
use crate::domain::git::config_keys::{KeyType, lookup};

const BOOL_VALUES: &[&str] = &["true", "false"];

/// One change the screen can ask git for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum ConfigOp {
    /// Set `key` in the write scope. `replacing` names the one value to change
    /// when the key holds several there.
    Set {
        key: String,
        value: String,
        kind: ValueKind,
        replacing: Option<String>,
    },
    /// One more value for a key that already holds some in the write scope.
    Add {
        key: String,
        value: String,
        kind: ValueKind,
    },
}

/// What a menu row will set, remembered while the menu is up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct PickTarget {
    key: String,
    kind: ValueKind,
    replacing: Option<String>,
    values: &'static [&'static str],
}

const fn scope_name(scope: WriteScope) -> &'static str {
    match scope {
        WriteScope::Local => "local",
        WriteScope::Global => "global",
        WriteScope::Worktree => "worktree",
    }
}

fn truthy(value: &str) -> bool {
    value.is_empty()
        || matches!(
            value.to_ascii_lowercase().as_str(),
            "true" | "yes" | "on" | "1"
        )
}

impl App {
    /// `e` / `Enter`: edit the selected value in the write scope.
    pub(super) fn edit_git_config_value(&mut self) {
        let Some(row) = self.git_config.selected_row().cloned() else {
            return;
        };
        let key = row.entry.key.clone();
        if let Some(reason) = self.git_config_read_only(&row.entry.key, row.entry.scope) {
            self.git_config.note = Some(reason);
            return;
        }
        let scope = self.git_config.scope;
        let replacing = self.replacing_in(scope, &row.entry.key, row.entry.scope, &row.entry.value);
        let known = lookup(&key).map(|k| k.kind);
        let title = format!("{key} ({})", scope_name(scope));
        match known {
            Some(kind @ (KeyType::Bool | KeyType::Enum(_))) => {
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
                self.git_config.pick = Some(PickTarget {
                    key,
                    kind: kind.value_kind(),
                    replacing,
                    values,
                });
                self.popup = Some(Popup::Menu(MenuState {
                    title,
                    items,
                    selected,
                }));
            },
            _ => {
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
            },
        }
    }

    /// `Space`: flip a known boolean key.
    pub(super) fn toggle_git_config_bool(&mut self) {
        let Some(row) = self.git_config.selected_row().cloned() else {
            return;
        };
        let key = row.entry.key.clone();
        if lookup(&key).map(|k| k.kind) != Some(KeyType::Bool) {
            self.git_config.note = Some(format!("{key} is not a boolean: Enter edits it"));
            return;
        }
        if let Some(reason) = self.git_config_read_only(&key, row.entry.scope) {
            self.git_config.note = Some(reason);
            return;
        }
        let scope = self.git_config.scope;
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
    pub(super) fn add_git_config_key(&mut self) {
        let scope = self.git_config.scope;
        self.open_name(
            NameKind::ConfigKey,
            format!("New key ({})", scope_name(scope)),
            TextInput::default(),
        );
    }

    /// A menu row of the allowed values was chosen.
    pub(super) fn pick_config_value(&mut self, index: usize) {
        let Some(target) = self.git_config.pick.take() else {
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
    pub(super) fn submit_git_config_name(&mut self, kind: &NameKind, text: &str) -> bool {
        match kind {
            NameKind::ConfigKey => {
                let key = text.trim().to_owned();
                if key.is_empty() {
                    self.report_notice("a key needs a name");
                    return false;
                }
                let scope = self.git_config.scope;
                let exists = self.git_config.rows.iter().any(|r| {
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
                };
                self.perform_git_config_op(&op)
            },
            _ => true,
        }
    }

    /// Why this value cannot be edited here, if it cannot.
    fn git_config_read_only(&self, key: &str, scope: Scope) -> Option<String> {
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
            .git_config
            .rows
            .iter()
            .filter(|r| r.entry.key == key && scope_of(r.entry.scope) == Some(target))
            .count();
        (held > 1 && scope_of(row_scope) == Some(target)).then(|| value.to_owned())
    }

    /// Run one change, re-read, and say what happened. `false` when git refused
    /// (its message is in the toast and nothing changed).
    pub(super) fn perform_git_config_op(&mut self, op: &ConfigOp) -> bool {
        let Some(repo) = &self.repo else {
            return false;
        };
        let scope = self.git_config.scope;
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
        };
        if let Err(e) = result {
            self.report_error(e);
            return false;
        }
        let (ConfigOp::Set { key, .. } | ConfigOp::Add { key, .. }) = op;
        self.reread_git_config();
        self.git_config.note = Some(format!("{key} changed in {}", scope_name(scope)));
        self.request_refresh();
        true
    }

    /// Keys of the config screen's edit actions, called from `git_config_key`.
    pub(super) fn git_config_edit_key(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Char('e') | KeyCode::Enter => self.edit_git_config_value(),
            KeyCode::Char(' ') => self.toggle_git_config_bool(),
            KeyCode::Char('a') => self.add_git_config_key(),
            _ => return false,
        }
        true
    }
}

/// The write scope that names the same file as a listed scope.
const fn scope_of(scope: Scope) -> Option<WriteScope> {
    match scope {
        Scope::Local => Some(WriteScope::Local),
        Scope::Global => Some(WriteScope::Global),
        Scope::Worktree => Some(WriteScope::Worktree),
        Scope::System | Scope::Command => None,
    }
}
