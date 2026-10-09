//! The five left panes: `drills`.

use crate::git::model::{CommitEntry, FileEntry};
use std::collections::HashSet;
use std::path::PathBuf;

/// State for the Branches pane's Enter-to-drill-down (lazygit's branch ->
/// log): the pane itself swaps its branch list for one branch's commit list,
/// in place, rather than moving focus elsewhere. Distinct from the passive
/// `DiffView::BranchLog` preview, which needs no Enter at all.
pub(crate) struct BranchDrill {
    pub(crate) branch: String,
    pub(crate) commits: Vec<CommitEntry>,
    /// The branch-list cursor to restore when `Esc` backs out.
    pub(crate) return_index: usize,
}

/// State for the Commits pane's Enter-to-drill-down: the pane swaps its
/// commit list for that commit's own changed-file tree, in place, the same
/// shape `BranchDrill` gives the Branches pane one level up. Read only, no
/// staging; `Esc` backs out.
pub(crate) struct CommitDrill {
    pub(crate) hash: String,
    /// `"<short_hash> <summary>"`, for `App::commits_title`.
    pub(crate) title: String,
    /// One synthetic `FileEntry` per file the commit's diff touched, same
    /// index order as the underlying `git::diff::Diff::files`/`file_lines()` so a
    /// selected row's scroll target is a plain index lookup.
    pub(crate) files: Vec<FileEntry>,
    /// The commit-list cursor to restore when `Esc` backs out.
    pub(crate) return_index: usize,
    /// Directory rows the user collapsed in this drill; starts empty, so a
    /// commit opens fully expanded whatever the Files pane has collapsed.
    pub(crate) collapsed: HashSet<PathBuf>,
}
