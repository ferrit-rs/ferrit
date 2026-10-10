//! Default action bindings and override parsing support.

use ratatui::crossterm::event::KeyCode;

use crate::ui::keymap::Keymap;
use crate::ui::keymap::action::Action;
use crate::ui::keymap::binding::KeyBinding;
use crate::ui::keymap::context::Context;

/// `(context, key, action)` for every default binding.
pub(crate) const ENTRIES: &[(Context, &str, Action)] = &[
    (Context::Global, "q", Action::Quit),
    (Context::Global, "?", Action::Help),
    (Context::Global, "@", Action::CommandLog),
    (Context::Global, "D", Action::Dashboard),
    (Context::Global, "C", Action::GitConfig),
    (Context::Global, "G", Action::CreateRemote),
    (Context::Global, "m", Action::OperationMenu),
    (Context::Global, "x", Action::ContextMenu),
    (Context::Global, "esc", Action::Back),
    (Context::Global, "enter", Action::Enter),
    (Context::Global, "l", Action::EnterDiff),
    (Context::Global, "r", Action::Refresh),
    (Context::Global, "f", Action::Fetch),
    (Context::Global, "p", Action::Pull),
    (Context::Global, "P", Action::Push),
    (Context::Global, "c", Action::Commit),
    (Context::Global, "A", Action::Amend),
    (Context::Global, "w", Action::RewordHead),
    (
        Context::Global,
        "1",
        Action::Focus(crate::ui::components::panes::nav::Pane::Status),
    ),
    (
        Context::Global,
        "2",
        Action::Focus(crate::ui::components::panes::nav::Pane::Files),
    ),
    (
        Context::Global,
        "3",
        Action::Focus(crate::ui::components::panes::nav::Pane::Branches),
    ),
    (
        Context::Global,
        "4",
        Action::Focus(crate::ui::components::panes::nav::Pane::Commits),
    ),
    (
        Context::Global,
        "5",
        Action::Focus(crate::ui::components::panes::nav::Pane::Stash),
    ),
    (Context::Global, "tab", Action::NextPane),
    (Context::Global, "right", Action::NextPane),
    (Context::Global, "backtab", Action::PrevPane),
    (Context::Global, "left", Action::PrevPane),
    (Context::Global, "ctrl-right", Action::ToggleBranchesTab),
    (Context::Global, "ctrl-left", Action::ToggleBranchesTab),
    (Context::Global, "j", Action::SelectDown),
    (Context::Global, "down", Action::SelectDown),
    (Context::Global, "k", Action::SelectUp),
    (Context::Global, "up", Action::SelectUp),
    (Context::Global, "J", Action::ScrollLineDown),
    (Context::Global, "K", Action::ScrollLineUp),
    (Context::Global, "pgdn", Action::ScrollPageDown),
    (Context::Global, "pgup", Action::ScrollPageUp),
    (Context::Global, "ctrl-d", Action::ScrollHalfDown),
    (Context::Global, "ctrl-u", Action::ScrollHalfUp),
    (Context::Global, "<", Action::ScrollTop),
    (Context::Global, ">", Action::ScrollBottom),
    (Context::Global, "]", Action::NextHunk),
    (Context::Global, "[", Action::PrevHunk),
    (Context::Files, "space", Action::StageFile),
    (Context::Files, "a", Action::StageAll),
    (Context::Files, "d", Action::Discard),
    (Context::Files, "s", Action::StashPush),
    (Context::Diff, "esc", Action::LeaveDiff),
    (Context::Diff, "h", Action::LeaveDiff),
    (Context::Diff, "j", Action::CursorDown),
    (Context::Diff, "down", Action::CursorDown),
    (Context::Diff, "k", Action::CursorUp),
    (Context::Diff, "up", Action::CursorUp),
    (Context::Diff, "]", Action::CursorNextHunk),
    (Context::Diff, "[", Action::CursorPrevHunk),
    (Context::Diff, "V", Action::ToggleSelection),
    (Context::Diff, "space", Action::StageCursor),
    (Context::Diff, "d", Action::Discard),
    (Context::Branches, "space", Action::Checkout),
    (Context::Branches, "n", Action::NewBranch),
    (Context::Branches, "u", Action::FastForward),
    (Context::Branches, "M", Action::Merge),
    (Context::Branches, "d", Action::DeleteBranch),
    (Context::Commits, "r", Action::RewordCommit),
    (Context::Commits, "w", Action::RewordCommit),
    (Context::Commits, "d", Action::DropCommit),
    (Context::Commits, "s", Action::Squash),
    (Context::Commits, "S", Action::Fixup),
    (Context::Commits, "e", Action::EditCommit),
    (Context::Commits, "F", Action::NewFixup),
    (Context::Commits, "a", Action::Autosquash),
    (Context::Stash, "space", Action::ApplyStash),
    (Context::Stash, "g", Action::PopStash),
    (Context::Stash, "d", Action::DropStash),
];

pub(crate) fn entries() -> &'static [(Context, &'static str, Action)] {
    ENTRIES
}

/// One `[keys]` entry that passed first checks.
pub(crate) struct Override {
    pub(crate) context: Context,
    pub(crate) action: Action,
    pub(crate) keys: Vec<KeyBinding>,
    pub(crate) label: String,
}

/// `Ctrl-c` always quits and cannot be rebound.
pub(crate) const RESERVED_QUIT: KeyBinding = KeyBinding {
    code: KeyCode::Char('c'),
    ctrl: true,
    alt: false,
};

pub(crate) fn default_bindings() -> Keymap {
    let bindings = ENTRIES
        .iter()
        .filter_map(|&(context, key, action)| {
            KeyBinding::parse(key).map(|binding| ((context, binding), action))
        })
        .collect();
    Keymap { bindings }
}
