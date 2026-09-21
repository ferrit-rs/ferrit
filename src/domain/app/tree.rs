//! Lazygit-style directory tree over a file list, shared by the Files pane
//! and a drilled commit's own changed-file list.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use crate::domain::git;

/// One visible row of the Files pane's directory tree (lazygit style).
/// `App::files_tree_rows` builds these fresh from `self.files` and
/// `self.collapsed_dirs` on every call — cheap at working-tree sizes, same
/// "no cache" choice `branch_lines`/`commit_lines` already make.
pub(super) enum FileRow {
    /// A directory header, including the always-present root ("/", the
    /// repo's own worktree). `path` is empty for the root.
    Dir {
        path: PathBuf,
        name: String,
        depth: usize,
        expanded: bool,
    },
    /// A changed file. `index` into `App.files`.
    File { index: usize, depth: usize },
}

/// One level of the Files tree, name -> child. `BTreeMap` for free
/// alphabetical iteration (matches lazygit: siblings sorted by name,
/// directories and files interleaved, not directories-first).
enum TreeNode {
    Dir(BTreeMap<String, Self>),
    File(usize),
}

/// Group `files` by directory into a tree keyed by path component. A path
/// component that collides with an existing file entry (pathological: git
/// cannot really produce this) drops that one file rather than panicking.
fn build_file_tree(files: &[crate::domain::repository::FileEntry]) -> BTreeMap<String, TreeNode> {
    let mut root: BTreeMap<String, TreeNode> = BTreeMap::new();
    'entries: for (index, entry) in files.iter().enumerate() {
        let mut components: Vec<_> = entry.path.components().collect();
        let Some(file_name) = components.pop() else {
            continue;
        };
        let mut dir = &mut root;
        for component in &components {
            let name = component.as_os_str().to_string_lossy().into_owned();
            let child = dir
                .entry(name)
                .or_insert_with(|| TreeNode::Dir(BTreeMap::new()));
            dir = match child {
                TreeNode::Dir(children) => children,
                TreeNode::File(_) => continue 'entries,
            };
        }
        let name = file_name.as_os_str().to_string_lossy().into_owned();
        dir.insert(name, TreeNode::File(index));
    }
    root
}

/// Depth-first flatten of `nodes` (a `build_file_tree` level) into visible
/// rows, skipping the children of any directory in `collapsed`.
fn flatten_file_tree(
    nodes: &BTreeMap<String, TreeNode>,
    dir_path: &Path,
    depth: usize,
    collapsed: &HashSet<PathBuf>,
    rows: &mut Vec<FileRow>,
) {
    for (name, node) in nodes {
        match node {
            TreeNode::Dir(children) => {
                let path = dir_path.join(name);
                let expanded = !collapsed.contains(&path);
                rows.push(FileRow::Dir {
                    path: path.clone(),
                    name: name.clone(),
                    depth,
                    expanded,
                });
                if expanded {
                    flatten_file_tree(children, &path, depth + 1, collapsed, rows);
                }
            },
            TreeNode::File(index) => rows.push(FileRow::File {
                index: *index,
                depth,
            }),
        }
    }
}

/// Lazygit-style directory tree over any file list: nested paths get a
/// collapsible root plus one `Dir` row per directory, flat paths skip the
/// tree and list files directly. Shared by the Files pane (`self.files`) and
/// a drilled commit's own changed-file list (`CommitDrill::files`).
pub(super) fn tree_rows(
    files: &[crate::domain::repository::FileEntry],
    collapsed: &HashSet<PathBuf>,
) -> Vec<FileRow> {
    let nested = files
        .iter()
        .any(|f| f.path.parent().is_some_and(|p| p != Path::new("")));
    if !nested {
        return (0..files.len())
            .map(|index| FileRow::File { index, depth: 0 })
            .collect();
    }

    let tree = build_file_tree(files);
    let root_expanded = !collapsed.contains(Path::new(""));
    let mut rows = vec![FileRow::Dir {
        path: PathBuf::new(),
        name: "/".to_owned(),
        depth: 0,
        expanded: root_expanded,
    }];
    if root_expanded {
        flatten_file_tree(&tree, Path::new(""), 1, collapsed, &mut rows);
    }
    rows
}

/// One synthetic `FileEntry` per file in a commit's diff, `staged: None` /
/// `worktree: <status>` so `theme::file_line` renders the single-letter code
/// lazygit shows for a commit's file tree (` M`, not the two-sided `MM` a
/// worktree entry can have). Feeds `CommitDrill::files`.
pub(super) fn commit_drill_files(
    diff: &git::diff::Diff,
) -> Vec<crate::domain::repository::FileEntry> {
    diff.files
        .iter()
        .map(|meta| {
            let new_path = diff.text.get(meta.new_path.clone()).unwrap_or_default();
            let old_path = diff.text.get(meta.old_path.clone()).unwrap_or_default();
            let path = if new_path == "/dev/null" {
                old_path
            } else {
                new_path
            };
            let worktree = match meta.status {
                git::diff::parse::FileStatus::Added => crate::domain::repository::Change::Added,
                git::diff::parse::FileStatus::Deleted => crate::domain::repository::Change::Deleted,
                git::diff::parse::FileStatus::Modified => {
                    crate::domain::repository::Change::Modified
                },
                git::diff::parse::FileStatus::Renamed | git::diff::parse::FileStatus::Copied => {
                    crate::domain::repository::Change::Renamed
                },
            };
            crate::domain::repository::FileEntry {
                path: PathBuf::from(path),
                staged: crate::domain::repository::Change::None,
                worktree,
                binary: meta.binary,
            }
        })
        .collect()
}
