//! Commits-pane history rewrites: reword, drop, squash, fixup and edit the
//! selected commit, each as one `git rebase -i`. See
//! `docs/PLAN_11_REBASE.md` R4.

use crate::app::App;
use crate::git;
use crate::git::rebase::RebaseEdit;
use crate::interface::panes::pane::Pane;
use crate::interface::popups::confirm::ConfirmPrompt;

impl App {
    /// The selected commit, when a rewrite key may act on it: Commits focused
    /// in `Mode::Nav`, not drilled into a commit's files, no popup or confirm
    /// up, and no merge / rebase / cherry-pick / revert already stopped (that
    /// is what the `m` menu is for; say so instead of doing nothing).
    fn rewrite_target(&mut self) -> Option<git::model::CommitEntry> {
        if !self.nav.on_commit_list() || self.modal.is_some() || self.repo.is_none() {
            return None;
        }
        if self.snapshot.operation.is_some() {
            self.report_notice("finish or abort the operation in progress first (m)");
            return None;
        }
        self.rows().selected_commit().cloned()
    }

    /// `w` on Commits: reword the selected commit. `HEAD` keeps phase 7's
    /// `git commit --amend` path, no rebase needed.
    pub(crate) fn reword_selected_commit(&mut self) {
        if self.nav.focus == Pane::Commits && self.selected(Pane::Commits) == 0 {
            self.open_commit(git::commit::CommitKind::Reword);
            return;
        }
        let Some(entry) = self.rewrite_target() else {
            return;
        };
        let Some(repo) = &self.repo else { return };
        match repo.commit_message(&entry.full_hash) {
            Ok(message) => self.open_reword_editor(
                entry.full_hash.clone(),
                format!("Reword {}", entry.short_hash),
                &message,
            ),
            Err(e) => self.report_error(e),
        }
    }

    /// `d` on Commits: ask before dropping, the one irreversible-looking
    /// rewrite (the reflog still has it, but nothing in ferrit shows that).
    pub(crate) fn drop_commit_prompt(&mut self) {
        let Some(entry) = self.rewrite_target() else {
            return;
        };
        self.modal.ask(ConfirmPrompt::drop_commit(&entry));
    }

    /// `s` (squash, keeps both messages, asks first like drop) or `S` (fixup,
    /// drops this one, acts at once) on Commits: fold the selected commit
    /// into the one below it. The oldest commit has nothing below, so `s`
    /// there skips the question and reports why.
    pub(crate) fn fold_selected_commit(&mut self, fixup: bool) {
        let Some(entry) = self.rewrite_target() else {
            return;
        };
        let below = self.snapshot.commits.get(self.selected(Pane::Commits) + 1);
        if fixup {
            self.run_rebase_edit(&entry.full_hash, &RebaseEdit::Fixup);
        } else if let Some(below) = below {
            self.modal.ask(ConfirmPrompt::squash_commit(&entry, below));
        } else {
            self.run_rebase_edit(&entry.full_hash, &RebaseEdit::Squash);
        }
    }

    /// `e` on Commits: stop the rebase at the selected commit so it can be
    /// amended, leaving the rebase in progress for `m` to continue.
    pub(crate) fn edit_selected_commit(&mut self) {
        let Some(entry) = self.rewrite_target() else {
            return;
        };
        self.run_rebase_edit(&entry.full_hash, &RebaseEdit::Edit);
    }

    /// `F` on Commits: `git commit --fixup=<selected>` with what is staged,
    /// for a later autosquash. Nothing staged is an error, not an empty
    /// commit.
    pub(crate) fn create_fixup_commit(&mut self) {
        let Some(entry) = self.rewrite_target() else {
            return;
        };
        let Some(repo) = &self.repo else { return };
        match git::commit::fixup(
            repo.as_ref(),
            &entry.full_hash,
            self.authorship.author_arg(),
        ) {
            Ok(_) => self.request_refresh(),
            Err(git::error::GitError::NothingStaged) => {
                self.report_error(git::error::GitError::NothingStaged);
            },
            Err(error) => self.report_error(error),
        }
    }

    /// `a` on Commits: fold every `fixup!` / `squash!` commit from the
    /// selected commit up into its target. Skipped, with a note, when no such
    /// commit has its target in that range (git would rewrite nothing).
    pub(crate) fn autosquash_from_selected(&mut self) {
        let Some(entry) = self.rewrite_target() else {
            return;
        };
        let selected = self.selected(Pane::Commits);
        if !git::rebase::has_foldable_fixup(&self.snapshot.commits, selected) {
            self.report_notice("no fixup! or squash! commit above this one to fold");
            return;
        }
        let Some(repo) = &self.repo else { return };
        let result = repo.autosquash(&entry.full_hash);
        self.finish_operation(result);
    }

    /// Confirmed drop.
    pub(crate) fn drop_commit(&mut self, hash: &str) {
        self.run_rebase_edit(hash, &RebaseEdit::Drop);
    }

    pub(crate) fn run_rebase_edit(&mut self, hash: &str, edit: &RebaseEdit) {
        let Some(repo) = &self.repo else { return };
        let result = repo.rebase_edit(hash, edit);
        self.finish_operation(result);
    }
}
