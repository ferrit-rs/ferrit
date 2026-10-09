//! The five left panes: `tree`.

use crate::git;
use crate::git::model::FileEntry;
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectionKey {
    File(PathBuf),
    Directory(PathBuf),
    Branch(String),
    Commit(String),
    Stash(String),
}

pub(crate) fn selection_key_for_file_rows(
    rows: &[FileRow],
    files: &[FileEntry],
    selected: usize,
) -> Option<SelectionKey> {
    match rows.get(selected)? {
        FileRow::Dir { path, .. } => Some(SelectionKey::Directory(path.clone())),
        FileRow::File { index, .. } => files
            .get(*index)
            .map(|entry| SelectionKey::File(entry.path.clone())),
    }
}

pub(crate) fn find_file_row_key(
    rows: &[FileRow],
    files: &[FileEntry],
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

/// One visible row of the Files pane's directory tree (lazygit style).
/// `App::files_tree_rows` builds these fresh from `self.snapshot.files` and
/// `self.nav.collapsed_dirs` on every call — cheap at working-tree sizes, same
/// "no cache" choice `branch_lines`/`commit_lines` already make.
pub(crate) enum FileRow {
    /// A directory header, or the root ("/", the repo's own worktree, only
    /// when it has two or more children). `path` is empty for the root and
    /// the full path of the last folded directory otherwise.
    Dir {
        path: PathBuf,
        name: String,
        depth: usize,
        expanded: bool,
    },
    /// A changed file. `index` into `App.files`.
    File { index: usize, depth: usize },
}

/// How much of a directory (or of the whole tree, for the root) is staged.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum StageState {
    None,
    Partial,
    All,
}

/// Staging aggregate over the files under `dir` (empty path: all of them),
/// lazygit's rule: `All` when every file is fully staged, `Partial` when any
/// has something in the index, `None` otherwise.
pub(crate) fn dir_stage_state(files: &[FileEntry], dir: &Path) -> StageState {
    let mut under = files.iter().filter(|f| f.path.starts_with(dir)).peekable();
    if under.peek().is_none() {
        return StageState::None;
    }
    let (mut all, mut any) = (true, false);
    for file in under {
        all &= file.is_fully_staged();
        any |= file.has_staged();
    }
    match (all, any) {
        (true, _) => StageState::All,
        (false, true) => StageState::Partial,
        _ => StageState::None,
    }
}

/// One level of the Files tree, name -> child. `BTreeMap` for free
/// alphabetical iteration (matches lazygit: siblings sorted by name,
/// directories and files interleaved, not directories-first).
pub(crate) enum TreeNode {
    Dir(BTreeMap<String, Self>),
    File(usize),
}

/// Group `files` by directory into a tree keyed by path component. A path
/// component that collides with an existing file entry (pathological: git
/// cannot really produce this) drops that one file rather than panicking.
pub(crate) fn build_file_tree(files: &[FileEntry]) -> BTreeMap<String, TreeNode> {
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

/// The Files pane's tree, as lazygit draws it: single-child directory chains
/// folded into one row (`x/y/z`), and a collapsible root ("/") only when the
/// root has two or more children; a lone child (file or folded directory)
/// sits at depth 0 with no root row. Empty when nothing changed.
pub(crate) fn tree_rows(files: &[FileEntry], collapsed: &HashSet<PathBuf>) -> Vec<FileRow> {
    let tree = build_file_tree(files);
    let mut rows = Vec::new();
    if tree.len() < 2 {
        flatten_folded(&tree, Path::new(""), 0, collapsed, &mut rows);
        return rows;
    }
    let expanded = !collapsed.contains(Path::new(""));
    rows.push(FileRow::Dir {
        path: PathBuf::new(),
        name: "/".to_owned(),
        depth: 0,
        expanded,
    });
    if expanded {
        flatten_folded(&tree, Path::new(""), 1, collapsed, &mut rows);
    }
    rows
}

/// A drilled commit's tree, as lazygit shows it: no root row, and a chain of
/// single-child directories folded into one row (`test/flows`). `collapsed`
/// is the drill's own set, keyed by the folded row's full path.
pub(crate) fn drill_tree_rows(files: &[FileEntry], collapsed: &HashSet<PathBuf>) -> Vec<FileRow> {
    let mut rows = Vec::new();
    flatten_folded(
        &build_file_tree(files),
        Path::new(""),
        0,
        collapsed,
        &mut rows,
    );
    rows
}

pub(crate) fn flatten_folded(
    nodes: &BTreeMap<String, TreeNode>,
    dir_path: &Path,
    depth: usize,
    collapsed: &HashSet<PathBuf>,
    rows: &mut Vec<FileRow>,
) {
    for (name, node) in nodes {
        match node {
            TreeNode::Dir(children) => {
                let (mut name, mut children) = (name.clone(), children);
                while let [(child, TreeNode::Dir(grand))] = children.iter().collect::<Vec<_>>()[..]
                {
                    name = format!("{name}/{child}");
                    children = grand;
                }
                let path = dir_path.join(&name);
                let expanded = !collapsed.contains(&path);
                rows.push(FileRow::Dir {
                    path: path.clone(),
                    name,
                    depth,
                    expanded,
                });
                if expanded {
                    flatten_folded(children, &path, depth + 1, collapsed, rows);
                }
            },
            TreeNode::File(index) => rows.push(FileRow::File {
                index: *index,
                depth,
            }),
        }
    }
}

/// One synthetic `FileEntry` per file in a commit's diff, `staged: None` /
/// `worktree: <status>` so `row_lines::file_line` renders the single-letter code
/// lazygit shows for a commit's file tree (` M`, not the two-sided `MM` a
/// worktree entry can have). Feeds `CommitDrill::files`.
pub(crate) fn commit_drill_files(diff: &git::diff::Diff) -> Vec<FileEntry> {
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
                git::diff::parse::FileStatus::Added => git::model::Change::Added,
                git::diff::parse::FileStatus::Deleted => git::model::Change::Deleted,
                git::diff::parse::FileStatus::Modified => git::model::Change::Modified,
                git::diff::parse::FileStatus::Renamed | git::diff::parse::FileStatus::Copied => {
                    git::model::Change::Renamed
                },
            };
            FileEntry {
                path: PathBuf::from(path),
                staged: git::model::Change::None,
                worktree,
                binary: meta.binary,
            }
        })
        .collect()
}
