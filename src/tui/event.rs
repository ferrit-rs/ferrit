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
use crate::tui::components::diff::{Mode, RightPane};
use crate::tui::components::panes::{Nav, PaneRows};
use crate::tui::components::panes::{Pane, SelectionKey};
use crate::tui::components::popups::ConfirmPrompt;
use crate::tui::components::popups::Popup;
use crate::tui::components::stash;
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
    /// Choose a value on the git config screen's menu.
    PickConfigValue(usize),
    /// Open the create-a-repository flow.
    OpenCreateRemote,
    /// A config key or value was typed: hand it to the git config screen.
    SubmitGitConfigName {
        /// Key or value.
        kind: crate::tui::components::menu::NameKind,
        /// What was typed, spaces kept.
        text: String,
    },
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
    OperationStep(crate::git::operation::Step),
    /// The first global git config write was confirmed: go on with the edit.
    ResumeGitConfigEdit(crate::git::config_edit::GlobalResume),
    /// `git init` in this folder.
    InitRepo(std::path::PathBuf),
    /// Unset a git config value.
    ConfigUnset(crate::git::config_edit::ConfigOp),
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
            right: &self.right,
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
                Event::EnterDiff {
                    has_worktree_change,
                } => {
                    if self.right.start_cursor(has_worktree_change) {
                        self.nav.mode = Mode::Diff;
                    }
                },
                Event::LeaveDiff => self.nav.mode = Mode::Nav,
                Event::WelcomeSelected(row) => self.full_screens.welcome_selected = row,
                Event::Quit => self.should_quit = true,
                Event::PickConfigValue(index) => self.pick_config_value(index),
                Event::OpenCreateRemote => self.open_create_remote(),
                Event::SubmitGitConfigName { kind, text } => {
                    if self.submit_git_config_name(&kind, &text) {
                        self.modal.close_popup();
                    }
                },
                Event::ClearAnchor => self.right.cursor.anchor = None,
                Event::RestoreStash { oid, pop } => {
                    let first_file = self.right.diff.first_stash_file(&oid);
                    if let Some(repo) = &mut self.repo {
                        let events = stash::restore(&oid, pop, repo.as_mut(), first_file);
                        self.apply(events);
                    }
                },
                Event::DropStash(oid) => {
                    if let Some(repo) = &mut self.repo {
                        let events = stash::drop_entry(&oid, repo.as_mut());
                        self.apply(events);
                    }
                },
                Event::StartRemote(request) => {
                    if let Some(sender) = self.workers.sender.clone() {
                        self.start_remote(request, sender);
                    }
                },
                Event::OperationStep(step) => self.apply_operation_step(step),
                Event::ResumeGitConfigEdit(resume) => self.resume_git_config_edit(resume),
                Event::InitRepo(dir) => self.init_here(&dir),
                Event::ConfigUnset(op) => self.confirm_git_config_unset(&op),
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

impl App {
    /// A popup asking for a name, for the screens that are not migrated yet.
    pub(crate) fn open_name(
        &mut self,
        kind: crate::tui::components::menu::NameKind,
        title: String,
        input: crate::tui::widgets::text_input::TextInput,
    ) {
        self.apply(vec![crate::tui::components::menu::open_name(
            kind, title, input,
        )]);
    }

    /// Run one step of the merge, rebase, cherry-pick or revert and say where
    /// git stopped.
    pub(crate) fn apply_operation_step(&mut self, step: crate::git::operation::Step) {
        let Some(repo) = &self.repo else { return };
        let result = repo.operation_step(step);
        self.finish_operation(result);
    }

    /// Refresh and report where git stopped after a step or a rewrite.
    /// Refreshes either way: a refusal changes nothing, a step changes a lot.
    pub(crate) fn finish_operation(&mut self, result: GitResult<OperationOutcome>) {
        self.request_refresh();
        match result {
            Ok(OperationOutcome::Done) => {},
            Ok(OperationOutcome::Stopped { conflicted: true }) => {
                self.modal.open_popup(Popup::Note(
                    "stopped on a conflict. Resolve it in Files, then press m and Continue."
                        .to_owned(),
                ));
            },
            Ok(OperationOutcome::Stopped { conflicted: false }) => {
                self.modal.open_popup(Popup::Note(
                    "stopped for you to edit. Make your change, then press m and Continue."
                        .to_owned(),
                ));
            },
            Err(e) => self.report_error(e),
        }
    }
}
