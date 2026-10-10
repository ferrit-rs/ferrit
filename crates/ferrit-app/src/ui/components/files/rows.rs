//! File-tree presentation and selection projection.

use crate::ui::components::files::tree::FileRow;
use crate::ui::components::files::tree::StageState;
use crate::ui::components::files::tree::dir_stage_state;
use crate::ui::components::files::tree::drill_tree_rows;
use crate::ui::components::files::tree::find_file_row_key;
use crate::ui::components::files::tree::selection_key_for_file_rows;
use crate::ui::components::files::tree::tree_rows;
use crate::ui::components::panes::nav::Nav;
use crate::ui::components::panes::selection::SelectionKey;
use crate::ui::row_lines;
use ferrit_domain::Snapshot;
use ferrit_domain::model::FileEntry;
use ferrit_tui::theme::palette::Palette;
use ratatui::text::Line;

/// Read-only file-pane projection. Tree construction stays in `tree`; this
/// type turns it into rows, lines and stable selection keys.
pub(crate) struct FileRows<'a> {
    nav: &'a Nav,
    snapshot: &'a Snapshot,
    palette: &'a Palette,
}

impl<'a> FileRows<'a> {
    pub(crate) fn new(nav: &'a Nav, snapshot: &'a Snapshot, palette: &'a Palette) -> Self {
        Self {
            nav,
            snapshot,
            palette,
        }
    }

    pub(crate) fn worktree_tree(&self) -> Vec<FileRow> {
        tree_rows(&self.snapshot.files, &self.nav.collapsed_dirs)
    }

    pub(crate) fn commit_tree(&self) -> Vec<FileRow> {
        match &self.nav.commit_drill {
            Some(drill) => drill_tree_rows(&drill.files, &drill.collapsed),
            None => Vec::new(),
        }
    }

    pub(crate) fn worktree_selection_key(&self) -> Option<SelectionKey> {
        selection_key_for_file_rows(
            &self.worktree_tree(),
            &self.snapshot.files,
            self.nav.selection[crate::ui::components::panes::nav::Pane::Files],
        )
    }

    pub(crate) fn commit_selection_key(&self) -> Option<SelectionKey> {
        let drill = self.nav.commit_drill.as_ref()?;
        selection_key_for_file_rows(
            &self.commit_tree(),
            &drill.files,
            self.nav.selection[crate::ui::components::panes::nav::Pane::Commits],
        )
    }

    pub(crate) fn find_worktree_selection(&self, key: &SelectionKey) -> Option<usize> {
        find_file_row_key(&self.worktree_tree(), &self.snapshot.files, key)
    }

    pub(crate) fn find_commit_selection(&self, key: &SelectionKey) -> Option<usize> {
        let drill = self.nav.commit_drill.as_ref()?;
        find_file_row_key(&self.commit_tree(), &drill.files, key)
    }

    pub(crate) fn worktree_lines(&self) -> Vec<Line<'static>> {
        if self.snapshot.files.is_empty() {
            return vec![Line::raw("working tree clean")];
        }
        self.worktree_tree()
            .iter()
            .filter_map(|row| match row {
                FileRow::Dir {
                    path,
                    name,
                    depth,
                    expanded,
                } => Some(row_lines::rows::dir_line(
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
                    .map(|entry| row_lines::rows::file_line(self.palette, entry, *depth)),
            })
            .collect()
    }

    pub(crate) fn commit_lines(&self) -> Option<Vec<Line<'static>>> {
        let drill = self.nav.commit_drill.as_ref()?;
        Some(
            self.commit_tree()
                .iter()
                .filter_map(|row| match row {
                    FileRow::Dir {
                        name,
                        depth,
                        expanded,
                        ..
                    } => Some(row_lines::rows::dir_line(
                        self.palette,
                        name,
                        *depth,
                        *expanded,
                        StageState::None,
                    )),
                    FileRow::File { index, depth } => drill
                        .files
                        .get(*index)
                        .map(|entry| row_lines::rows::file_line(self.palette, entry, *depth)),
                })
                .collect(),
        )
    }

    pub(crate) fn display(&self, index: usize) -> String {
        match self.worktree_tree().get(index) {
            Some(&FileRow::File { index, .. }) => self
                .snapshot
                .files
                .get(index)
                .map(FileEntry::display)
                .unwrap_or_default(),
            _ => String::new(),
        }
    }

    pub(crate) fn selected_is_directory(&self) -> bool {
        matches!(
            self.worktree_tree()
                .get(self.nav.selection[crate::ui::components::panes::nav::Pane::Files]),
            Some(FileRow::Dir { .. })
        )
    }

    pub(crate) fn selected_file(&self) -> Option<&'a FileEntry> {
        let rows = self.worktree_tree();
        let FileRow::File { index, .. } =
            rows.get(self.nav.selection[crate::ui::components::panes::nav::Pane::Files])?
        else {
            return None;
        };
        self.snapshot.files.get(*index)
    }
}
