//! The Commits pane: reword, drop, squash, fixup, edit, autosquash.

use crate::git;
use crate::git::commit::CommitKind;
use crate::git::error::GitError;
use crate::git::model::CommitEntry;
use crate::git::port::GitPort;
use crate::git::rebase::RebaseEdit;
use crate::tui::components::panes::Pane;
use crate::tui::components::popups::ConfirmPrompt;
use crate::tui::event::{Env, Event};

/// The selected commit, when a rewrite key may act on it: Commits focused in
/// `Mode::Nav`, not drilled into a commit's files, no popup or question up, and
/// no merge / rebase / cherry-pick / revert already stopped (that is what the
/// `m` menu is for; say so instead of doing nothing).
fn target<'a>(env: &Env<'a>) -> Result<&'a CommitEntry, Vec<Event>> {
    if !env.nav.on_commit_list() || env.modal_up || env.repo.is_none() {
        return Err(Vec::new());
    }
    if env.snapshot.operation.is_some() {
        return Err(vec![Event::Notice(
            "finish or abort the operation in progress first (m)".to_owned(),
        )]);
    }
    env.rows().selected_commit().ok_or_else(Vec::new)
}

/// `w` on Commits: reword the selected commit. `HEAD` keeps the
/// `git commit --amend` path, no rebase needed.
pub(crate) fn reword(env: &Env<'_>) -> Vec<Event> {
    if env.nav.focus == Pane::Commits && env.nav.selection[Pane::Commits] == 0 {
        return vec![Event::OpenCommit(CommitKind::Reword)];
    }
    let entry = match target(env) {
        Ok(entry) => entry,
        Err(events) => return events,
    };
    let Some(repo) = env.repo else {
        return Vec::new();
    };
    match repo.commit_message(&entry.full_hash) {
        Ok(message) => vec![Event::OpenReword {
            hash: entry.full_hash.clone(),
            title: format!("Reword {}", entry.short_hash),
            message,
        }],
        Err(e) => vec![Event::Report(e.into())],
    }
}

/// `d` on Commits: ask before dropping, the one irreversible-looking rewrite
/// (the reflog still has it, but nothing in ferrit shows that).
pub(crate) fn drop_prompt(env: &Env<'_>) -> Vec<Event> {
    match target(env) {
        Ok(entry) => vec![Event::Ask(ConfirmPrompt::drop_commit(entry))],
        Err(events) => events,
    }
}

/// `s` (squash, keeps both messages, asks first like drop) or `S` (fixup, drops
/// this one, acts at once) on Commits: fold the selected commit into the one
/// below it. The oldest commit has nothing below, so `s` there skips the question.
pub(crate) fn fold(env: &Env<'_>, fixup: bool) -> Vec<Event> {
    let entry = match target(env) {
        Ok(entry) => entry,
        Err(events) => return events,
    };
    let below = env
        .snapshot
        .commits
        .get(env.nav.selection[Pane::Commits] + 1);
    if fixup {
        rebase_edit(env.repo, &entry.full_hash, &RebaseEdit::Fixup)
    } else if let Some(below) = below {
        vec![Event::Ask(ConfirmPrompt::squash_commit(entry, below))]
    } else {
        rebase_edit(env.repo, &entry.full_hash, &RebaseEdit::Squash)
    }
}

/// `e` on Commits: stop the rebase at the selected commit so it can be amended,
/// leaving the rebase in progress for `m` to continue.
pub(crate) fn edit(env: &Env<'_>) -> Vec<Event> {
    match target(env) {
        Ok(entry) => rebase_edit(env.repo, &entry.full_hash, &RebaseEdit::Edit),
        Err(events) => events,
    }
}

/// `F` on Commits: `git commit --fixup=<selected>` with what is staged, for a
/// later autosquash. Nothing staged is an error, not an empty commit.
pub(crate) fn new_fixup(env: &Env<'_>) -> Vec<Event> {
    let entry = match target(env) {
        Ok(entry) => entry,
        Err(events) => return events,
    };
    let Some(repo) = env.repo else {
        return Vec::new();
    };
    match git::commit::fixup(repo, &entry.full_hash, env.author.clone()) {
        Ok(_) => vec![Event::Refresh],
        Err(GitError::NothingStaged) => vec![Event::Report(GitError::NothingStaged.into())],
        Err(error) => vec![Event::Report(error.into())],
    }
}

/// `a` on Commits: fold every `fixup!` / `squash!` commit from the selected
/// commit up into its target. Skipped, with a note, when no such commit has its
/// target in that range (git would rewrite nothing).
pub(crate) fn autosquash(env: &Env<'_>) -> Vec<Event> {
    let entry = match target(env) {
        Ok(entry) => entry,
        Err(events) => return events,
    };
    let selected = env.nav.selection[Pane::Commits];
    if !git::rebase::has_foldable_fixup(&env.snapshot.commits, selected) {
        return vec![Event::Notice(
            "no fixup! or squash! commit above this one to fold".to_owned(),
        )];
    }
    let Some(repo) = env.repo else {
        return Vec::new();
    };
    vec![Event::FinishOperation(repo.autosquash(&entry.full_hash))]
}

/// Run one `git rebase -i` edit on `hash` and say where git stopped.
pub(crate) fn rebase_edit(repo: Option<&dyn GitPort>, hash: &str, edit: &RebaseEdit) -> Vec<Event> {
    repo.map(|repo| Event::FinishOperation(repo.rebase_edit(hash, edit)))
        .into_iter()
        .collect()
}
