//! Which row of a pane is selected, and how to find it again after a refresh.

use super::tree::FileRow;
use crate::git;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectionKey {
    File(PathBuf),
    Directory(PathBuf),
    Branch(String),
    Commit(String),
    Stash(String),
}

pub(super) fn selection_key_for_file_rows(
    rows: &[FileRow],
    files: &[git::model::FileEntry],
    selected: usize,
) -> Option<SelectionKey> {
    match rows.get(selected)? {
        FileRow::Dir { path, .. } => Some(SelectionKey::Directory(path.clone())),
        FileRow::File { index, .. } => files
            .get(*index)
            .map(|entry| SelectionKey::File(entry.path.clone())),
    }
}

pub(super) fn find_file_row_key(
    rows: &[FileRow],
    files: &[git::model::FileEntry],
    key: &SelectionKey,
) -> Option<usize> {
    rows.iter().position(|row| match (row, key) {
        (FileRow::Dir { path, .. }, SelectionKey::Directory(wanted)) => path == wanted,
        (FileRow::File { index, .. }, SelectionKey::File(wanted)) => {
            files.get(*index).is_some_and(|entry| entry.path == *wanted)
        },
        _ => false,
    })
}
