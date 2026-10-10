//! Keymap action dispatch and navigation controller.

use crate::git;
use crate::git::remote::RemoteOp;
use crate::tui::App;
use crate::tui::components::panes::nav::{PANES, Pane};
use crate::tui::components::{branches, commits, files, menu, popups, remote, stash};
use crate::tui::event::Event;
use crate::tui::keymap::action::Action;

impl App {
    /// Run one resolved keymap action.
    pub(crate) fn run_action(&mut self, action: Action) {
        match action {
            Action::Quit => self.should_quit = true,
            Action::Help => self.open_help(),
            Action::CommandLog => self.apply(popups::open_command_log(&self.env())),
            Action::Dashboard => self.open_dashboard(),
            Action::GitConfig => self.open_git_config(),
            Action::CreateRemote => self.open_create_remote(),
            Action::OperationMenu => self.apply(menu::open_operation(&self.env())),
            Action::ContextMenu => self.apply(menu::open_context(&self.env())),
            Action::Back => self.go_back(),
            Action::Enter => self.enter_selected(),
            Action::EnterDiff => self.apply(files::enter_diff(&self.env())),
            Action::Refresh => self.request_refresh(),
            Action::Fetch => self.apply(remote::trigger(RemoteOp::Fetch)),
            Action::Pull => self.apply(remote::trigger(RemoteOp::Pull)),
            Action::Push => self.apply(remote::push(&self.env())),
            Action::Commit => self.open_commit(git::commit::CommitKind::Normal),
            Action::Amend => self.open_commit(git::commit::CommitKind::Amend),
            Action::RewordHead => self.open_commit(git::commit::CommitKind::Reword),
            Action::Focus(pane) => self.focus_pane(pane),
            Action::NextPane => self.focus_pane(self.pane_offset(1)),
            Action::PrevPane => self.focus_pane(self.pane_offset(PANES.len() - 1)),
            Action::ToggleBranchesTab => self.nav.toggle_branches_tab(),
            Action::SelectDown => self.select_down(),
            Action::SelectUp => self.select_up(),
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
            Action::StageFile => self.apply(files::stage_selected(&self.env())),
            Action::StageAll => self.apply(files::stage_all(&self.env())),
            Action::Discard => self.apply(files::discard_prompt(&self.env())),
            Action::StashPush => self.apply(stash::open_popup(&self.env())),
            Action::LeaveDiff => self.apply(vec![Event::LeaveDiff]),
            Action::CursorDown => self.right.move_cursor(1),
            Action::CursorUp => self.right.move_cursor(-1),
            Action::CursorNextHunk => self.right.jump_cursor_hunk(1),
            Action::CursorPrevHunk => self.right.jump_cursor_hunk(-1),
            Action::ToggleSelection => self.right.toggle_anchor(),
            Action::StageCursor => self.apply(files::stage_cursor(&self.env())),
            Action::Checkout => self.apply(branches::checkout(&self.env())),
            Action::NewBranch => self.apply(branches::open_new_popup(&self.env())),
            Action::FastForward => self.apply(branches::fast_forward(&self.env())),
            Action::Merge => self.apply(branches::merge(&self.env())),
            Action::DeleteBranch => self.apply(branches::delete_prompt(&self.env())),
            Action::RewordCommit => self.apply(commits::reword(&self.env())),
            Action::DropCommit => self.apply(commits::drop_prompt(&self.env())),
            Action::Squash => self.apply(commits::fold(&self.env(), false)),
            Action::Fixup => self.apply(commits::fold(&self.env(), true)),
            Action::EditCommit => self.apply(commits::edit(&self.env())),
            Action::NewFixup => self.apply(commits::new_fixup(&self.env())),
            Action::Autosquash => self.apply(commits::autosquash(&self.env())),
            Action::ApplyStash => self.apply(stash::restore_prompt(&self.env(), false)),
            Action::PopStash => self.apply(stash::restore_prompt(&self.env(), true)),
            Action::DropStash => self.apply(stash::drop_prompt(&self.env())),
        }
    }

    /// Scroll action inside right diff pane.
    pub(crate) fn run_scroll(&mut self, action: Action) {
        let half = isize::try_from((self.right.viewport / 2).max(1)).unwrap_or(isize::MAX);
        let page =
            isize::try_from(self.right.viewport.saturating_sub(1).max(1)).unwrap_or(isize::MAX);
        match action {
            Action::ScrollHalfDown => self.right.scroll_by(half),
            Action::ScrollHalfUp => self.right.scroll_by(-half),
            Action::ScrollLineDown => self.right.scroll_by(1),
            Action::ScrollLineUp => self.right.scroll_by(-1),
            Action::ScrollPageDown => self.right.scroll_by(page),
            Action::ScrollPageUp => self.right.scroll_by(-page),
            Action::ScrollBottom => self.right.scroll_by(isize::MAX),
            Action::ScrollTop => self.right.scroll_by(isize::MIN),
            Action::NextHunk => self.right.jump_anchor(1),
            Action::PrevHunk => self.right.jump_anchor(-1),
            _ => {},
        }
    }

    pub(crate) fn is_scroll(action: Action) -> bool {
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

    fn go_back(&mut self) {
        self.nav.right_focused = false;
        if let Some(drill) = self.nav.branch_drill.take() {
            self.nav.selection[Pane::Branches] = drill.return_index;
        }
        if let Some(drill) = self.nav.commit_drill.take() {
            self.nav.selection[Pane::Commits] = drill.return_index;
        }
    }

    fn enter_selected(&mut self) {
        self.apply(branches::enter_log(&self.env()));
        let opts = self.prefs.diff_opts();
        let (drilled, events) = files::keys::enter_commit_files(&self.env(), opts);
        self.apply(events);
        if !drilled {
            self.apply(files::keys::toggle_files_dir(&self.env()));
            self.apply(files::keys::toggle_commit_dir(&self.env()));
            self.apply(files::enter_diff(&self.env()));
        }
    }

    fn focus_pane(&mut self, pane: Pane) {
        self.nav.right_focused = false;
        self.nav.focus = pane;
    }

    pub(crate) fn open_help(&mut self) {
        self.help.show();
    }
}
