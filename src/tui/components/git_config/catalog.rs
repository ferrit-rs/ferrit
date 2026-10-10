//! Git keys that get a picker in the config screen.

use crate::git::config::ValueKind;

/// What a known key accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyType {
    /// `true` or `false`.
    Bool,
    /// One of these literals.
    Enum(&'static [&'static str]),
    /// Free text.
    Text,
}

impl KeyType {
    /// The `--type=` git validates the write with. An enum is checked by the
    /// picker, so git sees plain text.
    #[must_use]
    pub const fn value_kind(self) -> ValueKind {
        match self {
            Self::Bool => ValueKind::Bool,
            Self::Enum(_) | Self::Text => ValueKind::Text,
        }
    }
}

/// A key ferrit offers in the config editor, with what it accepts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KnownKey {
    /// Lower-case, as `git config --list` prints it.
    pub key: &'static str,
    /// What the key accepts.
    pub kind: KeyType,
}

const fn known(key: &'static str, kind: KeyType) -> KnownKey {
    KnownKey { key, kind }
}

/// The keys the config editor knows how to edit with a picker, instead of free text.
pub const KNOWN_KEYS: &[KnownKey] = &[
    known("commit.gpgsign", KeyType::Bool),
    known("fetch.prune", KeyType::Bool),
    known("push.autosetupremote", KeyType::Bool),
    known("rebase.autosquash", KeyType::Bool),
    known("rebase.autostash", KeyType::Bool),
    known(
        "pull.rebase",
        KeyType::Enum(&["false", "true", "merges", "interactive"]),
    ),
    known("pull.ff", KeyType::Enum(&["true", "false", "only"])),
    known(
        "push.default",
        KeyType::Enum(&["nothing", "current", "upstream", "simple", "matching"]),
    ),
    known(
        "merge.conflictstyle",
        KeyType::Enum(&["merge", "diff3", "zdiff3"]),
    ),
    known("core.autocrlf", KeyType::Enum(&["true", "false", "input"])),
    known(
        "diff.algorithm",
        KeyType::Enum(&["default", "myers", "minimal", "patience", "histogram"]),
    ),
    known("gpg.format", KeyType::Enum(&["openpgp", "x509", "ssh"])),
    known("user.name", KeyType::Text),
    known("user.email", KeyType::Text),
    known("user.signingkey", KeyType::Text),
    known("core.editor", KeyType::Text),
    known("init.defaultbranch", KeyType::Text),
];

/// The table entry for `key`; git lower-cases section and variable names, so
/// the match ignores case (a subsection such as `url.<base>.insteadof` is
/// never known).
#[must_use]
pub fn lookup(key: &str) -> Option<&'static KnownKey> {
    KNOWN_KEYS.iter().find(|k| k.key.eq_ignore_ascii_case(key))
}
