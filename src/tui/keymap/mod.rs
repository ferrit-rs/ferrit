//! Keymap domain: contexts, actions, bindings, defaults, and resolution.
//!
//! `Context` uses `strum` for config names. `Action` keeps a small explicit
//! name match because `Focus(Pane)` carries data and must map to five names.

pub mod action;
pub mod binding;
pub mod context;
pub(crate) mod defaults;

use std::collections::HashMap;
use std::collections::hash_map::Entry as MapEntry;

use crate::config::KeyOverrides;
use crate::tui::keymap::action::Action;
use crate::tui::keymap::binding::KeyBinding;
use crate::tui::keymap::context::Context;

/// `(context, key) -> action`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keymap {
    bindings: HashMap<(Context, KeyBinding), Action>,
}

impl Default for Keymap {
    fn default() -> Self {
        defaults::default_bindings()
    }
}

impl Keymap {
    /// Action `key` triggers, trying contexts in order.
    pub fn resolve(&self, contexts: &[Context], key: KeyBinding) -> Option<Action> {
        contexts
            .iter()
            .find_map(|&context| self.bindings.get(&(context, key)).copied())
    }

    /// Apply user `[keys]` overrides. Returns map and validation issues.
    pub fn from_overrides(overrides: &KeyOverrides) -> (Self, Vec<String>) {
        let mut issues = Vec::new();
        let mut entries: Vec<defaults::Override> = Vec::new();
        for (context_name, actions) in overrides {
            let Some(context) = Context::from_name(context_name) else {
                issues.push(format!("unknown key context `keys.{context_name}` ignored"));
                continue;
            };
            for (action_name, list) in actions {
                let label = format!("keys.{context_name}.{action_name}");
                let Some(action) = Action::from_name(action_name) else {
                    issues.push(format!("`{label}`: unknown action, ignored"));
                    continue;
                };
                if !defaults::ENTRIES
                    .iter()
                    .any(|&(candidate_context, _, candidate)| {
                        candidate_context == context && candidate == action
                    })
                {
                    issues.push(format!(
                        "`{label}`: `{action_name}` is not an action of the `{context_name}` context, ignored"
                    ));
                    continue;
                }
                let mut keys = Vec::new();
                let mut valid = true;
                for text in list.texts() {
                    match KeyBinding::parse(text) {
                        None => {
                            issues.push(format!(
                                "`{label}`: `{text}` is not a key, keeping the default"
                            ));
                            valid = false;
                        },
                        Some(binding) if binding == defaults::RESERVED_QUIT => {
                            issues.push(format!(
                                "`{label}`: ctrl-c always quits and cannot be bound, keeping the default"
                            ));
                            valid = false;
                        },
                        Some(binding) if !keys.contains(&binding) => keys.push(binding),
                        Some(_) => {},
                    }
                }
                if valid {
                    entries.push(defaults::Override {
                        context,
                        action,
                        keys,
                        label,
                    });
                }
            }
        }

        let mut map = Self::default().bindings;
        map.retain(|&(context, _), action| {
            !entries
                .iter()
                .any(|entry| entry.context == context && entry.action == *action)
        });
        let mut rejected: Vec<&defaults::Override> = Vec::new();
        for entry in &entries {
            let clash = entry.keys.iter().find_map(|&key| {
                map.get(&(entry.context, key))
                    .filter(|&&other| other != entry.action)
                    .map(|&other| (key, other))
            });
            match clash {
                Some((key, other)) => {
                    issues.push(format!(
                        "`{}`: `{key}` is already bound to `{}` there, keeping the default",
                        entry.label,
                        other.name()
                    ));
                    rejected.push(entry);
                },
                None => {
                    for &key in &entry.keys {
                        map.insert((entry.context, key), entry.action);
                    }
                },
            }
        }
        for entry in rejected {
            for &(context, text, action) in defaults::ENTRIES {
                if context != entry.context || action != entry.action {
                    continue;
                }
                let Some(key) = KeyBinding::parse(text) else {
                    continue;
                };
                match map.entry((context, key)) {
                    MapEntry::Occupied(_) => issues.push(format!(
                        "`{}`: its default `{key}` is taken too, so `{}` is unbound",
                        entry.label,
                        action.name()
                    )),
                    MapEntry::Vacant(slot) => {
                        slot.insert(action);
                    },
                }
            }
        }
        (Self { bindings: map }, issues)
    }

    /// Keys `action` has in `context`, stable across hash iteration.
    pub fn keys(&self, context: Context, action: Action) -> Vec<KeyBinding> {
        let mut keys: Vec<KeyBinding> = self
            .bindings
            .iter()
            .filter(|&(&(candidate_context, _), &candidate_action)| {
                candidate_context == context && candidate_action == action
            })
            .map(|(&(_, key), _)| key)
            .collect();
        let rank = |key: KeyBinding| {
            defaults::ENTRIES
                .iter()
                .position(|&(candidate_context, text, candidate_action)| {
                    candidate_context == context
                        && candidate_action == action
                        && KeyBinding::parse(text) == Some(key)
                })
                .unwrap_or(usize::MAX)
        };
        keys.sort_by_key(|&key| (rank(key), key.to_string()));
        keys
    }

    /// Actions that live in `context`, in default order.
    pub fn actions_of(context: Context) -> Vec<Action> {
        let mut actions = Vec::new();
        for &(candidate_context, _, action) in defaults::ENTRIES {
            if candidate_context == context && !actions.contains(&action) {
                actions.push(action);
            }
        }
        actions
    }

    /// Every binding, for help and tests.
    pub fn bindings(&self) -> impl Iterator<Item = (Context, KeyBinding, Action)> + '_ {
        self.bindings
            .iter()
            .map(|(&(context, key), &action)| (context, key, action))
    }
}
