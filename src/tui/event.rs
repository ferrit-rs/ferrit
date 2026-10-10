//! Component intents and the read-only environment they receive.
//!
//! Components decide from immutable input and return events. The reducer owns
//! state mutation.

use crate::git::Snapshot;
use crate::git::commit::CommitKind;
use crate::git::error::GitResult;
use crate::git::model::CommitEntry;
use crate::git::port::GitPort;
use crate::git::rebase::OperationOutcome;
use crate::theme::palette::Palette;
use crate::tui::components::diff::right_pane::RightPane;
use crate::tui::components::menu::NameKind;
use crate::tui::components::panes::nav::Nav;
use crate::tui::components::panes::nav::Pane;
use crate::tui::components::panes::rows::PaneRows;
use crate::tui::components::panes::selection::SelectionKey;
use crate::tui::components::popups::ConfirmPrompt;
use crate::tui::components::popups::Popup;
use crate::tui::error::AppError;

/// One change a component asks for.
pub(crate) enum Event {
    /// Put this popup over the panes.
    OpenPopup(Popup),
    /// Take the popup away.
    ClosePopup,
    /// Start (`true`) or end the commit editor's slide animation.
    CommitAnimation(bool),
    /// Read the repository again.
    Refresh,
    /// Tell the user what went wrong.
    Report(AppError),
    /// Select this row once a refresh lists it.
    SelectWhenListed(Pane, SelectionKey),
    /// Keep (or forget) the text of a cancelled commit editor.
    KeepCommitDraft(Option<String>),
    /// Pick the next commit identity.
    CycleAuthor,
    /// Show where a rebase or a rewrite stopped.
    FinishOperation(GitResult<OperationOutcome>),
    /// Ask a question before going on.
    Ask(ConfirmPrompt),
    /// Move the focus to this pane.
    Focus(Pane),
    /// A git action finished: read the repository again either way, and say why
    /// when it failed.
    FinishAction(GitResult<()>),
    /// Say something short that is not an error.
    Notice(String),
    /// Open the commit editor for this kind of commit.
    OpenCommit(CommitKind),
    /// Open the commit editor on an older commit, to reword it with a rebase.
    OpenReword {
        /// The commit's full hash.
        hash: String,
        /// The popup's title.
        title: String,
        /// The message the editor starts from.
        message: String,
    },
    /// Replace the Branches list with this branch's commits.
    DrillIntoBranch {
        /// The branch's name.
        branch: String,
        /// Its history.
        commits: Vec<CommitEntry>,
    },
    /// The new-branch popup's title.
    NewBranchTitle(String),
    /// A merge stopped on conflicts: name the files, as the refresh now lists them.
    MergeConflicted,
    /// Put the line cursor on the file's first selectable line and take the keys.
    EnterDiff {
        /// Start on the worktree side (else on the staged one).
        has_worktree_change: bool,
    },
    /// Give the keys back to the panes.
    LeaveDiff,
    /// The welcome screen's highlighted choice.
    WelcomeSelected(usize),
    /// Leave the program.
    Quit,
    /// Slide the side sheet out.
    CloseSheet,
    /// Collapse or expand a directory row of the Files tree.
    ToggleFilesDir(std::path::PathBuf),
    /// Collapse or expand a directory row of a drilled commit's tree.
    ToggleCommitDir(std::path::PathBuf),
    /// Replace the Commits list with one commit's changed files.
    DrillIntoCommit(crate::tui::components::panes::drills::CommitDrill),
    /// Back from the dashboard to the panes, telling its computation to stop.
    CloseDashboard,
    /// Open the help screen.
    OpenHelp,
    /// Show the git config screen.
    ShowGitConfig,
    /// Leave the git config screen.
    HideGitConfig,
    /// The mouse pointer leaves a clickable thing.
    HidePointer,
    /// Choose a value on the git config screen's menu.
    PickConfigValue(usize),
    /// Open the create-a-repository flow.
    OpenCreateRemote,
    /// The new-branch popup was submitted with this name.
    SubmitNewBranch(String),
    /// The stash popup was submitted with this message.
    PushStash(String),
    /// A name popup was submitted.
    SubmitName {
        /// What it was for.
        kind: NameKind,
        /// What was typed, spaces kept.
        text: String,
    },
    /// The upstream prompt was submitted.
    SubmitUpstream(String),
    /// Forget the start of a V-selection.
    ClearAnchor,
    /// Apply or pop a stash entry (needs the repository for writing).
    RestoreStash {
        /// The entry's oid.
        oid: String,
        /// Pop instead of apply.
        pop: bool,
    },
    /// Drop a stash entry.
    DropStash(String),
    /// Run a network operation in the background.
    StartRemote(crate::git::remote::RemoteRequest),
    /// Carry on a merge, rebase, cherry-pick or revert, or abort it.
    OperationStep(crate::git::rebase::Step),
    /// The first global git config write was confirmed: go on with the edit.
    ResumeGitConfigEdit(crate::tui::components::git_config::edit::GlobalResume),
    /// `git init` in this folder.
    InitRepo(std::path::PathBuf),
    /// Unset a git config value.
    ConfigUnset(crate::tui::components::git_config::edit::ConfigOp),
}

/// What a component may read of the app to decide: the model, and nothing it
/// could change.
pub(crate) struct Env<'a> {
    /// Where the user is in the panes.
    pub(crate) nav: &'a Nav,
    /// What the repository looked like at the last refresh.
    pub(crate) snapshot: &'a Snapshot,
    /// The repository, if there is one.
    pub(crate) repo: Option<&'a dyn GitPort>,
    /// A popup is up.
    pub(crate) popup_up: bool,
    /// A popup or a question is up.
    pub(crate) modal_up: bool,
    /// `Name <email>` for `--author`, from ferrit's identity pick.
    pub(crate) author: Option<String>,
    /// The right column: what it shows and where the line cursor is.
    pub(crate) right: &'a RightPane,
    pub(crate) palette: &'a Palette,
}

impl<'a> Env<'a> {
    /// The panes' rows, to read the selection.
    pub(crate) fn rows(&self) -> PaneRows<'a> {
        PaneRows {
            nav: self.nav,
            snapshot: self.snapshot,
            palette: self.palette,
        }
    }
}
