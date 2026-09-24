//! Turns a key into an `Action` through the keymap and runs it. What used to
//! be three stages of `on_key` (the `Mode::Diff` cursor keys, the right-pane
//! scroll block, then the main `match`) is one lookup over contexts, most
//! specific first. See `docs/PLAN_12_POLISH.md` P2.

use ratatui::crossterm::event::KeyEvent;

use super::keymap::{Action, Context, KeyBinding};
use super::{App, Mode, PANES, Pane, events, git};

impl App {
    /// The contexts a key is looked up in, most specific first: the diff
    /// cursor while it is up, then the focused pane, then everything.
    pub(super) fn key_contexts(&self) -> Vec<Context> {
        let mut contexts = Vec::with_capacity(3);
        if self.mode == Mode::Diff {
            contexts.push(Context::Diff);
        }
        contexts.extend(Context::for_pane(self.focus));
        contexts.push(Context::Global);
        contexts
    }

    /// Run the key's action, if it has one. Scrolling the right pane skips
    /// the preview rebuild (and its diff subprocess): it changes no selection.
    /// Every other key ends by re-syncing the preview, since focus or
    /// selection may have moved.
    pub(super) fn dispatch_key(&mut self, key: KeyEvent) {
        let binding = KeyBinding::from_event(key);
        let action = self.keymap.resolve(&self.key_contexts(), binding);
        if let Some(action) = action {
            if is_scroll(action) && self.right_is_diff() {
                self.run_scroll(action);
                return;
            }
            self.run_action(action);
        }
        self.update_right_pane();
    }

    fn run_scroll(&mut self, action: Action) {
        let half = isize::try_from((self.right_viewport / 2).max(1)).unwrap_or(isize::MAX);
        let page =
            isize::try_from(self.right_viewport.saturating_sub(1).max(1)).unwrap_or(isize::MAX);
        match action {
            Action::ScrollHalfDown => self.scroll_right(half),
            Action::ScrollHalfUp => self.scroll_right(-half),
            Action::ScrollLineDown => self.scroll_right(1),
            Action::ScrollLineUp => self.scroll_right(-1),
            Action::ScrollPageDown => self.scroll_right(page),
            Action::ScrollPageUp => self.scroll_right(-page),
            Action::ScrollBottom => self.scroll_right(isize::MAX),
            Action::ScrollTop => self.scroll_right(isize::MIN),
            Action::NextHunk => self.jump_diff_anchor(1),
            Action::PrevHunk => self.jump_diff_anchor(-1),
            _ => {},
        }
    }

    fn run_action(&mut self, action: Action) {
        match action {
            Action::Quit => self.should_quit = true,
            Action::Help => self.show_help = true,
            Action::CommandLog => self.open_command_log(),
            Action::OperationMenu => self.open_operation_menu(),
            Action::Back => {
                self.right_focused = false;
                if let Some(drill) = self.branch_drill.take() {
                    self.selection[Pane::Branches] = drill.return_index;
                }
                if let Some(drill) = self.commit_drill.take() {
                    self.selection[Pane::Commits] = drill.return_index;
                }
            },
            Action::Enter => {
                self.enter_branch_log();
                self.enter_commit_files();
                self.toggle_files_dir();
                self.toggle_commit_dir();
                self.enter_diff_mode();
            },
            Action::EnterDiff => self.enter_diff_mode(),
            Action::Refresh => self.request_refresh(),
            Action::Fetch => self.trigger_remote_op(events::RemoteOp::Fetch),
            Action::Pull => self.trigger_remote_op(events::RemoteOp::Pull),
            Action::Push => self.push_current_branch(),
            Action::Commit => self.open_commit(git::commit::CommitKind::Normal),
            Action::Amend => self.open_commit(git::commit::CommitKind::Amend),
            Action::RewordHead => self.open_commit(git::commit::CommitKind::Reword),
            Action::Focus(pane) => self.focus = pane,
            Action::NextPane => self.focus = self.pane_offset(1),
            Action::PrevPane => self.focus = self.pane_offset(PANES.len() - 1),
            Action::ToggleBranchesTab => self.toggle_branches_tab(),
            Action::SelectDown => self.select_down(),
            Action::SelectUp => self.select_up(),
            // Only meaningful over a real diff; anywhere else they do nothing.
            Action::ScrollLineDown
            | Action::ScrollLineUp
            | Action::ScrollPageDown
            | Action::ScrollPageUp
            | Action::ScrollHalfDown
            | Action::ScrollHalfUp
            | Action::ScrollTop
            | Action::ScrollBottom
            | Action::NextHunk
            | Action::PrevHunk => {},
            Action::StageFile => self.stage_selected_file(),
            Action::StageAll => self.stage_all_files(),
            Action::Discard => self.discard_prompt(),
            Action::StashPush => self.open_stash_popup(),
            Action::LeaveDiff => self.leave_diff_mode(),
            Action::CursorDown => self.move_diff_cursor(1),
            Action::CursorUp => self.move_diff_cursor(-1),
            Action::CursorNextHunk => self.jump_diff_cursor_hunk(1),
            Action::CursorPrevHunk => self.jump_diff_cursor_hunk(-1),
            Action::ToggleSelection => self.toggle_diff_anchor(),
            Action::StageCursor => self.stage_diff_cursor(),
            Action::Checkout => self.checkout_selected_branch(),
            Action::NewBranch => self.open_new_branch_popup(),
            Action::FastForward => self.fast_forward_selected_branch(),
            Action::Merge => self.merge_selected_branch(),
            Action::DeleteBranch => self.delete_branch_prompt(),
            Action::RewordCommit => self.reword_selected_commit(),
            Action::DropCommit => self.drop_commit_prompt(),
            Action::Squash => self.fold_selected_commit(false),
            Action::Fixup => self.fold_selected_commit(true),
            Action::EditCommit => self.edit_selected_commit(),
            Action::NewFixup => self.create_fixup_commit(),
            Action::Autosquash => self.autosquash_from_selected(),
            Action::ApplyStash => self.restore_selected_stash(false),
            Action::PopStash => self.restore_selected_stash(true),
            Action::DropStash => self.drop_stash_prompt(),
        }
    }
}

/// Actions that scroll or jump the right pane instead of changing selection.
const fn is_scroll(action: Action) -> bool {
    matches!(
        action,
        Action::ScrollLineDown
            | Action::ScrollLineUp
            | Action::ScrollPageDown
            | Action::ScrollPageUp
            | Action::ScrollHalfDown
            | Action::ScrollHalfUp
            | Action::ScrollTop
            | Action::ScrollBottom
            | Action::NextHunk
            | Action::PrevHunk
    )
}
