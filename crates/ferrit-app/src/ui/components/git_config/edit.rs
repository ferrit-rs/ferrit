//! Pending actions for the git config screen.

use ferrit_domain::config::{Scope, ValueKind, WriteScope};

/// One change the screen can ask git for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ConfigOp {
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
    /// Remove one value (or the whole key when the value is empty or unknown).
    Unset { key: String, value: Option<String> },
}

/// What to carry on with once the first global write is confirmed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GlobalResume {
    Edit,
    Toggle,
    Add,
}

/// What a menu row will set, remembered while the menu is up.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PickTarget {
    pub(crate) key: String,
    pub(crate) kind: ValueKind,
    pub(crate) replacing: Option<String>,
    pub(crate) values: &'static [&'static str],
}

pub(crate) fn scope_name(scope: WriteScope) -> &'static str {
    scope.into()
}

pub(crate) fn truthy(value: &str) -> bool {
    value.is_empty()
        || matches!(
            value.to_ascii_lowercase().as_str(),
            "true" | "yes" | "on" | "1"
        )
}

pub(crate) fn scope_label(scope: Scope) -> &'static str {
    match scope {
        Scope::Command => "command-line",
        _ => scope.into(),
    }
}

/// The write scope that names the same file as a listed scope.
pub(crate) const fn scope_of(scope: Scope) -> Option<WriteScope> {
    match scope {
        Scope::Local => Some(WriteScope::Local),
        Scope::Global => Some(WriteScope::Global),
        Scope::Worktree => Some(WriteScope::Worktree),
        Scope::System | Scope::Command => None,
    }
}
