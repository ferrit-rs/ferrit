//! The Branches pane: checkout, create, delete, fast-forward, merge.

use crate::git::branch::{self, MergeKind, MergeOutcome};
use crate::tui::components::menu::MenuState;
use crate::tui::components::panes::{BranchesTab, Pane, SelectionKey};
use crate::tui::components::popups::{ConfirmPrompt, Popup};
use crate::tui::event::{Env, Event};
use crate::tui::widgets::text_input::TextInput;

/// Enter on the Branches pane: lazygit's branch -> log drill-down. Read only, no
/// checkout. `Esc` backs out.
pub(crate) fn enter_log(env: &Env<'_>) -> Vec<Event> {
    if !env.nav.on_local_branches() {
        return Vec::new();
    }
    let Some(repo) = env.repo else {
        return Vec::new();
    };
    let Some(name) = env.rows().selected_branch().map(|b| b.name.clone()) else {
        return Vec::new();
    };
    match repo.branch_log(&name) {
        Ok(commits) => vec![Event::DrillIntoBranch {
            branch: name,
            commits,
        }],
        Err(e) => vec![Event::Report(e.into())],
    }
}

/// `<space>` on the Branches pane (`Mode::Nav`): checkout the selected branch.
/// A refresh picks up the new `HEAD`, branches, and files (a checkout changes
/// the working tree too).
pub(crate) fn checkout(env: &Env<'_>) -> Vec<Event> {
    if !env.nav.on_local_branches() {
        return Vec::new();
    }
    let (Some(name), Some(repo)) = (
        env.rows().selected_branch().map(|b| b.name.clone()),
        env.repo,
    ) else {
        return Vec::new();
    };
    vec![Event::FinishAction(repo.checkout(&name))]
}

/// `n` (Nav, Branches focused): open the new-branch popup, named from the
/// selected branch once submitted.
pub(crate) fn open_new_popup(env: &Env<'_>) -> Vec<Event> {
    if !env.nav.on_local_branches() || env.popup_up {
        return Vec::new();
    }
    let base = env
        .rows()
        .selected_branch()
        .map_or(env.snapshot.header.branch.as_str(), |b| b.name.as_str())
        .to_owned();
    vec![
        Event::NewBranchTitle(format!("New branch name (branch is off of '{base}')")),
        Event::OpenPopup(Popup::NewBranch(TextInput::default())),
    ]
}

/// `Enter` in the new-branch popup: `git checkout -b <name>` from the selected
/// branch, without tracking it. Success closes the popup and refreshes; failure
/// (a bad name, or one already taken) keeps the popup open with the typed text so
/// the user can fix it and retry: the message surfaces in the Status pane rather
/// than a second popup layered on this one.
pub(crate) fn create(name: &str, env: &Env<'_>) -> Vec<Event> {
    let Some(repo) = env.repo else {
        return Vec::new();
    };
    let result = match env.rows().selected_branch() {
        Some(base) => repo.create_branch_at(name, &format!("refs/heads/{}", base.name)),
        None => repo.create_branch(name),
    };
    match result {
        Ok(()) => {
            let mut events = vec![Event::ClosePopup];
            // The new branch is the checked-out one: select it, not the row the
            // cursor was on (lazygit).
            if env.nav.branch_drill.is_none() && env.nav.branches_tab == BranchesTab::Local {
                events.push(Event::SelectWhenListed(
                    Pane::Branches,
                    SelectionKey::Branch(name.to_owned()),
                ));
            }
            events.push(Event::Refresh);
            events
        },
        Err(e) => vec![Event::Report(e.into())],
    }
}

/// `d` (Nav, Branches focused): ask before deleting the selected branch. The
/// currently checked-out branch skips the question: `git` refuses to delete it
/// either way, so its own message goes straight to the Status line rather than
/// opening a question for an outcome that is already certain.
pub(crate) fn delete_prompt(env: &Env<'_>) -> Vec<Event> {
    if !env.nav.on_local_branches() {
        return Vec::new();
    }
    let Some(entry) = env.rows().selected_branch() else {
        return Vec::new();
    };
    if entry.is_head {
        return env
            .repo
            .map(|repo| Event::FinishAction(repo.delete_branch(&entry.name, false)))
            .into_iter()
            .collect();
    }
    vec![Event::Ask(ConfirmPrompt::delete_branch(entry.name.clone()))]
}

/// `u` (Nav, Branches focused): fast-forward the selected branch to its
/// upstream, checked out or not. No question: exactly as reversible as any other
/// git command, the reflog has your back.
pub(crate) fn fast_forward(env: &Env<'_>) -> Vec<Event> {
    if !env.nav.on_local_branches() {
        return Vec::new();
    }
    let (Some(name), Some(repo)) = (
        env.rows().selected_branch().map(|b| b.name.clone()),
        env.repo,
    ) else {
        return Vec::new();
    };
    vec![Event::FinishAction(repo.fast_forward(&name))]
}

/// `M` (Nav, Branches focused): open the Merge menu for the selected branch, and
/// merge nothing until a row is chosen. On the current branch there is nothing to
/// choose between, so it merges straight away (git answers "Already up to date").
pub(crate) fn merge(env: &Env<'_>) -> Vec<Event> {
    if !env.nav.on_local_branches() || env.modal_up {
        return Vec::new();
    }
    let Some(entry) = env.rows().selected_branch() else {
        return Vec::new();
    };
    if entry.is_head {
        return merge_with(env, MergeKind::Regular);
    }
    vec![Event::OpenPopup(Popup::Menu(MenuState::merge()))]
}

/// Merge the selected branch into the current one the way `kind` says. A refresh
/// always follows, even on a conflict: the Files pane already renders
/// `Change::Conflicted`, so the conflicted paths are visible without a dedicated
/// flow.
pub(crate) fn merge_with(env: &Env<'_>, kind: MergeKind) -> Vec<Event> {
    if !env.nav.on_local_branches() {
        return Vec::new();
    }
    let (Some(name), Some(repo)) = (
        env.rows().selected_branch().map(|b| b.name.clone()),
        env.repo,
    ) else {
        return Vec::new();
    };
    let mut events = vec![Event::Refresh];
    match branch::merge(repo, &name, kind) {
        Ok(MergeOutcome::Merged) => {},
        Ok(MergeOutcome::Conflicted) => events.push(Event::MergeConflicted),
        Err(e) => events.push(Event::Report(e.into())),
    }
    events
}
