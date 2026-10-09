//! The five left panes: where the cursor is, what each lists, the drill-downs, where the last frame put them, and how the column is drawn.

use crate::git;
use crate::git::Snapshot;
use crate::git::diff::DiffOpts;
use crate::git::model::{BranchEntry, CommitEntry, FileEntry, StashEntry};
use crate::theme::palette::Palette;
use crate::tui::components::diff::Mode;
use crate::tui::components::keybar::KeybarHit;
use crate::tui::components::settings::SettingsHits;
use crate::tui::draw::Landed;
use crate::tui::event::Env;
use crate::tui::event::Event;
use crate::tui::widgets::pane_list::PaneList;
use crate::tui::widgets::panel::Panel;
use crate::tui::workers::Shared;
use crate::tui::{App, row_lines};
use enum_map::{Enum, EnumMap};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Default)]
pub struct Nav {
    /// Which left pane has focus.
    pub focus: Pane,
    /// Selection cursor per pane, keyed by `Pane`.
    pub selection: EnumMap<Pane, usize>,
    /// Directories collapsed in the Files pane's tree view (`FileRow`,
    /// `files_tree_rows`). Empty means "everything expanded", lazygit's own
    /// default; paths persist across `refresh()`, only `Enter` on a
    /// directory row changes this.
    pub(crate) collapsed_dirs: HashSet<PathBuf>,
    /// `Some` while the Branches pane is drilled into one branch's own log
    /// (Enter on a branch, `Esc` to back out); `None` shows the branch list.
    pub(crate) branch_drill: Option<BranchDrill>,
    /// Which of the Branches pane's own two tabs is showing.
    /// `Ctrl-Right`/`Ctrl-Left` switch it, Branches focused.
    pub(crate) branches_tab: BranchesTab,
    /// `Some` while the Commits pane is drilled into one commit's own
    /// changed-file tree (Enter on a commit, `Esc` to back out); `None`
    /// shows the commit list.
    pub(crate) commit_drill: Option<CommitDrill>,
    /// Rows an action just created (the new branch, the new `HEAD`) that the
    /// selection moves to once a refresh lists them, as lazygit does. Kept
    /// until found, so a refresh already in flight when the action ran, which
    /// cannot list them yet, does not lose it.
    pub(crate) select_when_listed: Vec<(Pane, SelectionKey)>,
    /// A click landed on the right pane. Purely a border-highlight flag for
    /// now (see `docs/PLAN_5_CLICK_BEHAVIOR.md`, "right-pane-focus plan");
    /// left-pane navigation and selection are untouched. Cleared by `Esc` or
    /// a click back on a left pane.
    pub(crate) right_focused: bool,
    /// Whether keys go to the left panes or to the Files diff cursor
    /// (`docs/PLAN_6_STAGING.md`).
    pub(crate) mode: Mode,
}

impl Nav {
    /// The branch and the commit the user is drilled into, if any: what a
    /// refresh has to re-read besides the snapshot.
    pub(crate) fn drill_targets(&self) -> (Option<String>, Option<String>) {
        (
            self.branch_drill.as_ref().map(|drill| drill.branch.clone()),
            self.commit_drill.as_ref().map(|drill| drill.hash.clone()),
        )
    }

    /// Whether the Branches pane is drilled into one branch's own commit
    /// log right now. `ui::draw_keybar` uses this to fall back to the
    /// default keybar there — `<space>`/`n`/`d`/`u`/`M` act on a branch
    /// list row, not a commit row, so the Branches-specific hints would be
    /// misleading while drilled in.
    pub(crate) fn branches_drilled(&self) -> bool {
        self.branch_drill.is_some()
    }

    /// Is the Commits pane showing one commit's changed files instead of the
    /// commit list? The commit rewrite keys and their keybar apply only to the
    /// list.
    pub(crate) fn commits_drilled(&self) -> bool {
        self.commit_drill.is_some()
    }

    /// Selection cursor for a given pane.
    pub(crate) fn selected(&self, pane: Pane) -> usize {
        self.selection[pane]
    }

    /// After an action that created `key`'s row, select it in `pane` as soon as
    /// a refresh lists it.
    pub(crate) fn select_when_listed(&mut self, pane: Pane, key: SelectionKey) {
        self.select_when_listed.retain(|(p, _)| *p != pane);
        self.select_when_listed.push((pane, key));
    }
}

/// What each pane had selected: its row index and the key of that row.
pub(crate) type Remembered = [(Pane, usize, Option<SelectionKey>); 5];

impl Nav {
    fn rows<'a>(&'a self, snapshot: &'a Snapshot, palette: &'a Palette) -> PaneRows<'a> {
        PaneRows {
            nav: self,
            snapshot,
            palette,
        }
    }

    /// Note what is selected in every pane, to find it again after a refresh.
    pub(crate) fn remember(&self, snapshot: &Snapshot, palette: &Palette) -> Remembered {
        let rows = self.rows(snapshot, palette);
        PANES.map(|pane| (pane, self.selection[pane], rows.selection_key(pane)))
    }

    /// Put each pane's cursor back on the row it had (by key), or on the same
    /// index when that row is gone, clamped to the new length. Rows an action
    /// just created and is waiting for are selected once they are listed.
    pub(crate) fn restore(&mut self, snapshot: &Snapshot, palette: &Palette, old: Remembered) {
        let rows = self.rows(snapshot, palette);
        let moved = old.map(|(pane, old_index, key)| {
            let last = rows.row_count(pane).saturating_sub(1);
            let index = key
                .as_ref()
                .and_then(|key| rows.find_selection_key(pane, key))
                .unwrap_or(old_index);
            (pane, index.min(last))
        });
        let mut found = Vec::new();
        let mut waiting = std::mem::take(&mut self.select_when_listed);
        waiting.retain(|(pane, key)| {
            let listed = self.rows(snapshot, palette).find_selection_key(*pane, key);
            if let Some(index) = listed {
                found.push((*pane, index));
            }
            listed.is_none()
        });
        self.select_when_listed = waiting;
        for (pane, index) in moved.into_iter().chain(found) {
            self.selection[pane] = index;
        }
    }

    /// A drilled branch log stays live across a background refresh instead of
    /// going stale; a branch that vanished (deleted, renamed) backs out of the
    /// drill-down instead of erroring the whole refresh.
    pub(crate) fn refresh_branch_log(&mut self, branch: &str, result: Shared<Vec<CommitEntry>>) {
        if self
            .branch_drill
            .as_ref()
            .is_none_or(|drill| drill.branch != branch)
        {
            return;
        }
        match result {
            Ok(commits) => {
                if let Some(drill) = &mut self.branch_drill {
                    drill.commits = commits;
                }
            },
            Err(_) => self.branch_drill = None,
        }
    }

    /// Same for a drilled commit's file tree: re-read so it reflects the diff
    /// as of this refresh; a commit that vanished (a reword or rebase changed
    /// its hash) backs out rather than erroring the refresh.
    pub(crate) fn refresh_commit_files(&mut self, hash: &str, result: Shared<Vec<FileEntry>>) {
        if self
            .commit_drill
            .as_ref()
            .is_none_or(|drill| drill.hash != hash)
        {
            return;
        }
        match result {
            Ok(files) => {
                if let Some(drill) = &mut self.commit_drill {
                    drill.files = files;
                }
            },
            Err(_) => self.commit_drill = None,
        }
    }
}

impl Nav {
    /// Is the Branches pane focused and showing its list of local branches?
    /// Not while drilled into a branch's log, where the selected row is a
    /// commit, nor on its Remotes tab.
    pub(crate) fn on_local_branches(&self) -> bool {
        self.focus == Pane::Branches
            && self.branch_drill.is_none()
            && self.branches_tab == BranchesTab::Local
    }

    /// `Ctrl-Right` / `Ctrl-Left`, Branches focused: switch its own Local
    /// branches / Remotes tab. A no-op while drilled into a branch's log, which
    /// has only one tab's worth of content.
    pub(crate) fn toggle_branches_tab(&mut self) {
        if self.focus != Pane::Branches || self.branch_drill.is_some() {
            return;
        }
        self.branches_tab = match self.branches_tab {
            BranchesTab::Local => BranchesTab::Remotes,
            BranchesTab::Remotes => BranchesTab::Local,
        };
    }

    /// Swap the Branches pane's list for `branch`'s commit history, in place:
    /// focus stays on Branches, only its rows and title change. `Esc` backs
    /// out to `return_index`.
    pub(crate) fn drill_into_branch(&mut self, branch: String, commits: Vec<CommitEntry>) {
        let return_index = self.selection[Pane::Branches];
        self.branch_drill = Some(BranchDrill {
            branch,
            commits,
            return_index,
        });
        self.selection[Pane::Branches] = 0;
    }

    /// Is the Stash pane focused in `Mode::Nav`?
    pub(crate) fn on_stash(&self) -> bool {
        self.focus == Pane::Stash && self.mode == Mode::Nav
    }

    /// Is the Files pane focused in `Mode::Nav`?
    pub(crate) fn on_files(&self) -> bool {
        self.focus == Pane::Files && self.mode == Mode::Nav
    }

    /// Is the Commits pane showing its list (not one commit's files) in
    /// `Mode::Nav`, where the rewrite keys act?
    pub(crate) fn on_commit_list(&self) -> bool {
        self.focus == Pane::Commits && self.mode == Mode::Nav && self.commit_drill.is_none()
    }
}

/// `(discriminant, diff text)` for cheap "did the right pane actually change"
/// checks: `String` equality on a few KB, no hashing.
/// The five left panes, in top-to-bottom screen order.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug, Enum)]
pub enum Pane {
    Status,
    /// Where ferrit opens, like lazygit.
    #[default]
    Files,
    Branches,
    Commits,
    Stash,
}

/// Panes in order. Index into this is also the index into `App::selection`.
pub const PANES: [Pane; 5] = [
    Pane::Status,
    Pane::Files,
    Pane::Branches,
    Pane::Commits,
    Pane::Stash,
];

impl Pane {
    /// Position in `PANES`, for the focus-cycling arithmetic in `pane_offset`.
    /// The variant order is the `PANES` order, so the discriminant is it.
    pub fn index(self) -> usize {
        self as usize
    }

    /// Bordered-box title, lazygit style: `[N] Tab - Tab - Tab`. The extra tab
    /// names are inert labels for now; only the first is a real view.
    pub fn title(self) -> &'static str {
        match self {
            Self::Status => "[1] Status",
            Self::Files => "[2] Files - Worktrees - Submodules",
            Self::Branches => "[3] Local branches - Remotes - Tags",
            Self::Commits => "[4] Commits - Reflog",
            Self::Stash => "[5] Stash",
        }
    }

    /// Contextual title for the right pane when this left pane has focus,
    /// matching what lazygit shows there.
    pub fn right_title(self) -> &'static str {
        match self {
            Self::Status => " Status ",
            Self::Files => " Unstaged changes ",
            Self::Branches => " Log ",
            Self::Commits => " Patch ",
            Self::Stash => " Stash ",
        }
    }
}

/// Which of the Branches pane's own two real tabs is showing (the third,
/// Tags, is still an inert label — `Pane::title`). `Remotes` has no
/// selection cursor of its own; it is `Repo::remotes()` rendered plainly,
/// same as the Local tab's list was for the entirety of phase 2 before
/// phase 8 made it actionable. `docs/PLAN_9_REMOTE.md`.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub(crate) enum BranchesTab {
    #[default]
    Local,
    Remotes,
}

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
enum TreeNode {
    Dir(BTreeMap<String, Self>),
    File(usize),
}

/// Group `files` by directory into a tree keyed by path component. A path
/// component that collides with an existing file entry (pathological: git
/// cannot really produce this) drops that one file rather than panicking.
fn build_file_tree(files: &[FileEntry]) -> BTreeMap<String, TreeNode> {
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

fn flatten_folded(
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

#[derive(Default)]
pub(crate) struct HitAreas {
    /// Each left pane's bordered rect, for routing a click to the pane it
    /// landed in.
    pub(crate) left: EnumMap<Pane, Rect>,
    /// `ListState::offset` for each left pane, copied back by
    /// `ui::draw_left_column` after `render_stateful_widget` moves it to keep
    /// the selection on screen. Lets a click in a scrolled list map to the
    /// right row. Only valid post-render; 0 before the first draw.
    pub(crate) list_offset: EnumMap<Pane, usize>,
    /// Per left pane, the selected row a wheel scroll left behind: while the
    /// selection is still that row, the view stays where the wheel put it,
    /// even with the selection off screen (lazygit). Any other selection
    /// re-attaches the view to it, so no key or click has to clear this.
    pub(crate) view_detached_at: EnumMap<Pane, Option<usize>>,
    /// The configured Git author in the bottom info panel.
    pub(crate) author: Rect,
    /// The visible Dashboard trigger beside the author.
    pub(crate) dashboard: Rect,
    /// Where the keybar was drawn and what each part of it runs when clicked.
    pub(crate) keybar: Rect,
    pub(crate) keybar_hits: Vec<KeybarHit>,
    /// The settings sheet's clickable parts.
    pub(crate) settings: SettingsHits,
}

impl HitAreas {
    pub(crate) fn list_offset(&self, pane: Pane) -> usize {
        self.list_offset[pane]
    }

    pub(crate) fn set_list_offset(&mut self, pane: Pane, offset: usize) {
        self.list_offset[pane] = offset;
    }

    /// Whether a wheel scroll left `pane`'s view away from `selected`, its
    /// selected row: the detachment ends when the selection moves.
    pub(crate) fn view_detached(&self, pane: Pane, selected: usize) -> bool {
        self.view_detached_at[pane] == Some(selected)
    }

    /// Scroll `pane`'s list by `rows` (negative is up) and keep its selection
    /// where it is, which may leave it off screen. `draw_left_column` clamps
    /// the offset to the list's length on the next frame.
    pub(crate) fn scroll_list(&mut self, pane: Pane, selected: usize, rows: isize) {
        self.view_detached_at[pane] = Some(selected);
        self.list_offset[pane] = self.list_offset[pane].saturating_add_signed(rows);
    }
}

impl HitAreas {
    /// Take in the parts of a frame's `Landed` that are about where things are.
    /// `selection` is each pane's selected row: a wheel scroll leaves the view
    /// where it put it only while the selection stays on the row it left behind.
    pub(crate) fn land(&mut self, landed: &mut Landed, selection: &EnumMap<Pane, usize>) {
        for &(pane, rect) in &landed.left {
            self.left[pane] = rect;
            if self.view_detached_at[pane] != Some(selection[pane]) {
                self.view_detached_at[pane] = None;
            }
        }
        for &(pane, offset) in &landed.list_offset {
            self.list_offset[pane] = offset;
        }
        if let Some(area) = landed.author {
            self.author = area;
        }
        if let Some(area) = landed.dashboard {
            self.dashboard = area;
        }
        if let Some((area, hits)) = landed.keybar.take() {
            self.keybar = area;
            self.keybar_hits = hits;
        }
        if let Some(hits) = landed.settings_hits.take() {
            self.settings = hits;
        }
    }
}

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

/// Colour each pane's rows by what they mean. Status and Files come from the
/// live snapshot on `App`; the rest are still mock.
fn pane_lines(app: &App, pane: Pane) -> Vec<Line<'static>> {
    match pane {
        Pane::Status => app.status_lines(),
        Pane::Files => app.file_lines(),
        Pane::Branches => app.branch_lines(),
        Pane::Commits => app.commit_lines(),
        Pane::Stash => app.stash_lines(),
    }
}

pub(crate) fn draw_left_column(frame: &mut Frame<'_>, app: &App, landed: &mut Landed, area: Rect) {
    let palette = &app.palette();
    // Status only ever shows 1 line, or 2 when there's a conflict to report
    // (`App::status_lines`): sized to that instead of a flat 4, so a short
    // terminal doesn't pay for a conflict line that (almost always) isn't
    // there.
    let status_height = u16::try_from(app.status_lines().len() + 2).unwrap_or(4);
    let [status_row, accordion_area] =
        Layout::vertical([Constraint::Length(status_height), Constraint::Min(0)]).areas(area);

    // lazygit's `expandFocusedSidePanel` accordion: the focused pane claims a
    // weighted majority of the space, everyone else shares what's left.
    // Weighted rather than "a fixed floor each, 100% of the leftover to
    // focus": that scheme gave a dramatic boost in a roomy terminal but fell
    // back to a perfectly even split — no accordion at all — the moment
    // there wasn't room for every pane's floor, which is exactly the short
    // terminal where showing one pane clearly, lazygit-style, matters most.
    // `FOCUS_WEIGHT` shares go to the focused pane, 1 share to each other;
    // when the focus is Status (outside this group), there is no pane to
    // boost, so every pane gets 1 share (an even split, not left blank).
    // Ratatui's `Fill`/`Min` mix is order-sensitive at small heights (it can
    // starve the boosted pane below its neighbours), so the split is
    // computed by hand rather than left to the `Layout` solver.
    const DYNAMIC: [Pane; 4] = [Pane::Files, Pane::Branches, Pane::Commits, Pane::Stash];
    const FOCUS_WEIGHT: u16 = 4;
    const MIN_HEIGHT: u16 = 2; // a collapsed but still-bordered box: no room for a content row
    let focus_index = DYNAMIC.iter().position(|&p| p == app.nav.focus);

    let weights: [u16; 4] = focus_index.map_or([1; 4], |idx| {
        std::array::from_fn(|i| if i == idx { FOCUS_WEIGHT } else { 1 })
    });
    let total_weight: u16 = weights.iter().sum();
    let mut heights: [u16; 4] = std::array::from_fn(|i| {
        let weight = weights.get(i).copied().unwrap_or(1);
        (accordion_area.height * weight / total_weight).max(MIN_HEIGHT)
    });

    // The weighted shares rarely sum to exactly `accordion_area.height`,
    // especially once every pane is floored to `MIN_HEIGHT`. Round-robin the
    // remainder (or the overshoot) so the total always matches exactly,
    // never taking a pane below 0.
    let mut diff = i32::from(accordion_area.height) - i32::from(heights.iter().sum::<u16>());
    let mut i = 0;
    while diff != 0 {
        let Some(h) = heights.get_mut(i) else { break };
        if diff > 0 {
            *h += 1;
            diff -= 1;
        } else if *h > 0 {
            *h -= 1;
            diff += 1;
        }
        i = (i + 1) % heights.len();
    }

    let mut rows = [
        status_row,
        Rect::default(),
        Rect::default(),
        Rect::default(),
        Rect::default(),
    ];
    let mut y = accordion_area.y;
    for (i, &h) in heights.iter().enumerate() {
        if let Some(row) = rows.get_mut(i + 1) {
            *row = Rect {
                x: accordion_area.x,
                y,
                width: accordion_area.width,
                height: h,
            };
        }
        y += h;
    }

    for (&pane, &row) in PANES.iter().zip(&rows) {
        // Remembered for click routing: written before the list body is
        // read, so this `&mut` borrow never overlaps the `&self` one below.
        landed.left.push((pane, row));

        let focused = app.nav.focus == pane && !app.right_focused();
        let border = if focused {
            Style::new()
                .fg(app.theme.config.color())
                .add_modifier(Modifier::BOLD)
        } else {
            Style::new().fg(palette.idle)
        };
        let title_text = if pane == Pane::Branches {
            app.branches_title()
        } else if pane == Pane::Commits {
            app.commits_title()
        } else {
            pane.title().to_owned()
        };
        let title = Line::styled(
            format!(" {title_text} "),
            if focused {
                Style::new()
                    .fg(app.theme.config.color())
                    .add_modifier(Modifier::BOLD)
            } else {
                Style::new().fg(palette.idle)
            },
        );

        let mut panel = Panel::new().title(title).border_style(border);
        if let Some((cur, total)) = app.counter(pane) {
            panel = panel.bottom_title(row_lines::counter_line(palette, cur, total));
        }
        let block = panel.block();

        let row_ct = app.row_count(pane);
        let mut lines = pane_lines(app, pane);
        let mut highlight = row_lines::selection_style(palette, focused);
        if pane == Pane::Files && focused {
            // Files rows carry a staging colour that the bar must not repaint.
            highlight.fg = None;
            if let Some(line) = lines.get_mut(app.selected(pane)) {
                row_lines::keep_colours_on_selection(palette, line);
            }
        }
        let detached = app.view_detached(pane);
        let offset = PaneList::new(lines, block)
            .detached(detached)
            .selected((row_ct > 0).then(|| app.selected(pane).min(row_ct - 1)))
            .offset(app.list_offset(pane))
            .highlight_style(highlight)
            .scrollbar_style(border)
            .render(frame, row);
        // Ratatui may have moved the offset to keep the selection on screen;
        // copy it back so a click in a scrolled list maps to the right row.
        landed.list_offset.push((pane, offset));
    }
}
