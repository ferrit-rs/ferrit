//! What a component asks the app to do. A component decides from what it is
//! given and returns events; `App::apply` is the one place that changes the
//! state, so a component never reaches into another.

use crate::git::Snapshot;
use crate::git::commit::CommitKind;
use crate::git::error::GitResult;
use crate::git::model::CommitEntry;
use crate::git::operation::OperationOutcome;
use crate::git::port::GitPort;
use crate::git::staging;
use crate::theme::palette::Palette;
use crate::tui::App;
use crate::tui::components::panes::{Nav, PaneRows};
use crate::tui::components::panes::{Pane, SelectionKey};
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
    palette: &'a Palette,
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

impl App {
    /// What a component may read to decide.
    pub(crate) fn env(&self) -> Env<'_> {
        Env {
            nav: &self.nav,
            snapshot: &self.snapshot,
            repo: self.repo.as_deref(),
            popup_up: self.modal.popup().is_some(),
            modal_up: self.modal.is_some(),
            author: self.authorship.author_arg(),
            palette: &self.prefs.palette,
        }
    }

    /// Make the changes components asked for, in order.
    pub(crate) fn apply(&mut self, events: Vec<Event>) {
        for event in events {
            match event {
                Event::OpenPopup(popup) => self.modal.open_popup(popup),
                Event::ClosePopup => self.modal.close_popup(),
                Event::CommitAnimation(open) => {
                    if open {
                        self.render.commit.open();
                    } else {
                        self.render.commit.close();
                    }
                },
                Event::Refresh => self.request_refresh(),
                Event::Report(error) => self.report_error(error),
                Event::SelectWhenListed(pane, key) => {
                    self.nav.select_when_listed(pane, key);
                },
                Event::KeepCommitDraft(draft) => self.commit_draft = draft,
                Event::CycleAuthor => self.authorship.cycle(),
                Event::FinishOperation(result) => self.finish_operation(result),
                Event::Ask(prompt) => self.modal.ask(prompt),
                Event::Focus(pane) => self.nav.focus = pane,
                Event::FinishAction(result) => {
                    self.request_refresh();
                    if let Err(error) = result {
                        self.report_error(error);
                    }
                },
                Event::Notice(message) => self.report_notice(message),
                Event::OpenCommit(kind) => self.open_commit(kind),
                Event::OpenReword {
                    hash,
                    title,
                    message,
                } => self.open_reword_editor(hash, title, &message),
                Event::DrillIntoBranch { branch, commits } => {
                    self.nav.drill_into_branch(branch, commits);
                },
                Event::NewBranchTitle(title) => self.new_branch_title = title,
                Event::MergeConflicted => {
                    let files = staging::conflicted_paths(&self.snapshot.files).join(", ");
                    self.modal.open_popup(Popup::Note(format!(
                        "merge conflict in {files}. Fix the files and stage them with <space>, \
                         then press m and choose Continue, or Abort."
                    )));
                },
            }
        }
    }
}
