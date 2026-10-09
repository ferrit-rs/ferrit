//! What the screens and the tests are shown: the right pane's diff, the popups and the menus, as plain data.

use super::create_remote;
use crate::components::tui_overlay::state::OverlayState;
use crate::components::ui::text_input::TextInput;
use crate::git;

/// What the right pane shows behind the image preview. A second cached,
/// rebuilt-on-nav value alongside `preview`, not a replacement: an image
/// selection still wins. See `docs/PLAN_3_DIFF_VIEW.md`.
#[derive(Debug, Clone, Default)]
pub enum DiffView {
    /// Status / Stash focused: no real diff, the mock text shows.
    #[default]
    None,
    /// Read failed, or nothing to show. A dim single line, never a panic.
    Note(String),
    /// Files pane: one file's `git diff`, both sides at once (lazygit's own
    /// Unstaged Changes / Staged Changes split).
    Files(FilesDiff),
    /// Commits pane, or a drilled branch's log: one commit's metadata and
    /// `git show` diff.
    Commit(git::model::CommitEntry, git::diff::Diff),
    /// Stash pane: the selected entry's `git stash show -p`, scrolled and
    /// rendered like a commit diff (`docs/PLAN_10_STASH.md`).
    Stash(git::model::StashEntry, git::diff::Diff),
    /// Branches pane, not drilled in: the selected branch's own log, shown
    /// passively (no Enter needed), lazygit's live branch -> log preview.
    BranchLog(BranchLog),
}

/// A Files-pane selection's two sides at once, lazygit's own Unstaged
/// Changes / Staged Changes split: a file half-staged shows real content in
/// both, a file entirely on one side shows an empty diff on the other.
#[derive(Debug, Clone)]
pub struct FilesDiff {
    pub unstaged: git::diff::Diff,
    pub staged: git::diff::Diff,
}

/// A branch's own commit log for the passive `DiffView::BranchLog` preview.
#[derive(Debug, Clone)]
pub struct BranchLog {
    pub branch: String,
    pub commits: Vec<git::model::CommitEntry>,
}

/// Read-only view of an editor popup for `ui::draw_commit_popup`
/// (`docs/PLAN_7_COMMIT.md`), reused as-is for the new-branch popup
/// (`docs/PLAN_8_BRANCHES.md`) — same shape, different title/footer.
/// Borrows the draft's lines, so it is cheap to build fresh every frame
/// rather than cached.
pub struct CommitPopupView<'a> {
    pub title: &'a str,
    /// Commit subject, or the whole single-field input for another popup.
    pub input: &'a TextInput,
    /// Commit body editor; absent for the new-branch input.
    pub description: Option<&'a TextInput>,
    pub summary_focused: bool,
    pub overlay_state: Option<&'a mut OverlayState>,
    /// Compatibility view of the component's text lines.
    pub lines: &'a [String],
    /// `(row, character column)` cursor position.
    pub cursor: (usize, usize),
    /// `Some((sign_off, no_verify))` for the commit popup's toggle line;
    /// `None` for the new-branch popup, which has nothing to toggle.
    pub toggles: Option<(bool, bool)>,
    /// The commit popup's author line (`author: Name <email>`); `None` for the
    /// other popups and for a reword, which keeps the commit's own author.
    pub author: Option<String>,
    /// Footer key hints, e.g. `"Commit: Ctrl-S | ... | Cancel: Esc"`.
    pub hints: &'static str,
}

/// The one active popup, ready for rendering. Explicit variants keep popup
/// precedence in one `match` instead of chaining `if let` checks.
pub enum PopupView<'a> {
    Commit(CommitPopupView<'a>),
    /// The "stage everything?" question. Carries the backdrop's animation only when
    /// the view is for drawing (`popup_view_with`).
    CommitAllConfirm(Option<&'a mut OverlayState>),
    NewBranch(CommitPopupView<'a>),
    Stash(CommitPopupView<'a>),
    Name(CommitPopupView<'a>),
    CommandLog(CommandLogView),
    Menu(MenuView),
    Upstream(CommitPopupView<'a>),
    Askpass(CommitPopupView<'a>),
    CreateRemote(create_remote::CreateRemoteView<'a>),
    Note(&'a str),
}

/// The `@` viewer's render data: every recorded command and how far the view
/// is scrolled up from the newest.
#[derive(Debug)]
pub struct CommandLogView {
    pub records: Vec<git::command_log::CommandRecord>,
    pub from_bottom: usize,
}

/// A menu's render data: the title, one line per row, and the highlighted
/// row.
#[derive(Debug)]
pub struct MenuView {
    pub title: String,
    pub rows: Vec<String>,
    pub selected: usize,
    /// What the highlighted row does; empty when the menu has no hints.
    pub hint: &'static str,
}
