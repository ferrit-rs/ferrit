//! The Stash pane: stash, apply, pop, drop.

use std::path::PathBuf;

use crate::git::error::GitError;
use crate::git::model::StashEntry;
use crate::git::port::GitPort;
use crate::git::refs::{self, StashOutcome};
use crate::tui::components::panes::nav::Pane;
use crate::tui::components::panes::selection::SelectionKey;
use crate::tui::components::popups::{ConfirmPrompt, Popup};
use crate::tui::event::{Env, Event};
use crate::tui::widgets::text_input::TextInput;

/// The selected stash entry, only while Stash is focused in `Mode::Nav` and no
/// popup is up.
fn selected<'a>(env: &Env<'a>) -> Option<&'a StashEntry> {
    if !env.nav.on_stash() || env.popup_up {
        return None;
    }
    env.rows().selected_stash()
}

/// `s` (Nav, Files focused): open the stash message popup. A clean tree opens
/// nothing and says so.
pub(crate) fn open_popup(env: &Env<'_>) -> Vec<Event> {
    if !env.nav.on_files() || env.popup_up {
        return Vec::new();
    }
    if env.snapshot.files.is_empty() {
        return vec![Event::Report(GitError::NothingToStash.into())];
    }
    vec![Event::OpenPopup(Popup::Stash(TextInput::default()))]
}

/// `Enter` in the stash popup: `git stash push --include-untracked`. Success
/// and "nothing to stash" close it; any other failure keeps the popup and the
/// typed message so the user can retry (the new-branch rule).
pub(crate) fn push(message: &str, repo: Option<&dyn GitPort>) -> Vec<Event> {
    let Some(repo) = repo else {
        return Vec::new();
    };
    match repo.stash_push(message.trim()) {
        Ok(()) => vec![Event::ClosePopup, Event::Refresh],
        Err(e @ GitError::NothingToStash) => {
            vec![Event::ClosePopup, Event::Report(e.into())]
        },
        Err(e) => vec![Event::Report(e.into())],
    }
}

/// `<space>` (apply) or `g` (pop) on the Stash pane: ask before doing either.
pub(crate) fn restore_prompt(env: &Env<'_>, pop: bool) -> Vec<Event> {
    selected(env)
        .map(|entry| Event::Ask(ConfirmPrompt::restore_stash(entry, pop)))
        .into_iter()
        .collect()
}

/// Confirmed apply or pop. A clean restore moves the focus to Files with the
/// first restored file selected, like lazygit; a conflict or an error leaves the
/// focus on Stash.
pub(crate) fn restore(
    oid: &str,
    pop: bool,
    repo: &mut dyn GitPort,
    first_file: Option<PathBuf>,
) -> Vec<Event> {
    let result = refs::restore(repo, oid, pop);
    let mut events = Vec::new();
    if matches!(result, Ok(StashOutcome::Done)) {
        events.push(Event::Focus(Pane::Files));
        if let Some(path) = first_file {
            events.push(Event::SelectWhenListed(
                Pane::Files,
                SelectionKey::File(path),
            ));
        }
    }
    events.push(Event::Refresh);
    match result {
        Ok(StashOutcome::Done) => {},
        Ok(StashOutcome::Conflicted) => events.push(Event::OpenPopup(Popup::Note(
            "stash applied with conflicts. The stash was kept. Resolve the conflicts in Files."
                .to_owned(),
        ))),
        Err(e) => events.push(Event::Report(e.into())),
    }
    events
}

/// `d` on the Stash pane: ask before dropping.
pub(crate) fn drop_prompt(env: &Env<'_>) -> Vec<Event> {
    selected(env)
        .map(|entry| Event::Ask(ConfirmPrompt::drop_stash(entry)))
        .into_iter()
        .collect()
}

/// Confirmed `d`: `git stash drop`, refreshing either way.
pub(crate) fn drop_entry(oid: &str, repo: &mut dyn GitPort) -> Vec<Event> {
    vec![Event::FinishAction(repo.stash_drop(oid))]
}
