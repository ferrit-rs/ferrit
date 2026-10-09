//! The Files pane: stage, unstage, discard.

use crate::git;
use crate::git::diff::DiffSide;
use crate::git::error::GitResult;
use crate::git::model::Change;
use crate::git::rebase::RebaseEdit;
use crate::git::remote::RemoteRequest;
use crate::git::staging::{self, Plan, Refusal};
use crate::tui::App;
use crate::tui::components::diff::Mode;
use crate::tui::components::panes::{FileRow, Pane};
use crate::tui::components::popups::{ConfirmAction, ConfirmPrompt};
use crate::tui::components::{commits, stash};
use crate::tui::error::AppError;
use std::ops::Range;

impl App {
    /// `Enter` / `l` on a Files-pane file row (`Mode::Nav`): focus the diff
    /// for staging within it. A no-op off the Files pane, on a directory
    /// row, already in `Mode::Diff`, or when neither side has a selectable
    /// line to put the cursor on: those stage whole-file only, from `Mode::Nav`.
    pub(crate) fn enter_diff_mode(&mut self) {
        if self.nav.focus != Pane::Files || self.nav.mode == Mode::Diff {
            return;
        }
        let Some(entry) = self.rows().selected_file() else {
            return;
        };
        if self.right.start_cursor(entry.worktree != Change::None) {
            self.nav.mode = Mode::Diff;
        }
    }

    /// `Esc` / `h` in `Mode::Diff`: back to `Mode::Nav`.
    pub(crate) fn leave_diff_mode(&mut self) {
        self.nav.mode = Mode::Nav;
    }

    /// Refresh after a stage / unstage / discard, then surface a failure in
    /// the Status pane.
    pub(crate) fn finish_apply(&mut self, result: GitResult<()>) {
        self.request_refresh();
        if let Err(e) = result {
            self.report_error(e);
        }
    }

    /// Make a stage's call, then refresh and surface a failure in the Status
    /// pane. `git apply` is atomic per invocation, so a failure leaves the
    /// repository exactly as it was; the refresh still runs so a failed attempt
    /// (context drift from an external edit) re-reads the current diff for the
    /// retry (`docs/PLAN_6_STAGING.md` "apply fails").
    pub(crate) fn run_stage(&mut self, plan: Plan) {
        let action = match plan {
            Plan::Nothing => return,
            Plan::Refuse(Refusal::ConflictMarkers(path)) => {
                self.report_error(AppError::ConflictMarkers(path));
                return;
            },
            Plan::Do(action) => action,
        };
        let Some(repo) = &self.repo else {
            return;
        };
        let result = staging::run(repo.as_ref(), &action);
        self.finish_apply(result);
        if let Some(left_out) = action.left_out() {
            self.report_error(AppError::PartlyStaged(left_out.to_vec()));
        }
    }

    /// `<space>` on a Files row (`Mode::Nav`): stage or unstage the whole
    /// file or directory, direction inferred from which side has a change
    /// (`docs/PLAN_6_STAGING.md` "Stage vs unstage is one key").
    pub(crate) fn stage_selected_file(&mut self) {
        if self.nav.focus != Pane::Files {
            return;
        }
        let rows = self.rows().files_tree_rows();
        let Some(repo) = &self.repo else {
            return;
        };
        let plan = match rows.get(self.selected(Pane::Files)) {
            Some(FileRow::Dir { path, .. }) => {
                staging::plan_directory(repo.as_ref(), &self.snapshot.files, path)
            },
            Some(FileRow::File { index, .. }) => match self.snapshot.files.get(*index) {
                Some(entry) => staging::plan_file(repo.as_ref(), entry),
                None => return,
            },
            None => return,
        };
        self.run_stage(plan);
    }

    /// `<space>` in `Mode::Diff`: stage/unstage the hunk under the cursor,
    /// or the V-selection when one is active.
    pub(crate) fn stage_diff_cursor(&mut self) {
        let Some(granule) = self.right.current_granule() else {
            return;
        };
        let action = staging::plan_granule(granule, self.right.cursor.side);
        self.right.cursor.anchor = None;
        self.run_stage(Plan::Do(action));
    }

    /// `a` (Nav, Files focused): stage every changed file if any is
    /// unstaged, else unstage everything: one `git` call either way
    /// (`docs/PLAN_6_STAGING.md` milestone S4).
    pub(crate) fn stage_all_files(&mut self) {
        if self.nav.focus != Pane::Files {
            return;
        }
        let Some(repo) = &self.repo else {
            return;
        };
        let plan = staging::plan_all(repo.as_ref(), &self.snapshot.files);
        self.run_stage(plan);
    }

    /// `d`: ask before discarding a worktree change, at the file granularity
    /// from `Mode::Nav` (Files focused) or at the hunk / line granularity
    /// under the cursor from `Mode::Diff`. Discard only ever touches the
    /// worktree (`docs/PLAN_6_STAGING.md`'s own scope), so it is a no-op on
    /// the Staged side and on a file with no worktree change of its own.
    pub(crate) fn discard_prompt(&mut self) {
        let prompt = match self.nav.mode {
            Mode::Nav if self.nav.focus == Pane::Files => {
                let Some(entry) = self.rows().selected_file() else {
                    return;
                };
                if entry.worktree == Change::None {
                    return;
                }
                ConfirmPrompt::discard_file(&entry.path)
            },
            Mode::Diff if self.right.cursor.side == DiffSide::Worktree => {
                let Some(granule) = self.right.current_granule() else {
                    return;
                };
                let Some(entry) = self.rows().selected_file() else {
                    return;
                };
                ConfirmPrompt::discard_granule(granule, &entry.path)
            },
            Mode::Nav | Mode::Diff => return,
        };
        self.modal.ask(prompt);
    }

    /// `y` while a confirm prompt is up: run its action. A branch delete
    /// refused for being unmerged (`"is not fully merged"`, the same
    /// stable-substring technique `commit.rs`'s `NothingStaged` already
    /// uses) re-opens the confirm one more time asking to force it,
    /// rather than reporting the refusal and stopping — `git branch -d`
    /// is offering a choice, not failing outright.
    pub(crate) fn run_confirm(&mut self) {
        let Some(prompt) = self.modal.take_confirm() else {
            return;
        };
        match prompt.action {
            ConfirmAction::DiscardFile(path) => {
                let action = staging::plan_discard_file(&self.snapshot.files, &path);
                self.right.cursor.anchor = None;
                self.run_stage(Plan::Do(action));
            },
            ConfirmAction::DiscardGranule(granule) => {
                self.right.cursor.anchor = None;
                self.run_stage(Plan::Do(staging::plan_discard_granule(granule)));
            },
            ConfirmAction::DeleteBranch { name, force } => {
                let Some(repo) = &self.repo else { return };
                match repo.delete_branch(&name, force) {
                    Ok(()) => self.request_refresh(),
                    Err(git::error::GitError::BranchNotMerged(_)) if !force => {
                        self.modal.ask(ConfirmPrompt {
                            message: format!(
                                "'{name}' is not fully merged. Force delete? This may lose \
                                 commits with no other reference to them."
                            ),
                            action: ConfirmAction::DeleteBranch { name, force: true },
                        });
                    },
                    Err(e) => self.report_error(e),
                }
            },
            ConfirmAction::DropStash { oid } => {
                let Some(repo) = &mut self.repo else { return };
                let events = stash::drop_entry(&oid, repo.as_mut());
                self.apply(events);
            },
            ConfirmAction::RestoreStash { oid, pop } => {
                let first_file = self.right.diff.first_stash_file(&oid);
                let Some(repo) = &mut self.repo else { return };
                let events = stash::restore(&oid, pop, repo.as_mut(), first_file);
                self.apply(events);
            },
            ConfirmAction::DropCommit { hash } => {
                let events = commits::rebase_edit(self.repo.as_deref(), &hash, &RebaseEdit::Drop);
                self.apply(events);
            },
            ConfirmAction::SquashCommit { hash } => {
                let events = commits::rebase_edit(self.repo.as_deref(), &hash, &RebaseEdit::Squash);
                self.apply(events);
            },
            ConfirmAction::AbortOperation => {
                self.apply_operation_step(git::operation::Step::Abort);
            },
            ConfirmAction::ConfigGlobal(resume) => self.resume_git_config_edit(resume),
            ConfirmAction::InitRepo(dir) => self.init_here(&dir),
            ConfirmAction::ConfigUnset(op) => self.confirm_git_config_unset(&op),
            ConfirmAction::ForcePush => {
                if let Some(sender) = self.workers.sender.clone() {
                    self.start_remote(RemoteRequest::force_push(), sender);
                }
            },
        }
    }

    /// Cursor state for the right-pane render: `(side, cursor line, V-select
    /// range)` while `Mode::Diff` is up, else `None`.
    pub fn diff_cursor(&self) -> Option<(DiffSide, usize, Option<Range<usize>>)> {
        (self.nav.mode == Mode::Diff).then(|| self.right.cursor_view())
    }

    /// Right-pane title suffix while `Mode::Diff` is up: `hunk 1/3` or
    /// `lines 41-42`/`line 41`, so it is obvious what `<space>` will hit.
    pub fn diff_granule_hint(&self) -> Option<String> {
        if self.nav.mode != Mode::Diff {
            return None;
        }
        self.right.granule_hint()
    }
}
