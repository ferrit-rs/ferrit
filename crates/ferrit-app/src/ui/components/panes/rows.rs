//! The five left panes: `rows`.

use crate::ui::components::files::rows::FileRows;
use crate::ui::components::files::tree::FileRow;
use crate::ui::components::panes::nav::BranchesTab;
use crate::ui::components::panes::nav::Nav;
use crate::ui::components::panes::nav::Pane;
use crate::ui::components::panes::selection::SelectionKey;
use crate::ui::row_lines;
use ferrit_domain::Snapshot;
use ferrit_domain::model::{BranchEntry, CommitEntry, FileEntry, StashEntry};
use ferrit_theme::palette::Palette;
use ratatui::text::Line;

pub(crate) struct PaneRows<'a> {
    pub(crate) nav: &'a Nav,
    pub(crate) snapshot: &'a Snapshot,
    pub(crate) palette: &'a Palette,
}

impl<'a> PaneRows<'a> {
    fn files(&self) -> FileRows<'a> {
        FileRows::new(self.nav, self.snapshot, self.palette)
    }

    /// Files pane rows, lazygit-style directory tree: single-child directory
    /// chains folded, a root ("/") first only when it has two or more
    /// children, changed files grouped under directory header rows. Empty when nothing changed. Built fresh from
    /// `self.snapshot.files` and `self.nav.collapsed_dirs` on every call; cheap at
    /// working-tree sizes, same choice `branch_lines`/`commit_lines` make.
    pub(crate) fn files_tree_rows(&self) -> Vec<FileRow> {
        self.files().worktree_tree()
    }

    /// Same tree shape as `files_tree_rows`, over a drilled commit's own
    /// changed files instead of the worktree's. Empty while not drilled.
    pub(crate) fn commit_tree_rows(&self) -> Vec<FileRow> {
        self.files().commit_tree()
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
            Pane::Files => self.files().worktree_selection_key(),
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
                |_| self.files().commit_selection_key(),
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
                self.files().find_worktree_selection(key)
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
                self.files().find_commit_selection(key)
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
        self.files().worktree_lines()
    }

    /// Porcelain-style `XY path` text for one Files tree row, or an empty
    /// string for a directory row. Debug/probe helper; keyed by the same
    /// row index `file_lines`/`row_count` use, not a flat index into
    /// `self.snapshot.files`.
    pub(crate) fn file_display(&self, i: usize) -> String {
        self.files().display(i)
    }

    /// Is the selected Files row a directory (the root row included)?
    pub(crate) fn files_selection_is_dir(&self) -> bool {
        self.files().selected_is_directory()
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
                .map(|entry| row_lines::rows::commit_line(self.palette, entry))
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
                .map(|entry| row_lines::rows::remote_line(self.palette, entry))
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
                row_lines::rows::branch_line_with_status(self.palette, branch, status)
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
        if let Some(lines) = self.files().commit_lines() {
            return lines;
        }
        if self.snapshot.commits.is_empty() {
            return vec![Line::raw("no commits yet")];
        }
        self.snapshot
            .commits
            .iter()
            .map(|entry| row_lines::rows::commit_line(self.palette, entry))
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
            .map(|entry| row_lines::rows::stash_line(self.palette, entry))
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
        self.files().selected_file()
    }
}
