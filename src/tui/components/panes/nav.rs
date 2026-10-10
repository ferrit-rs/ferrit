//! The five left panes: `nav`.

use crate::git::Snapshot;
use crate::git::model::{CommitEntry, FileEntry};
use crate::theme::palette::Palette;
use crate::tui::components::diff::right_pane::Mode;
use crate::tui::components::panes::drills::BranchDrill;
use crate::tui::components::panes::drills::CommitDrill;
use crate::tui::components::panes::rows::PaneRows;
use crate::tui::components::panes::tree::SelectionKey;
use crate::tui::workers::Shared;
use enum_map::{Enum, EnumMap};
use std::collections::HashSet;
use std::path::PathBuf;

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
    /// `enum-map`'s enum index follows the variant order used by `PANES`.
    pub fn index(self) -> usize {
        self.into_usize()
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
