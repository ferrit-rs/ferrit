//! Files-pane navigation intents.

use crate::git::diff::DiffOpts;
use crate::tui::components::files::tree::FileRow;
use crate::tui::components::files::tree::commit_drill_files;
use crate::tui::components::panes::drills::CommitDrill;
use crate::tui::components::panes::nav::Pane;
use crate::tui::event::Env;
use crate::tui::event::Event;
use std::collections::HashSet;

/// Enter on a directory row in the Files pane: toggle it collapsed or expanded
/// (lazygit's tree). A no-op on a file row (`files::enter_diff` handles that one
/// instead).
pub(crate) fn toggle_files_dir(env: &Env<'_>) -> Vec<Event> {
    if env.nav.focus != Pane::Files {
        return Vec::new();
    }
    let rows = env.rows().files_tree_rows();
    match rows.get(env.nav.selection[Pane::Files]) {
        Some(FileRow::Dir { path, .. }) => vec![Event::ToggleFilesDir(path.clone())],
        _ => Vec::new(),
    }
}

/// Enter on the Commits pane: swap the commit list for that commit's own
/// changed-file tree, in place, `branches::enter_log`'s counterpart one pane
/// over. Read only. `Esc` backs out. `true` when it drilled in.
pub(crate) fn enter_commit_files(env: &Env<'_>, opts: DiffOpts) -> (bool, Vec<Event>) {
    if env.nav.focus != Pane::Commits || env.nav.commit_drill.is_some() {
        return (false, Vec::new());
    }
    let Some(repo) = env.repo else {
        return (false, Vec::new());
    };
    let return_index = env.nav.selection[Pane::Commits];
    let Some(entry) = env.snapshot.commits.get(return_index) else {
        return (false, Vec::new());
    };
    let hash = entry.full_hash.clone();
    let title = format!("{} {}", entry.short_hash, entry.summary);
    match repo.commit_diff(&hash, opts) {
        Ok(diff) => (
            true,
            vec![Event::DrillIntoCommit(CommitDrill {
                hash,
                title,
                files: commit_drill_files(&diff),
                return_index,
                collapsed: HashSet::default(),
            })],
        ),
        Err(e) => (false, vec![Event::Report(e.into())]),
    }
}

/// Enter on a directory row while drilled into a commit's file tree: toggle it
/// collapsed or expanded, `toggle_files_dir`'s counterpart.
pub(crate) fn toggle_commit_dir(env: &Env<'_>) -> Vec<Event> {
    if env.nav.focus != Pane::Commits || env.nav.commit_drill.is_none() {
        return Vec::new();
    }
    let rows = env.rows().commit_tree_rows();
    match rows.get(env.nav.selection[Pane::Commits]) {
        Some(FileRow::Dir { path, .. }) => vec![Event::ToggleCommitDir(path.clone())],
        _ => Vec::new(),
    }
}
