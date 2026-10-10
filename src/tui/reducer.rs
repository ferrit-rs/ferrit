//! Apply component intents and runtime outcomes to application state.
//!
//! This is the single mutation boundary for component events.

use crate::git::error::GitResult;
use crate::git::rebase::OperationOutcome;
use crate::git::staging;
use crate::tui::App;
use crate::tui::components::dashboard::Sheet;
use crate::tui::components::dashboard::state::Dashboard;
use crate::tui::components::dashboard::state::StatsCompletion;
use crate::tui::components::dashboard::state::StatsCtx;
use crate::tui::components::diff::right_pane::Mode;
use crate::tui::components::menu::{self, NameKind};
use crate::tui::components::panes::nav::Pane;
use crate::tui::components::popups::Popup;
use crate::tui::components::{branches, remote, stash};
use crate::tui::draw::FullScreen;

use crate::tui::event::{Env, Event};

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
                Event::CloseSheet => self.render.sheet.close(),
                Event::ToggleFilesDir(path) => {
                    if !self.nav.collapsed_dirs.remove(&path) {
                        self.nav.collapsed_dirs.insert(path);
                    }
                    let last = self.row_count(Pane::Files).saturating_sub(1);
                    self.nav.selection[Pane::Files] = self.nav.selection[Pane::Files].min(last);
                },
                Event::ToggleCommitDir(path) => {
                    if let Some(drill) = &mut self.nav.commit_drill
                        && !drill.collapsed.remove(&path)
                    {
                        drill.collapsed.insert(path);
                    }
                    let last = self.row_count(Pane::Commits).saturating_sub(1);
                    self.nav.selection[Pane::Commits] = self.nav.selection[Pane::Commits].min(last);
                },
                Event::DrillIntoCommit(drill) => {
                    self.nav.commit_drill = Some(drill);
                    self.nav.selection[Pane::Commits] = 0;
                },
                Event::CloseDashboard => self.close_dashboard(),
                Event::OpenHelp => self.open_help(),
                Event::ShowGitConfig => self.full_screens.active = FullScreen::GitConfig,
                Event::HideGitConfig => self.full_screens.active = FullScreen::None,
                Event::HidePointer => self.mouse_pointer.request(false),
                Event::PickConfigValue(index) => {
                    let mut config = self.git_config_ctx();
                    config.pick_value(index);
                    let events = config.events;
                    self.apply(events);
                },
                Event::OpenCreateRemote => self.open_create_remote(),
                Event::SubmitNewBranch(name) => {
                    let events = branches::create(&name, &self.env());
                    self.apply(events);
                },
                Event::PushStash(message) => {
                    let events = stash::push(&message, self.repo.as_deref());
                    self.apply(events);
                },
                Event::SubmitName { kind, text } => {
                    if matches!(kind, NameKind::ConfigKey | NameKind::ConfigValue(_)) {
                        // A value keeps its spaces; a refusal keeps the popup.
                        let mut config = self.git_config_ctx();
                        let close = config.submit_name(&kind, &text);
                        let mut events = config.events;
                        if close {
                            events.push(Event::ClosePopup);
                        }
                        self.apply(events);
                    } else if let Some(repo) = &mut self.repo {
                        let events = menu::submit_name(&kind, &text, repo.as_mut());
                        self.apply(events);
                    }
                },
                Event::SubmitUpstream(value) => {
                    let events = remote::submit_upstream(&value);
                    self.apply(events);
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
                Event::ResumeGitConfigEdit(resume) => {
                    let mut config = self.git_config_ctx();
                    config.resume(resume);
                    let events = config.events;
                    self.apply(events);
                },
                Event::InitRepo(dir) => self.init_here(&dir),
                Event::ConfigUnset(op) => {
                    let mut config = self.git_config_ctx();
                    config.confirm_unset(&op);
                    let events = config.events;
                    self.apply(events);
                },
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
    /// Run one step of the merge, rebase, cherry-pick or revert and say where
    /// git stopped.
    pub(crate) fn apply_operation_step(&mut self, step: crate::git::rebase::Step) {
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

impl App {
    /// `D`: show the dashboard over the panes, computing what is not cached.
    pub fn open_dashboard(&mut self) {
        self.open_sheet(Sheet::Dashboard);
    }

    /// Back to the panes. A running computation is told to stop; its result, if
    /// it still arrives, is kept only when it is good.
    pub fn close_dashboard(&mut self) {
        self.render.sheet.close();
        self.sheets.dashboard.cancel_running();
    }

    /// Open `sheet` in the drawer, ready for its first frame.
    pub(crate) fn open_sheet(&mut self, sheet: Sheet) {
        self.sheets.kind = sheet;
        match sheet {
            Sheet::Settings => self.settings_ctx().prepare(),
            Sheet::Dashboard => self.with_dashboard(Dashboard::prepare),
        }
        self.render.sheet.open();
    }

    /// Run `f` on the dashboard with what it needs of the app.
    pub(crate) fn with_dashboard<R>(
        &mut self,
        f: impl FnOnce(&mut Dashboard, &StatsCtx<'_>) -> R,
    ) -> R {
        let open = self.dashboard_is_open();
        let ctx = StatsCtx {
            snapshot: &self.snapshot,
            repo: self.repo.as_deref(),
            sender: self.workers.sender.clone(),
            open,
        };
        f(&mut self.sheets.dashboard, &ctx)
    }

    /// `AppEvent::StatsDone` arrived.
    pub(crate) fn on_stats_done(&mut self, completion: StatsCompletion) {
        let open = self.dashboard_is_open();
        self.sheets.dashboard.on_done(completion, open);
    }
}
