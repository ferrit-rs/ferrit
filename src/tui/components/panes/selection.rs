//! Stable identities used to restore pane selections after refresh.

use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SelectionKey {
    File(PathBuf),
    Directory(PathBuf),
    Branch(String),
    Commit(String),
    Stash(String),
}
