//! The key-bar question and what a yes does.

use super::diff_cursor::Granule;
use super::git_config_edit;
use std::path::PathBuf;

/// A pending confirmation: a `d` discard (phase 6) or a branch delete
/// (`docs/PLAN_8_BRANCHES.md`), the first *other* thing that needed a
/// yes/no gate — generalized from phase 6's `DiscardPrompt`, which was
/// exactly this shape with `action` fixed to a discard. `PLAN_0_GENERAL.md`:
/// "anything that loses work asks first". `y` runs `action`, `n` / `Esc`
/// cancels; nothing else can happen while it is up, same as the help
/// overlay.
pub(crate) struct ConfirmPrompt {
    pub(crate) message: String,
    pub(crate) action: ConfirmAction,
}

pub(crate) enum ConfirmAction {
    /// The whole file's worktree change (`d` in `Mode::Nav`, Files focused).
    DiscardFile(PathBuf),
    /// A hunk or a line selection (`d` in `Mode::Diff`, worktree side).
    DiscardGranule(Granule),
    /// `d` in `Mode::Nav`, Branches focused: `git branch -d` / `-D`. `force`
    /// is `false` on the first confirm, `true` on the second one offered
    /// after an unmerged-branch refusal (`App::run_confirm`).
    DeleteBranch { name: String, force: bool },
    /// Abort the merge, rebase, cherry-pick or revert in progress (`m` menu).
    AbortOperation,
    /// `d` on the Commits pane: drop that commit with `git rebase -i`.
    DropCommit { hash: String },
    /// `s` on the Commits pane: squash that commit into the one below.
    SquashCommit { hash: String },
    /// `d` on the Stash pane: `git stash drop`, resolved by oid.
    DropStash { oid: String },
    /// `<space>` (apply) or `g` (pop) on the Stash pane: confirmed first, like
    /// drop, since both mutate the working tree with no undo.
    RestoreStash { oid: String, pop: bool },
    /// Push a branch known to be behind its upstream, using a lease guard.
    ForcePush,
    /// First write to the global git config of this session: once confirmed,
    /// the edit that asked carries on (`app::git_config_edit`).
    ConfigGlobal(git_config_edit::GlobalResume),
    /// `d` on the git config screen: unset one value.
    ConfigUnset(git_config_edit::ConfigOp),
    /// `i` on the welcome screen: `git init` in this folder.
    InitRepo(PathBuf),
}
