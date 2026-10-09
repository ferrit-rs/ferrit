//! Fetch, pull, push: what the keys ask for. Running them in the background and
//! taking their answer are `workers`.

use crate::git::remote::{self, PushPlan, RemoteOp, RemoteRequest};
use crate::tui::components::popups::{ConfirmAction, ConfirmPrompt, Popup};
use crate::tui::error::AppError;
use crate::tui::event::{Env, Event};
use crate::tui::widgets::text_input::TextInput;

/// `f` / `p`: fetch / pull. Global, not Branches-only: these act on the repo and
/// its current branch, not a selected row. A no-op with no event sender set
/// (`App::mock()`, or a test driving `on_key` without `run()`).
pub(crate) fn trigger(op: RemoteOp) -> Vec<Event> {
    vec![Event::StartRemote(RemoteRequest::new(op))]
}

/// `P`: push. Unlike `f`/`p`, push needs to know *before* running whether the
/// current branch has an upstream at all (the Status header already has it, read
/// for the ahead/behind count): `git::remote::plan_push` decides between a plain
/// push, a lease-guarded one that asks first, one that sets the upstream, and a
/// LazyGit-style editable `<remote> <branch>` prompt.
pub(crate) fn push(env: &Env<'_>) -> Vec<Event> {
    if env.popup_up {
        return Vec::new();
    }
    let Some(repo) = env.repo else {
        return Vec::new();
    };
    let remotes = if env.snapshot.header.upstream.is_some() {
        Vec::new()
    } else {
        match repo.remotes() {
            Ok(remotes) => remotes,
            Err(error) => return vec![Event::Report(error.into())],
        }
    };
    match remote::plan_push(&env.snapshot.header, &remotes, repo.push_default_current()) {
        PushPlan::Plain => trigger(RemoteOp::Push),
        PushPlan::ConfirmForce(message) => vec![Event::Ask(ConfirmPrompt {
            message,
            action: ConfirmAction::ForcePush,
        })],
        PushPlan::SetCurrent => vec![Event::StartRemote(RemoteRequest::push_current())],
        PushPlan::AskUpstream(text) => vec![Event::OpenPopup(Popup::Upstream(
            TextInput::from_text(&text),
        ))],
    }
}

/// The upstream prompt was answered: `git push -u <remote> <local>:<remote
/// branch>`.
pub(crate) fn submit_upstream(value: &str) -> Vec<Event> {
    let Some((remote, branch)) = remote::parse_upstream(value) else {
        return vec![Event::Report(AppError::BadUpstream)];
    };
    vec![
        Event::ClosePopup,
        Event::StartRemote(RemoteRequest::push_to(remote.to_owned(), branch.to_owned())),
    ]
}
