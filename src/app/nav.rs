//! Where the user is in the panes: which one has focus, the cursor in each,
//! the drill-downs, the Branches tab and the keyboard mode. Moving the cursor,
//! drilling in and out and re-finding a row after a refresh are `App` methods
//! (`drill_nav`, `branch_actions`, `tree`); this is the state they share.

use std::collections::HashSet;
use std::path::PathBuf;

use enum_map::EnumMap;

use super::{BranchDrill, BranchesTab, CommitDrill, Mode, Pane, SelectionKey};

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
    pub(super) collapsed_dirs: HashSet<PathBuf>,
    /// `Some` while the Branches pane is drilled into one branch's own log
    /// (Enter on a branch, `Esc` to back out); `None` shows the branch list.
    pub(super) branch_drill: Option<BranchDrill>,
    /// Which of the Branches pane's own two tabs is showing.
    /// `Ctrl-Right`/`Ctrl-Left` switch it, Branches focused.
    pub(super) branches_tab: BranchesTab,
    /// `Some` while the Commits pane is drilled into one commit's own
    /// changed-file tree (Enter on a commit, `Esc` to back out); `None`
    /// shows the commit list.
    pub(super) commit_drill: Option<CommitDrill>,
    /// Rows an action just created (the new branch, the new `HEAD`) that the
    /// selection moves to once a refresh lists them, as lazygit does. Kept
    /// until found, so a refresh already in flight when the action ran, which
    /// cannot list them yet, does not lose it.
    pub(super) select_when_listed: Vec<(Pane, SelectionKey)>,
    /// A click landed on the right pane. Purely a border-highlight flag for
    /// now (see `docs/PLAN_5_CLICK_BEHAVIOR.md`, "right-pane-focus plan");
    /// left-pane navigation and selection are untouched. Cleared by `Esc` or
    /// a click back on a left pane.
    pub(super) right_focused: bool,
    /// Whether keys go to the left panes or to the Files diff cursor
    /// (`docs/PLAN_6_STAGING.md`).
    pub(super) mode: Mode,
}
