//! Staging, unstaging and discarding, from the keys that ask for them: pick
//! what the key acts on (`PaneRows`, `RightPane`, `git::staging`), call the
//! port, then refresh and tell the user how it went.

use std::ops::Range;
use std::path::Path;

use super::{App, events, git};
use crate::app::error::AppError;
use crate::git::apply::{ApplyDir, ApplyTarget, Granule};
use crate::git::diff::DiffSide;
use crate::git::error::GitResult;
use crate::git::model::Change;
use crate::git::staging;
use crate::interface::panes::diff_cursor::Mode;
use crate::interface::panes::pane::Pane;
use crate::interface::panes::tree::FileRow;
use crate::interface::popups::confirm::{ConfirmAction, ConfirmPrompt};

impl App {
    /// `Enter` / `l` on a Files-pane file row (`Mode::Nav`): focus the diff
    /// for staging within it. A no-op off the Files pane, on a directory
    /// row, already in `Mode::Diff`, or when neither side has a selectable
    /// line to put the cursor on: those stage whole-file only, from `Mode::Nav`.
    pub(super) fn enter_diff_mode(&mut self) {
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
    pub(super) fn leave_diff_mode(&mut self) {
        self.nav.mode = Mode::Nav;
    }

    /// Run a `Granule` through the repository, if there is one.
    fn apply_granule(
        &self,
        granule: &Granule,
        dir: ApplyDir,
        target: ApplyTarget,
    ) -> GitResult<()> {
        match &self.repo {
            Some(repo) => staging::apply_granule(repo.as_ref(), granule, dir, target),
            None => Ok(()),
        }
    }

    /// Refresh after a stage / unstage / discard, then surface a failure in
    /// the Status pane. `git apply` is atomic per invocation, so a failure
    /// leaves the repository exactly as it was; the refresh still runs so a
    /// failed attempt (context drift from an external edit) re-reads the
    /// current diff for the retry (`docs/PLAN_6_STAGING.md` "apply fails").
    pub(super) fn finish_apply(&mut self, result: GitResult<()>) {
        self.request_refresh();
        if let Err(e) = result {
            self.report_error(e);
        }
    }

    /// `<space>` on a Files row (`Mode::Nav`): stage or unstage the whole
    /// file, direction inferred from which side has a change
    /// (`docs/PLAN_6_STAGING.md` "Stage vs unstage is one key").
    pub(super) fn stage_selected_file(&mut self) {
        if self.nav.focus != Pane::Files {
            return;
        }
        let directory = match self
            .rows()
            .files_tree_rows()
            .get(self.selected(Pane::Files))
        {
            Some(FileRow::Dir { path, .. }) => Some(path.clone()),
            _ => None,
        };
        if let Some(path) = directory {
            self.stage_directory(&path);
            return;
        }
        let Some(entry) = self.rows().selected_file() else {
            return;
        };
        let Some(dir) = staging::direction([entry]) else {
            return;
        };
        let path = entry.path.clone();
        // `git add` on an unmerged path marks it resolved whatever the file
        // holds; refuse while conflict markers remain.
        if dir == ApplyDir::Forward && entry.is_conflicted() && self.has_markers(&path) {
            self.report_error(AppError::ConflictMarkers(path));
            return;
        }
        let Some(repo) = &self.repo else {
            return;
        };
        let result = repo.stage_file(&path, dir);
        self.finish_apply(result);
    }

    /// `<space>` on a directory row: stage every change under it, or unstage them
    /// all when none is left to stage, as lazygit does. The root row (an empty
    /// path) is every file. Conflicted files that still hold markers block it.
    fn stage_directory(&mut self, directory: &Path) {
        let under = self
            .snapshot
            .files
            .iter()
            .filter(|f| directory.as_os_str().is_empty() || f.path.starts_with(directory));
        let Some(dir) = staging::direction(under.clone()) else {
            return;
        };
        let Some(repo) = &self.repo else {
            return;
        };
        let blocked = staging::unresolved_conflicts(repo.as_ref(), under);
        if dir == ApplyDir::Forward && !blocked.is_empty() {
            if let Some(first) = blocked.first() {
                self.report_error(AppError::ConflictMarkers(first.clone()));
            }
            return;
        }
        let result = if directory.as_os_str().is_empty() {
            repo.stage_all(dir)
        } else {
            repo.stage_file(directory, dir)
        };
        self.finish_apply(result);
    }

    /// Conflict markers still in `path`.
    fn has_markers(&self, path: &Path) -> bool {
        self.repo
            .as_ref()
            .is_some_and(|repo| staging::has_markers(repo.as_ref(), path))
    }

    /// `<space>` in `Mode::Diff`: stage/unstage the hunk under the cursor,
    /// or the V-selection when one is active.
    pub(super) fn stage_diff_cursor(&mut self) {
        let Some(granule) = self.right.current_granule() else {
            return;
        };
        let dir = match self.right.cursor.side {
            DiffSide::Worktree => ApplyDir::Forward,
            DiffSide::Staged => ApplyDir::Reverse,
        };
        let result = self.apply_granule(&granule, dir, ApplyTarget::Index);
        self.right.cursor.anchor = None;
        self.finish_apply(result);
    }

    /// `a` (Nav, Files focused): stage every changed file if any is
    /// unstaged, else unstage everything: one `git` call either way
    /// (`docs/PLAN_6_STAGING.md` milestone S4).
    pub(super) fn stage_all_files(&mut self) {
        if self.nav.focus != Pane::Files {
            return;
        }
        let Some(dir) = staging::direction(&self.snapshot.files) else {
            return;
        };
        let Some(repo) = &self.repo else {
            return;
        };
        let blocked = if dir == ApplyDir::Forward {
            staging::unresolved_conflicts(repo.as_ref(), &self.snapshot.files)
        } else {
            Vec::new()
        };
        let result = if blocked.is_empty() {
            repo.stage_all(dir)
        } else {
            repo.stage_all_except(&blocked)
        };
        self.finish_apply(result);
        if !blocked.is_empty() {
            self.report_error(AppError::PartlyStaged(blocked));
        }
    }

    /// `d`: ask before discarding a worktree change, at the file granularity
    /// from `Mode::Nav` (Files focused) or at the hunk / line granularity
    /// under the cursor from `Mode::Diff`. Discard only ever touches the
    /// worktree (`docs/PLAN_6_STAGING.md`'s own scope), so it is a no-op on
    /// the Staged side and on a file with no worktree change of its own.
    pub(super) fn discard_prompt(&mut self) {
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
    pub(super) fn run_confirm(&mut self) {
        let Some(prompt) = self.modal.take_confirm() else {
            return;
        };
        match prompt.action {
            ConfirmAction::DiscardFile(path) => {
                let untracked = self
                    .snapshot
                    .files
                    .iter()
                    .find(|f| f.path == path)
                    .is_some_and(|f| f.worktree == Change::Untracked);
                let result = match &self.repo {
                    Some(repo) => repo.discard_file(&path, untracked),
                    None => return,
                };
                self.right.cursor.anchor = None;
                self.finish_apply(result);
            },
            ConfirmAction::DiscardGranule(granule) => {
                let result = self.apply_granule(&granule, ApplyDir::Reverse, ApplyTarget::Worktree);
                self.right.cursor.anchor = None;
                self.finish_apply(result);
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
            ConfirmAction::DropStash { oid } => self.drop_stash(&oid),
            ConfirmAction::RestoreStash { oid, pop } => self.restore_stash(&oid, pop),
            ConfirmAction::DropCommit { hash } => self.drop_commit(&hash),
            ConfirmAction::SquashCommit { hash } => {
                self.run_rebase_edit(&hash, &git::rebase::RebaseEdit::Squash);
            },
            ConfirmAction::AbortOperation => {
                self.apply_operation_step(git::operation::Step::Abort);
            },
            ConfirmAction::ConfigGlobal(resume) => self.resume_git_config_edit(resume),
            ConfirmAction::InitRepo(dir) => self.init_here(&dir),
            ConfirmAction::ConfigUnset(op) => self.confirm_git_config_unset(&op),
            ConfirmAction::ForcePush => {
                if let Some(sender) = self.workers.sender.clone() {
                    self.start_remote_op_with_force(
                        events::RemoteOp::Push,
                        None,
                        None,
                        true,
                        sender,
                    );
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
