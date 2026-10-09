//! The five left panes: `rows`.

use crate::git::Snapshot;
use crate::git::model::{BranchEntry, CommitEntry, FileEntry, StashEntry};
use crate::theme::palette::Palette;
use crate::tui::components::panes::nav::BranchesTab;
use crate::tui::components::panes::nav::Nav;
use crate::tui::components::panes::nav::Pane;
use crate::tui::components::panes::tree::FileRow;
use crate::tui::components::panes::tree::SelectionKey;
use crate::tui::components::panes::tree::StageState;
use crate::tui::components::panes::tree::dir_stage_state;
use crate::tui::components::panes::tree::drill_tree_rows;
use crate::tui::components::panes::tree::find_file_row_key;
use crate::tui::components::panes::tree::selection_key_for_file_rows;
use crate::tui::components::panes::tree::tree_rows;
use crate::tui::row_lines;
use ratatui::text::Line;

pub(crate) struct PaneRows<'a> {
    pub(crate) nav: &'a Nav,
    pub(crate) snapshot: &'a Snapshot,
    pub(crate) palette: &'a Palette,
}

impl PaneRows<'_> {
    /// Files pane rows, lazygit-style directory tree: single-child directory
    /// chains folded, a root ("/") first only when it has two or more
    /// children, changed files grouped under directory header rows. Empty when nothing changed. Built fresh from
    /// `self.snapshot.files` and `self.nav.collapsed_dirs` on every call; cheap at
    /// working-tree sizes, same choice `branch_lines`/`commit_lines` make.
    pub(crate) fn files_tree_rows(&self) -> Vec<FileRow> {
        tree_rows(&self.snapshot.files, &self.nav.collapsed_dirs)
    }

    /// Same tree shape as `files_tree_rows`, over a drilled commit's own
    /// changed files instead of the worktree's. Empty while not drilled.
    pub(crate) fn commit_tree_rows(&self) -> Vec<FileRow> {
        match &self.nav.commit_drill {
            Some(drill) => drill_tree_rows(&drill.files, &drill.collapsed),
            None => Vec::new(),
        }
    }

    /// Selectable row count for a pane, for clamping the cursor and deciding
    /// whether to draw a highlight.
    pub(crate) fn row_count(&self, pane: Pane) -> usize {
        match pane {
            Pane::Status => 0,
            Pane::Files => self.files_tree_rows().len(),
            Pane::Branches if self.nav.branches_tab == BranchesTab::Remotes => 0,
            Pane::Branches => self
                .nav
                .branch_drill
                .as_ref()
                .map_or(self.snapshot.branches.len(), |drill| drill.commits.len()),
            Pane::Commits => match &self.nav.commit_drill {
                Some(_) => self.commit_tree_rows().len(),
                None => self.snapshot.commits.len(),
            },
            Pane::Stash => self.snapshot.stashes.len(),
        }
    }

    pub(crate) fn selection_key(&self, pane: Pane) -> Option<SelectionKey> {
        match pane {
            Pane::Status => None,
            Pane::Files => selection_key_for_file_rows(
                &self.files_tree_rows(),
                &self.snapshot.files,
                self.nav.selection[pane],
            ),
            Pane::Branches if self.nav.branches_tab == BranchesTab::Remotes => None,
            Pane::Branches => self.nav.branch_drill.as_ref().map_or_else(
                || {
                    self.snapshot
                        .branches
                        .get(self.nav.selection[pane])
                        .map(|entry| SelectionKey::Branch(entry.name.clone()))
                },
                |drill| {
                    drill
                        .commits
                        .get(self.nav.selection[pane])
                        .map(|entry| SelectionKey::Commit(entry.full_hash.clone()))
                },
            ),
            Pane::Commits => self.nav.commit_drill.as_ref().map_or_else(
                || {
                    self.snapshot
                        .commits
                        .get(self.nav.selection[pane])
                        .map(|entry| SelectionKey::Commit(entry.full_hash.clone()))
                },
                |drill| {
                    selection_key_for_file_rows(
                        &self.commit_tree_rows(),
                        &drill.files,
                        self.nav.selection[pane],
                    )
                },
            ),
            Pane::Stash => self
                .snapshot
                .stashes
                .get(self.nav.selection[pane])
                .map(|entry| SelectionKey::Stash(entry.oid.clone())),
        }
    }

    pub(crate) fn find_selection_key(&self, pane: Pane, key: &SelectionKey) -> Option<usize> {
        match (pane, key) {
            (Pane::Files, SelectionKey::File(_) | SelectionKey::Directory(_)) => {
                find_file_row_key(&self.files_tree_rows(), &self.snapshot.files, key)
            },
            (Pane::Branches, SelectionKey::Branch(name)) if self.nav.branch_drill.is_none() => self
                .snapshot
                .branches
                .iter()
                .position(|entry| entry.name == *name),
            (Pane::Branches, SelectionKey::Commit(hash)) => self
                .nav
                .branch_drill
                .as_ref()?
                .commits
                .iter()
                .position(|entry| entry.full_hash == *hash),
            (Pane::Commits, SelectionKey::Commit(hash)) if self.nav.commit_drill.is_none() => self
                .snapshot
                .commits
                .iter()
                .position(|entry| entry.full_hash == *hash),
            (Pane::Commits, SelectionKey::File(_) | SelectionKey::Directory(_)) => {
                let drill = self.nav.commit_drill.as_ref()?;
                find_file_row_key(&self.commit_tree_rows(), &drill.files, key)
            },
            (Pane::Stash, SelectionKey::Stash(oid)) => self
                .snapshot
                .stashes
                .iter()
                .position(|entry| entry.oid == *oid),
            _ => None,
        }
    }

    /// `(current, total)` for the pane's `N of M` border counter, or `None`
    /// when the pane has no selectable rows.
    pub(crate) fn counter(&self, pane: Pane) -> Option<(usize, usize)> {
        let total = self.row_count(pane);
        (total > 0).then(|| (self.nav.selection[pane].min(total - 1) + 1, total))
    }

    /// Files pane rows, or a single "working tree clean" line: a flat list,
    /// or lazygit's directory tree once any changed file sits below the
    /// repo root (`files_tree_rows`).
    pub(crate) fn file_lines(&self) -> Vec<Line<'static>> {
        if self.snapshot.files.is_empty() {
            return vec![Line::raw("working tree clean")];
        }
        self.files_tree_rows()
            .iter()
            .filter_map(|row| match row {
                FileRow::Dir {
                    path,
                    name,
                    depth,
                    expanded,
                } => Some(row_lines::dir_line(
                    self.palette,
                    name,
                    *depth,
                    *expanded,
                    dir_stage_state(&self.snapshot.files, path),
                )),
                FileRow::File { index, depth } => self
                    .snapshot
                    .files
                    .get(*index)
                    .map(|entry| row_lines::file_line(self.palette, entry, *depth)),
            })
            .collect()
    }

    /// Porcelain-style `XY path` text for one Files tree row, or an empty
    /// string for a directory row. Debug/probe helper; keyed by the same
    /// row index `file_lines`/`row_count` use, not a flat index into
    /// `self.snapshot.files`.
    pub(crate) fn file_display(&self, i: usize) -> String {
        match self.files_tree_rows().get(i) {
            Some(&FileRow::File { index, .. }) => self
                .snapshot
                .files
                .get(index)
                .map(FileEntry::display)
                .unwrap_or_default(),
            _ => String::new(),
        }
    }

    /// Is the selected Files row a directory (the root row included)?
    pub(crate) fn files_selection_is_dir(&self) -> bool {
        matches!(
            self.files_tree_rows().get(self.nav.selection[Pane::Files]),
            Some(FileRow::Dir { .. })
        )
    }

    /// Branches pane rows: the branch list, or one branch's own commit log
    /// while drilled in (`branch_drill`, `enter_branch_log`), each with its
    /// own empty-state line.
    pub(crate) fn branch_lines(&self, head_status: Option<&str>) -> Vec<Line<'static>> {
        if let Some(drill) = &self.nav.branch_drill {
            if drill.commits.is_empty() {
                return vec![Line::raw("no commits yet")];
            }
            return drill
                .commits
                .iter()
                .map(|entry| row_lines::commit_line(self.palette, entry))
                .collect();
        }
        if self.nav.branches_tab == BranchesTab::Remotes {
            if self.snapshot.remotes.is_empty() {
                return vec![Line::raw("no remotes configured")];
            }
            return self
                .snapshot
                .remotes
                .iter()
                .map(|entry| row_lines::remote_line(self.palette, entry))
                .collect();
        }
        if self.snapshot.branches.is_empty() {
            return vec![Line::raw("no local branches")];
        }
        self.snapshot
            .branches
            .iter()
            .map(|branch| {
                let status = if branch.is_head { head_status } else { None };
                row_lines::branch_line_with_status(self.palette, branch, status)
            })
            .collect()
    }

    /// `[3] Local branches - Remotes - Tags`, or `[3] Commits (<branch>)`
    /// while drilled into a branch's log (Enter on a branch, `Esc` to back
    /// out; see `enter_branch_log`).
    pub(crate) fn branches_title(&self) -> String {
        match &self.nav.branch_drill {
            Some(drill) => format!("[3] Commits ({})", drill.branch),
            None => Pane::Branches.title().to_owned(),
        }
    }

    /// Commits pane rows: the commit list, or one commit's own changed-file
    /// tree while drilled in (`commit_drill`, `enter_commit_files`), same
    /// shape `branch_lines` gives the Branches pane.
    pub(crate) fn commit_lines(&self) -> Vec<Line<'static>> {
        if let Some(drill) = &self.nav.commit_drill {
            return self
                .commit_tree_rows()
                .iter()
                .filter_map(|row| match row {
                    FileRow::Dir {
                        name,
                        depth,
                        expanded,
                        ..
                    } => Some(row_lines::dir_line(
                        self.palette,
                        name,
                        *depth,
                        *expanded,
                        StageState::None,
                    )),
                    FileRow::File { index, depth } => drill
                        .files
                        .get(*index)
                        .map(|entry| row_lines::file_line(self.palette, entry, *depth)),
                })
                .collect();
        }
        if self.snapshot.commits.is_empty() {
            return vec![Line::raw("no commits yet")];
        }
        self.snapshot
            .commits
            .iter()
            .map(|entry| row_lines::commit_line(self.palette, entry))
            .collect()
    }

    /// `[4] Commits - Reflog`, or `[4] Diff files (<hash> <summary>)` while
    /// drilled into a commit's own changed-file tree (Enter on a commit,
    /// `Esc` to back out; see `enter_commit_files`).
    pub(crate) fn commits_title(&self) -> String {
        match &self.nav.commit_drill {
            Some(drill) => format!("[4] Diff files ({})", drill.title),
            None => Pane::Commits.title().to_owned(),
        }
    }

    /// Stash pane rows, or the empty-state line.
    pub(crate) fn stash_lines(&self) -> Vec<Line<'static>> {
        if self.snapshot.stashes.is_empty() {
            return vec![Line::raw("(no stash entries)")];
        }
        self.snapshot
            .stashes
            .iter()
            .map(|entry| row_lines::stash_line(self.palette, entry))
            .collect()
    }
}

impl<'a> PaneRows<'a> {
    /// The stash entry under the cursor.
    pub(crate) fn selected_stash(&self) -> Option<&'a StashEntry> {
        self.snapshot.stashes.get(self.nav.selection[Pane::Stash])
    }

    /// The commit under the cursor on the Commits pane's list.
    pub(crate) fn selected_commit(&self) -> Option<&'a CommitEntry> {
        self.snapshot.commits.get(self.nav.selection[Pane::Commits])
    }

    /// The branch under the cursor on the Branches pane's local list.
    pub(crate) fn selected_branch(&self) -> Option<&'a BranchEntry> {
        self.snapshot
            .branches
            .get(self.nav.selection[Pane::Branches])
    }

    /// The `FileEntry` behind the Files pane's current selection, or `None` on
    /// a directory row or an empty pane.
    pub(crate) fn selected_file(&self) -> Option<&'a FileEntry> {
        let rows = self.files_tree_rows();
        let FileRow::File { index, .. } = rows.get(self.nav.selection[Pane::Files])? else {
            return None;
        };
        self.snapshot.files.get(*index)
    }
}
