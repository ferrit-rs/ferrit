//! Commits-pane history rewrites: reword, drop, squash, fixup and edit the
//! selected commit, each as one `git rebase -i`. See
//! `docs/PLAN_11_REBASE.md` R4.

use super::{App, AppError, ConfirmAction, ConfirmPrompt, Mode, Pane, git};
use crate::domain::git::rebase::RebaseEdit;

impl App {
    /// The selected commit, when a rewrite key may act on it: Commits focused
    /// in `Mode::Nav`, not drilled into a commit's files, no popup or confirm
    /// up, and no merge / rebase / cherry-pick / revert already stopped (that
    /// is what the `m` menu is for; say so instead of doing nothing).
    fn rewrite_target(&mut self) -> Option<git::model::CommitEntry> {
        if self.focus != Pane::Commits
            || self.mode != Mode::Nav
            || self.commit_drill.is_some()
            || self.popup.is_some()
            || self.pending_confirm.is_some()
            || self.repo.is_none()
        {
            return None;
        }
        if self.operation.is_some() {
            self.report_notice("finish or abort the operation in progress first (m)");
            return None;
        }
        self.commits.get(self.selected(Pane::Commits)).cloned()
    }

    /// `w` on Commits: reword the selected commit. `HEAD` keeps phase 7's
    /// `git commit --amend` path, no rebase needed.
    pub(super) fn reword_selected_commit(&mut self) {
        if self.focus == Pane::Commits && self.selected(Pane::Commits) == 0 {
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
    pub(super) fn drop_commit_prompt(&mut self) {
        let Some(entry) = self.rewrite_target() else {
            return;
        };
        self.pending_confirm = Some(ConfirmPrompt {
            message: format!("drop {} {}?", entry.short_hash, entry.summary),
            action: ConfirmAction::DropCommit {
                hash: entry.full_hash,
            },
        });
    }

    /// `s` (squash, keeps both messages) or `S` (fixup, drops this one) on
    /// Commits: fold the selected commit into the one below it.
    pub(super) fn fold_selected_commit(&mut self, fixup: bool) {
        let Some(entry) = self.rewrite_target() else {
            return;
        };
        let edit = if fixup {
            RebaseEdit::Fixup
        } else {
            RebaseEdit::Squash
        };
        self.run_rebase_edit(&entry.full_hash, &edit);
    }

    /// `e` on Commits: stop the rebase at the selected commit so it can be
    /// amended, leaving the rebase in progress for `m` to continue.
    pub(super) fn edit_selected_commit(&mut self) {
        let Some(entry) = self.rewrite_target() else {
            return;
        };
        self.run_rebase_edit(&entry.full_hash, &RebaseEdit::Edit);
    }

    /// `F` on Commits: `git commit --fixup=<selected>` with what is staged,
    /// for a later autosquash. Nothing staged is an error, not an empty
    /// commit.
    pub(super) fn create_fixup_commit(&mut self) {
        let Some(entry) = self.rewrite_target() else {
            return;
        };
        let opts = git::commit::CommitOpts {
            sign_off: false,
            no_verify: false,
            author: self.selected_author.as_ref().and_then(|identity| {
                identity
                    .email
                    .as_ref()
                    .map(|email| format!("{} <{email}>", identity.name))
            }),
        };
        let kind = git::commit::CommitKind::Fixup {
            target: entry.full_hash,
        };
        let Some(repo) = &self.repo else { return };
        match repo.commit(&kind, "", opts) {
            Ok(_) => self.request_refresh(),
            Err(git::error::GitError::NothingStaged) => {
                self.report_error(AppError::NothingStaged);
            },
            Err(error) => self.report_error(error),
        }
    }

    /// `a` on Commits: fold every `fixup!` / `squash!` commit from the
    /// selected commit up into its target. Skipped, with a note, when no such
    /// commit has its target in that range (git would rewrite nothing).
    pub(super) fn autosquash_from_selected(&mut self) {
        let Some(entry) = self.rewrite_target() else {
            return;
        };
        let selected = self.selected(Pane::Commits);
        if !has_foldable_fixup(&self.commits, selected) {
            self.report_notice("no fixup! or squash! commit above this one to fold");
            return;
        }
        let Some(repo) = &self.repo else { return };
        let result = repo.autosquash(&entry.full_hash);
        self.finish_operation(result);
    }

    /// Confirmed drop.
    pub(super) fn drop_commit(&mut self, hash: &str) {
        self.run_rebase_edit(hash, &RebaseEdit::Drop);
    }

    pub(super) fn run_rebase_edit(&mut self, hash: &str, edit: &RebaseEdit) {
        let Some(repo) = &self.repo else { return };
        let result = repo.rebase_edit(hash, edit);
        self.finish_operation(result);
    }
}

/// Does any `fixup! <subject>` / `squash! <subject>` among `commits[..=selected]`
/// (newest first) have a commit with that subject further down, still inside
/// the range? Only then does an autosquash from `selected` change anything.
fn has_foldable_fixup(commits: &[git::model::CommitEntry], selected: usize) -> bool {
    let Some(range) = commits.get(..=selected) else {
        return false;
    };
    range.iter().enumerate().any(|(index, commit)| {
        let Some(target) = commit
            .summary
            .strip_prefix("fixup! ")
            .or_else(|| commit.summary.strip_prefix("squash! "))
        else {
            return false;
        };
        range
            .iter()
            .skip(index + 1)
            .any(|candidate| candidate.summary == target)
    })
}
