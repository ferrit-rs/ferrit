//! What a popup can be.

use crate::git::keys::askpass;
use crate::interface::components::ui::text_input::TextInput;
use crate::interface::state::create_remote_form;
use crate::interface::state::{commit_draft, context_menu, menu};

/// Modal state that owns all input while it is up, the same idea as
/// `show_help` today but richer (`docs/PLAN_7_COMMIT.md`).
pub(crate) enum Popup {
    Commit(commit_draft::CommitDraft),
    CommitAllConfirm,
    /// New-branch name input (`docs/PLAN_8_BRANCHES.md`). `Enter` *submits*
    /// here, unlike the commit popup, where `Enter` inserts a newline —
    /// the only behavioural difference from reusing `TextInput` outright.
    NewBranch(TextInput),
    /// A one-line name or message with a purpose: rename a branch or a stash,
    /// a branch at a commit, a stash keeping the index (`app::context_menu`).
    Name(context_menu::NameTarget, TextInput),
    /// Stash message input, `s` on Files (`docs/PLAN_10_STASH.md`). `Enter`
    /// submits; an empty message lets git write its own.
    Stash(TextInput),
    /// `P` with no upstream: edit `<remote> <branch>` before first push.
    Upstream(TextInput),
    /// A passphrase, password or host-key question from ssh/git during a
    /// remote op (`app::askpass`).
    Askpass(askpass::AskpassPrompt),
    /// A list of actions to pick from (`app::menu`): the `m` menu for an
    /// operation stopped mid-way, and later the `x` menu.
    Menu(menu::MenuState),
    /// `@`: every recorded `git` command, newest last (`docs/PLAN_12_POLISH.md`
    /// P0). `from_bottom` is how many rows the view is scrolled up from the
    /// newest entry; the renderer clamps it to what fits.
    CommandLog {
        from_bottom: usize,
    },
    /// Creating the GitHub repository: the `gh` check, the form, the last
    /// question (`app::create_remote`).
    CreateRemote(create_remote_form::Step),
    /// A dismissible message: a commit failure, "empty commit message", a
    /// branch-op failure, or a merge conflict.
    Note(String),
}
