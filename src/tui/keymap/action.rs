//! Actions available outside popups.

use crate::tui::components::panes::nav::Pane;

/// Everything a key can do outside a popup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Quit,
    Help,
    CommandLog,
    /// The full-screen repository dashboard.
    Dashboard,
    /// The full-screen git config editor.
    GitConfig,
    /// Create the GitHub repository.
    CreateRemote,
    OperationMenu,
    /// The menu of extra actions for the selected row.
    ContextMenu,
    /// Leave a drill or the right pane.
    Back,
    /// Drill into the selection, toggle a directory, or focus the diff.
    Enter,
    EnterDiff,
    Refresh,
    Fetch,
    Pull,
    Push,
    Commit,
    Amend,
    /// Reword `HEAD`'s message only.
    RewordHead,
    Focus(Pane),
    NextPane,
    PrevPane,
    ToggleBranchesTab,
    SelectDown,
    SelectUp,
    ScrollLineDown,
    ScrollLineUp,
    ScrollPageDown,
    ScrollPageUp,
    ScrollHalfDown,
    ScrollHalfUp,
    ScrollTop,
    ScrollBottom,
    NextHunk,
    PrevHunk,
    StageFile,
    StageAll,
    Discard,
    StashPush,
    LeaveDiff,
    CursorDown,
    CursorUp,
    CursorNextHunk,
    CursorPrevHunk,
    ToggleSelection,
    StageCursor,
    Checkout,
    NewBranch,
    FastForward,
    Merge,
    DeleteBranch,
    /// Reword the selected commit.
    RewordCommit,
    DropCommit,
    Squash,
    Fixup,
    EditCommit,
    NewFixup,
    Autosquash,
    ApplyStash,
    PopStash,
    DropStash,
}

impl Action {
    /// Name used in `[keys.<context>]`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Quit => "quit",
            Self::Help => "help",
            Self::CommandLog => "command_log",
            Self::Dashboard => "dashboard",
            Self::GitConfig => "git_config",
            Self::CreateRemote => "create_remote",
            Self::OperationMenu => "operation_menu",
            Self::ContextMenu => "context_menu",
            Self::Back => "back",
            Self::Enter => "enter",
            Self::EnterDiff => "enter_diff",
            Self::Refresh => "refresh",
            Self::Fetch => "fetch",
            Self::Pull => "pull",
            Self::Push => "push",
            Self::Commit => "commit",
            Self::Amend => "amend",
            Self::RewordHead => "reword_head",
            Self::Focus(Pane::Status) => "focus_status",
            Self::Focus(Pane::Files) => "focus_files",
            Self::Focus(Pane::Branches) => "focus_branches",
            Self::Focus(Pane::Commits) => "focus_commits",
            Self::Focus(Pane::Stash) => "focus_stash",
            Self::NextPane => "next_pane",
            Self::PrevPane => "prev_pane",
            Self::ToggleBranchesTab => "toggle_branches_tab",
            Self::SelectDown => "select_down",
            Self::SelectUp => "select_up",
            Self::ScrollLineDown => "scroll_line_down",
            Self::ScrollLineUp => "scroll_line_up",
            Self::ScrollPageDown => "scroll_page_down",
            Self::ScrollPageUp => "scroll_page_up",
            Self::ScrollHalfDown => "scroll_half_down",
            Self::ScrollHalfUp => "scroll_half_up",
            Self::ScrollTop => "scroll_top",
            Self::ScrollBottom => "scroll_bottom",
            Self::NextHunk => "next_hunk",
            Self::PrevHunk => "prev_hunk",
            Self::StageFile => "stage_file",
            Self::StageAll => "stage_all",
            Self::Discard => "discard",
            Self::StashPush => "stash_push",
            Self::LeaveDiff => "leave_diff",
            Self::CursorDown => "cursor_down",
            Self::CursorUp => "cursor_up",
            Self::CursorNextHunk => "cursor_next_hunk",
            Self::CursorPrevHunk => "cursor_prev_hunk",
            Self::ToggleSelection => "toggle_selection",
            Self::StageCursor => "stage_cursor",
            Self::Checkout => "checkout",
            Self::NewBranch => "new_branch",
            Self::FastForward => "fast_forward",
            Self::Merge => "merge",
            Self::DeleteBranch => "delete_branch",
            Self::RewordCommit => "reword_commit",
            Self::DropCommit => "drop_commit",
            Self::Squash => "squash",
            Self::Fixup => "fixup",
            Self::EditCommit => "edit_commit",
            Self::NewFixup => "new_fixup",
            Self::Autosquash => "autosquash",
            Self::ApplyStash => "apply_stash",
            Self::PopStash => "pop_stash",
            Self::DropStash => "drop_stash",
        }
    }

    /// Find an action by its config name.
    pub fn from_name(name: &str) -> Option<Self> {
        crate::tui::keymap::defaults::entries()
            .iter()
            .map(|&(_, _, action)| action)
            .find(|action| action.name() == name)
    }
}
