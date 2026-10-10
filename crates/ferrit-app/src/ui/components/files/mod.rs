//! The Files pane: stage, unstage, discard.

pub(crate) mod keys;
pub(crate) mod rows;
pub(crate) mod tree;

use crate::ui::components::diff::right_pane::Mode;
use crate::ui::components::files::tree::FileRow;
use crate::ui::components::panes::nav::Pane;
use crate::ui::components::popups::ConfirmPrompt;
use crate::ui::error::AppError;
use crate::ui::event::{Env, Event};
use ferrit_domain::diff::DiffSide;
use ferrit_domain::model::Change;
use ferrit_domain::port::GitPort;
use ferrit_domain::staging::{self, Plan, Refusal};

/// `Enter` / `l` on a Files-pane file row (`Mode::Nav`): focus the diff for
/// staging within it. A no-op off the Files pane, on a directory row, already in
/// `Mode::Diff`, or when neither side has a selectable line to put the cursor on:
/// those stage whole-file only, from `Mode::Nav`.
pub(crate) fn enter_diff(env: &Env<'_>) -> Vec<Event> {
    if env.nav.focus != Pane::Files || env.nav.mode == Mode::Diff {
        return Vec::new();
    }
    env.rows()
        .selected_file()
        .map(|entry| Event::EnterDiff {
            has_worktree_change: entry.worktree != Change::None,
        })
        .into_iter()
        .collect()
}

/// Make a stage's call, then refresh and surface a failure in the Status pane.
/// `git apply` is atomic per invocation, so a failure leaves the repository
/// exactly as it was; the refresh still runs so a failed attempt (context drift
/// from an external edit) re-reads the current diff for the retry
/// (`docs/PLAN_6_STAGING.md` "apply fails").
pub(crate) fn run(plan: Plan, repo: Option<&dyn GitPort>) -> Vec<Event> {
    let action = match plan {
        Plan::Nothing => return Vec::new(),
        Plan::Refuse(Refusal::ConflictMarkers(path)) => {
            return vec![Event::Report(AppError::ConflictMarkers(path))];
        },
        Plan::Do(action) => action,
    };
    let Some(repo) = repo else {
        return Vec::new();
    };
    let mut events = vec![Event::FinishAction(staging::run(repo, &action))];
    if let Some(left_out) = action.left_out() {
        events.push(Event::Report(AppError::PartlyStaged(left_out.to_vec())));
    }
    events
}

/// `<space>` on a Files row (`Mode::Nav`): stage or unstage the whole file or
/// directory, direction inferred from which side has a change
/// (`docs/PLAN_6_STAGING.md` "Stage vs unstage is one key").
pub(crate) fn stage_selected(env: &Env<'_>) -> Vec<Event> {
    if env.nav.focus != Pane::Files {
        return Vec::new();
    }
    let Some(repo) = env.repo else {
        return Vec::new();
    };
    let rows = env.rows().files_tree_rows();
    let plan = match rows.get(env.nav.selection[Pane::Files]) {
        Some(FileRow::Dir { path, .. }) => staging::plan_directory(repo, &env.snapshot.files, path),
        Some(FileRow::File { index, .. }) => match env.snapshot.files.get(*index) {
            Some(entry) => staging::plan_file(repo, entry),
            None => return Vec::new(),
        },
        None => return Vec::new(),
    };
    run(plan, env.repo)
}

/// `<space>` in `Mode::Diff`: stage/unstage the hunk under the cursor, or the
/// V-selection when one is active.
pub(crate) fn stage_cursor(env: &Env<'_>) -> Vec<Event> {
    let Some(granule) = env.right.current_granule() else {
        return Vec::new();
    };
    let action = staging::plan_granule(granule, env.right.cursor.side);
    let mut events = vec![Event::ClearAnchor];
    events.extend(run(Plan::Do(action), env.repo));
    events
}

/// `a` (Nav, Files focused): stage every changed file if any is unstaged, else
/// unstage everything: one `git` call either way (`docs/PLAN_6_STAGING.md`
/// milestone S4).
pub(crate) fn stage_all(env: &Env<'_>) -> Vec<Event> {
    if env.nav.focus != Pane::Files {
        return Vec::new();
    }
    let Some(repo) = env.repo else {
        return Vec::new();
    };
    run(staging::plan_all(repo, &env.snapshot.files), env.repo)
}

/// `d`: ask before discarding a worktree change, at the file granularity from
/// `Mode::Nav` (Files focused) or at the hunk / line granularity under the cursor
/// from `Mode::Diff`. Discard only ever touches the worktree
/// (`docs/PLAN_6_STAGING.md`'s own scope), so it is a no-op on the Staged side and
/// on a file with no worktree change of its own.
pub(crate) fn discard_prompt(env: &Env<'_>) -> Vec<Event> {
    let prompt = match env.nav.mode {
        Mode::Nav if env.nav.focus == Pane::Files => {
            let Some(entry) = env.rows().selected_file() else {
                return Vec::new();
            };
            if entry.worktree == Change::None {
                return Vec::new();
            }
            ConfirmPrompt::discard_file(&entry.path)
        },
        Mode::Diff if env.right.cursor.side == DiffSide::Worktree => {
            let Some(granule) = env.right.current_granule() else {
                return Vec::new();
            };
            let Some(entry) = env.rows().selected_file() else {
                return Vec::new();
            };
            ConfirmPrompt::discard_granule(granule, &entry.path)
        },
        Mode::Nav | Mode::Diff => return Vec::new(),
    };
    vec![Event::Ask(prompt)]
}

/// The question about a whole file was answered yes.
pub(crate) fn discard_file(env: &Env<'_>, path: &std::path::Path) -> Vec<Event> {
    let action = staging::plan_discard_file(&env.snapshot.files, path);
    let mut events = vec![Event::ClearAnchor];
    events.extend(run(Plan::Do(action), env.repo));
    events
}

/// The question about a hunk or some lines was answered yes.
pub(crate) fn discard_granule(env: &Env<'_>, granule: ferrit_domain::apply::Granule) -> Vec<Event> {
    let mut events = vec![Event::ClearAnchor];
    events.extend(run(
        Plan::Do(staging::plan_discard_granule(granule)),
        env.repo,
    ));
    events
}
