//! Files/Commits pane directory-tree drill navigation (expand/collapse, drill into a commit's files).

use super::{App, CommitDrill, DiffOpts, FileRow, Pane, commit_drill_files};

impl App {
    /// Enter on a directory row in the Files pane: toggle it collapsed or
    /// expanded (lazygit's tree). A no-op on a file row (`enter_diff_mode`
    /// handles that one instead).
    pub(super) fn toggle_files_dir(&mut self) {
        if self.focus != Pane::Files {
            return;
        }
        let rows = self.files_tree_rows();
        let Some(FileRow::Dir { path, .. }) = rows.get(self.selected(Pane::Files)) else {
            return;
        };
        if !self.collapsed_dirs.remove(path) {
            self.collapsed_dirs.insert(path.clone());
        }
        let last = self.row_count(Pane::Files).saturating_sub(1);
        self.selection[Pane::Files] = self.selection[Pane::Files].min(last);
    }

    /// Enter on the Commits pane: swap the commit list for that commit's own
    /// changed-file tree, in place, `enter_branch_log`'s counterpart one pane
    /// over. Read only. `Esc` backs out (`on_key`).
    pub(super) fn enter_commit_files(&mut self) {
        if self.focus != Pane::Commits || self.commit_drill.is_some() {
            return;
        }
        let Some(repo) = &self.repo else { return };
        let return_index = self.selected(Pane::Commits);
        let Some(entry) = self.commits.get(return_index) else {
            return;
        };
        let hash = entry.full_hash.clone();
        let title = format!("{} {}", entry.short_hash, entry.summary);
        match repo.commit_diff(&hash, DiffOpts::default()) {
            Ok(diff) => {
                self.commit_drill = Some(CommitDrill {
                    hash,
                    title,
                    files: commit_drill_files(&diff),
                    return_index,
                });
                self.selection[Pane::Commits] = 0;
            },
            Err(e) => self.report_error(e),
        }
    }

    /// Enter on a directory row while drilled into a commit's file tree:
    /// toggle it collapsed or expanded, `toggle_files_dir`'s counterpart.
    pub(super) fn toggle_commit_dir(&mut self) {
        if self.focus != Pane::Commits || self.commit_drill.is_none() {
            return;
        }
        let rows = self.commit_tree_rows();
        let Some(FileRow::Dir { path, .. }) = rows.get(self.selected(Pane::Commits)) else {
            return;
        };
        if !self.collapsed_dirs.remove(path) {
            self.collapsed_dirs.insert(path.clone());
        }
        let last = self.row_count(Pane::Commits).saturating_sub(1);
        self.selection[Pane::Commits] = self.selection[Pane::Commits].min(last);
    }
}
