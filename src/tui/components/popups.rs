//! What can sit over the panes: a popup, a question, a note, and the keys they take.

use crate::git::apply::Granule;
use crate::git::model::{CommitEntry, StashEntry};
use crate::theme::palette::Palette;
use crate::tui::App;
use crate::tui::components::diff::{CommandLogView, CommitPopupView, MenuView, PopupView};
use crate::tui::components::menu;
use crate::tui::components::{commit_editor, create_remote, remote};
use crate::tui::widgets::dialog::Dialog;
use crate::tui::widgets::text_input::{TextInput, TextInputMode};
use crate::tui::widgets::tui_overlay::state::OverlayState;
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Wrap};
use std::path::{Path, PathBuf};

impl App {
    /// Read active popup as one enum, without any animation state.
    pub fn popup_view(&self) -> Option<PopupView<'_>> {
        self.popup_view_with(None)
    }

    /// The active popup for drawing: `overlay` is the commit popup's animation, which
    /// the views of the commit editor and of the stage-everything question carry.
    pub(crate) fn popup_view_with<'a>(
        &'a self,
        overlay: Option<&'a mut OverlayState>,
    ) -> Option<PopupView<'a>> {
        let kind = match self.modal.popup()? {
            Popup::Commit(_) => PopupKind::Commit,
            Popup::CommitAllConfirm => PopupKind::CommitAllConfirm,
            Popup::NewBranch(_) => PopupKind::NewBranch,
            Popup::Stash(_) => PopupKind::Stash,
            Popup::Name(..) => PopupKind::Name,
            Popup::CommandLog { .. } => PopupKind::CommandLog,
            Popup::Menu(_) => PopupKind::Menu,
            Popup::Upstream(_) => PopupKind::Upstream,
            Popup::Askpass(_) => PopupKind::Askpass,
            Popup::CreateRemote(_) => PopupKind::CreateRemote,
            Popup::Note(_) => PopupKind::Note,
        };
        match kind {
            PopupKind::Commit => self.commit_popup_with(overlay).map(PopupView::Commit),
            PopupKind::CommitAllConfirm => Some(PopupView::CommitAllConfirm(overlay)),
            PopupKind::NewBranch => self.new_branch_popup().map(PopupView::NewBranch),
            PopupKind::Stash => self.stash_popup().map(PopupView::Stash),
            PopupKind::Name => self.name_popup().map(PopupView::Name),
            PopupKind::CommandLog => self.command_log_popup().map(PopupView::CommandLog),
            PopupKind::Menu => self.menu_popup().map(PopupView::Menu),
            PopupKind::Upstream => self.upstream_popup().map(PopupView::Upstream),
            PopupKind::Askpass => self.askpass_popup().map(PopupView::Askpass),
            PopupKind::CreateRemote => self.create_remote_view().map(PopupView::CreateRemote),
            PopupKind::Note => self.note_popup().map(PopupView::Note),
        }
    }

    /// The pending discard / branch-delete confirmation message, for the
    /// keybar prompt (`ui::draw_keybar`), or `None` when nothing is
    /// pending.
    pub fn confirm_message(&self) -> Option<&str> {
        self.modal.confirm().map(|p| p.message.as_str())
    }

    /// The new-branch popup's render data, reusing `ui::draw_commit_popup`'s
    /// shape (`docs/PLAN_8_BRANCHES.md`), or `None` when it is not up.
    pub fn new_branch_popup(&self) -> Option<CommitPopupView<'_>> {
        let Some(Popup::NewBranch(buf)) = self.modal.popup() else {
            return None;
        };
        Some(CommitPopupView {
            title: &self.new_branch_title,
            input: buf,
            description: None,
            summary_focused: false,
            overlay_state: None,
            lines: buf.lines(),
            cursor: buf.cursor(),
            toggles: None,
            author: None,
            hints: "Create: Enter | Cancel: Esc",
        })
    }

    /// The stash popup's render data, same shape as the new-branch one.
    pub fn stash_popup(&self) -> Option<CommitPopupView<'_>> {
        let Some(Popup::Stash(buf)) = self.modal.popup() else {
            return None;
        };
        Some(CommitPopupView {
            title: "Stash changes",
            input: buf,
            description: None,
            summary_focused: false,
            overlay_state: None,
            lines: buf.lines(),
            cursor: buf.cursor(),
            toggles: None,
            author: None,
            hints: "Stash: Enter | Cancel: Esc",
        })
    }

    /// A name popup's render data, same shape as the new-branch one.
    pub fn name_popup(&self) -> Option<CommitPopupView<'_>> {
        let Some(Popup::Name(target, input)) = self.modal.popup() else {
            return None;
        };
        Some(CommitPopupView {
            title: target.title.as_str(),
            input,
            description: None,
            summary_focused: false,
            overlay_state: None,
            lines: input.lines(),
            cursor: input.cursor(),
            toggles: None,
            author: None,
            hints: target.hints(),
        })
    }

    /// The `@` viewer's render data: the whole ring, reads included.
    pub fn command_log_popup(&self) -> Option<CommandLogView> {
        let Some(Popup::CommandLog { from_bottom }) = self.modal.popup() else {
            return None;
        };
        Some(CommandLogView {
            records: crate::git::command_log::recent(usize::MAX, true),
            from_bottom: *from_bottom,
        })
    }

    /// The menu's render data: each row is `label (shortcut)`.
    pub fn menu_popup(&self) -> Option<MenuView> {
        let Some(Popup::Menu(menu)) = self.modal.popup() else {
            return None;
        };
        Some(MenuView {
            title: menu.title.clone(),
            rows: menu
                .items
                .iter()
                .map(|item| format!("{}  ({})", item.label, item.shortcut))
                .collect(),
            selected: menu.selected,
            hint: menu.items.get(menu.selected).map_or("", |item| item.hint),
        })
    }

    /// `@`: open the command log viewer, scrolled to the newest entry.
    pub(crate) fn open_command_log(&mut self) {
        if self.modal.popup().is_none() {
            self.modal.open_popup(Popup::CommandLog { from_bottom: 0 });
        }
    }

    /// A dismissible note's message (`ui::draw_note_popup`), or `None` when
    /// none is up.
    pub fn note_popup(&self) -> Option<&str> {
        match self.modal.popup() {
            Some(Popup::Note(msg)) => Some(msg),
            _ => None,
        }
    }

    pub fn upstream_value(&self) -> Option<String> {
        match self.modal.popup() {
            Some(Popup::Upstream(input)) => Some(input.text()),
            _ => None,
        }
    }

    pub fn upstream_popup(&self) -> Option<CommitPopupView<'_>> {
        let Some(Popup::Upstream(input)) = self.modal.popup() else {
            return None;
        };
        Some(CommitPopupView {
            title: "Set upstream",
            input,
            description: None,
            summary_focused: false,
            overlay_state: None,
            lines: input.lines(),
            cursor: input.cursor(),
            toggles: None,
            author: None,
            hints: "Push: Enter | Cancel: Esc",
        })
    }

    /// Every key while a non-commit popup is up. Commit editor routes to
    /// `app::commit`, which owns its separate summary/body key model.
    pub(crate) fn popup_key(&mut self, key: KeyEvent) {
        if matches!(self.modal.popup(), Some(Popup::Commit(_))) {
            self.commit_popup_key(key);
            return;
        }
        if matches!(self.modal.popup(), Some(Popup::CommitAllConfirm)) {
            self.commit_all_confirm_key(key);
            return;
        }
        if matches!(self.modal.popup(), Some(Popup::Askpass(_))) {
            self.askpass_key(key);
            return;
        }
        if matches!(self.modal.popup(), Some(Popup::Menu(_))) {
            self.menu_key(key);
            return;
        }
        if matches!(self.modal.popup(), Some(Popup::CreateRemote(_))) {
            self.create_remote_key(key);
            return;
        }
        let mut dismiss = false;
        let mut create_branch_now = false;
        let mut stash_now = false;
        let mut submit_name_now = false;
        let mut submit_upstream = None;

        match self.modal.popup_mut() {
            None => return,
            Some(Popup::Note(_)) => {
                if matches!(key.code, KeyCode::Esc | KeyCode::Enter) {
                    dismiss = true;
                }
            },
            // Both are routed to their own handlers above.
            Some(
                Popup::Commit(_)
                | Popup::CommitAllConfirm
                | Popup::Menu(_)
                | Popup::Askpass(_)
                | Popup::CreateRemote(_),
            ) => {},
            Some(Popup::NewBranch(buf)) => match key.code {
                KeyCode::Esc => dismiss = true,
                KeyCode::Enter => create_branch_now = true,
                _ => {
                    buf.handle_key_event(key, TextInputMode::SingleLine);
                },
            },
            Some(Popup::CommandLog { from_bottom }) => match key.code {
                KeyCode::Esc | KeyCode::Char('@' | 'q') => dismiss = true,
                other => *from_bottom = scrolled_command_log(*from_bottom, other),
            },
            Some(Popup::Name(_, input)) => match key.code {
                KeyCode::Esc => dismiss = true,
                KeyCode::Enter => submit_name_now = true,
                _ => {
                    input.handle_key_event(key, TextInputMode::SingleLine);
                },
            },
            Some(Popup::Stash(buf)) => match key.code {
                KeyCode::Esc => dismiss = true,
                KeyCode::Enter => stash_now = true,
                _ => {
                    buf.handle_key_event(key, TextInputMode::SingleLine);
                },
            },
            Some(Popup::Upstream(input)) => match key.code {
                KeyCode::Esc => dismiss = true,
                KeyCode::Enter => submit_upstream = Some(input.text()),
                _ => {
                    input.handle_key_event(key, TextInputMode::SingleLine);
                },
            },
        }

        if dismiss {
            self.modal.close_popup();
        }
        if create_branch_now {
            self.do_create_branch();
        }
        if stash_now {
            self.do_stash_push();
        }
        if submit_name_now {
            self.submit_name();
        }
        if let Some(value) = submit_upstream {
            self.submit_upstream(&value);
        }
    }
}

/// Modal state that owns all input while it is up, the same idea as
/// `show_help` today but richer (`docs/PLAN_7_COMMIT.md`).
pub(crate) enum Popup {
    Commit(commit_editor::CommitDraft),
    CommitAllConfirm,
    /// New-branch name input (`docs/PLAN_8_BRANCHES.md`). `Enter` *submits*
    /// here, unlike the commit popup, where `Enter` inserts a newline —
    /// the only behavioural difference from reusing `TextInput` outright.
    NewBranch(TextInput),
    /// A one-line name or message with a purpose: rename a branch or a stash,
    /// a branch at a commit, a stash keeping the index (`app::context_menu`).
    Name(menu::NameTarget, TextInput),
    /// Stash message input, `s` on Files (`docs/PLAN_10_STASH.md`). `Enter`
    /// submits; an empty message lets git write its own.
    Stash(TextInput),
    /// `P` with no upstream: edit `<remote> <branch>` before first push.
    Upstream(TextInput),
    /// A passphrase, password or host-key question from ssh/git during a
    /// remote op (`app::askpass`).
    Askpass(remote::AskpassPrompt),
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
    CreateRemote(create_remote::Step),
    /// A dismissible message: a commit failure, "empty commit message", a
    /// branch-op failure, or a merge conflict.
    Note(String),
}

#[derive(Default)]
pub(crate) enum Modal {
    #[default]
    None,
    Popup(Popup),
    Confirm(ConfirmPrompt),
}

impl Modal {
    pub(crate) fn is_some(&self) -> bool {
        !matches!(self, Self::None)
    }

    pub(crate) fn popup(&self) -> Option<&Popup> {
        match self {
            Self::Popup(popup) => Some(popup),
            _ => None,
        }
    }

    pub(crate) fn popup_mut(&mut self) -> Option<&mut Popup> {
        match self {
            Self::Popup(popup) => Some(popup),
            _ => None,
        }
    }

    /// Show `popup`, replacing whatever was up.
    pub(crate) fn open_popup(&mut self, popup: Popup) {
        *self = Self::Popup(popup);
    }

    /// Close the popup. A question that is up is not a popup and stays.
    pub(crate) fn close_popup(&mut self) {
        if matches!(self, Self::Popup(_)) {
            *self = Self::None;
        }
    }

    pub(crate) fn take_popup(&mut self) -> Option<Popup> {
        match std::mem::take(self) {
            Self::Popup(popup) => Some(popup),
            other => {
                *self = other;
                None
            },
        }
    }

    pub(crate) fn confirm(&self) -> Option<&ConfirmPrompt> {
        match self {
            Self::Confirm(prompt) => Some(prompt),
            _ => None,
        }
    }

    /// Ask `prompt` on the key bar, replacing whatever was up.
    pub(crate) fn ask(&mut self, prompt: ConfirmPrompt) {
        *self = Self::Confirm(prompt);
    }

    /// Withdraw the question. A popup that is up is not a question and stays.
    pub(crate) fn cancel_confirm(&mut self) {
        if matches!(self, Self::Confirm(_)) {
            *self = Self::None;
        }
    }

    pub(crate) fn take_confirm(&mut self) -> Option<ConfirmPrompt> {
        match std::mem::take(self) {
            Self::Confirm(prompt) => Some(prompt),
            other => {
                *self = other;
                None
            },
        }
    }
}

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
    ConfigGlobal(crate::git::config_edit::GlobalResume),
    /// `d` on the git config screen: unset one value.
    ConfigUnset(crate::git::config_edit::ConfigOp),
    /// `i` on the welcome screen: `git init` in this folder.
    InitRepo(PathBuf),
}

impl ConfirmPrompt {
    /// Ask before discarding every change a file has in the worktree.
    pub(crate) fn discard_file(path: &Path) -> Self {
        Self {
            message: format!("discard all changes in {}?", path.display()),
            action: ConfirmAction::DiscardFile(path.to_path_buf()),
        }
    }

    /// Ask before discarding a hunk or some lines of a file.
    pub(crate) fn discard_granule(granule: Granule, path: &Path) -> Self {
        Self {
            message: format!("discard {} in {}?", granule.describe(), path.display()),
            action: ConfirmAction::DiscardGranule(granule),
        }
    }
}

impl ConfirmPrompt {
    /// Ask before deleting a branch (`force` on the second ask, after git
    /// refused an unmerged one).
    pub(crate) fn delete_branch(name: String) -> Self {
        Self {
            message: format!("delete branch {name}?"),
            action: ConfirmAction::DeleteBranch { name, force: false },
        }
    }
}

impl ConfirmPrompt {
    /// Ask before applying or popping a stash entry: both mutate the working
    /// tree at once, with no undo (`docs/PLAN_10_STASH.md`).
    pub(crate) fn restore_stash(entry: &StashEntry, pop: bool) -> Self {
        let verb = if pop { "pop" } else { "apply" };
        Self {
            message: format!("{verb} stash@{{{}}}: {}?", entry.index, entry.message),
            action: ConfirmAction::RestoreStash {
                oid: entry.oid.clone(),
                pop,
            },
        }
    }

    /// Ask before dropping a stash entry, the one irreversible stash action.
    pub(crate) fn drop_stash(entry: &StashEntry) -> Self {
        Self {
            message: format!("drop stash@{{{}}}: {}?", entry.index, entry.message),
            action: ConfirmAction::DropStash {
                oid: entry.oid.clone(),
            },
        }
    }

    /// Ask before dropping a commit (the reflog still has it, but nothing in
    /// ferrit shows that).
    pub(crate) fn drop_commit(entry: &CommitEntry) -> Self {
        Self {
            message: format!("drop {} {}?", entry.short_hash, entry.summary),
            action: ConfirmAction::DropCommit {
                hash: entry.full_hash.clone(),
            },
        }
    }

    /// Ask before squashing a commit into the one below it.
    pub(crate) fn squash_commit(entry: &CommitEntry, below: &CommitEntry) -> Self {
        Self {
            message: format!(
                "squash {} {} into {} {}?",
                entry.short_hash, entry.summary, below.short_hash, below.summary
            ),
            action: ConfirmAction::SquashCommit {
                hash: entry.full_hash.clone(),
            },
        }
    }
}

/// Rows a `PageUp` / `PageDown` moves the command log viewer.
pub(crate) const COMMAND_LOG_PAGE: usize = 10;

/// The viewer's scroll offset (rows up from the newest entry) after `key`.
/// `usize::MAX` means "as far up as it goes"; the renderer clamps it.
pub(crate) fn scrolled_command_log(from_bottom: usize, key: KeyCode) -> usize {
    match key {
        KeyCode::Char('k') | KeyCode::Up => from_bottom.saturating_add(1),
        KeyCode::Char('j') | KeyCode::Down => from_bottom.saturating_sub(1),
        KeyCode::PageUp => from_bottom.saturating_add(COMMAND_LOG_PAGE),
        KeyCode::PageDown => from_bottom.saturating_sub(COMMAND_LOG_PAGE),
        KeyCode::Home | KeyCode::Char('g') => usize::MAX,
        KeyCode::End | KeyCode::Char('G') => 0,
        _ => from_bottom,
    }
}

#[derive(Clone, Copy)]
pub(crate) enum PopupKind {
    Commit,
    CommitAllConfirm,
    NewBranch,
    Stash,
    Name,
    CommandLog,
    Menu,
    Upstream,
    Askpass,
    CreateRemote,
    Note,
}

pub(crate) fn draw_note(frame: &mut Frame<'_>, area: Rect, message: &str, palette: &Palette) {
    let width = 60.min(area.width);
    let content_lines = message.lines().count().max(1);
    let body_rows = u16::try_from(content_lines)
        .unwrap_or(u16::MAX)
        .saturating_add(1);
    let warn = Style::new().fg(palette.del).add_modifier(Modifier::BOLD);
    let dialog = Dialog::new(Line::styled(" commit ", warn))
        .fit_content(width, body_rows, 1)
        .border_style(warn)
        .render(frame, area);
    frame.render_widget(
        Paragraph::new(message.to_owned()).wrap(Wrap { trim: false }),
        dialog.body,
    );
    frame.render_widget(
        Paragraph::new(Line::styled(
            "Esc / Enter to dismiss",
            Style::new().fg(palette.idle),
        )),
        dialog.footer,
    );
}

#[cfg(test)]
mod tests {
    use super::{COMMAND_LOG_PAGE, KeyCode, scrolled_command_log};

    #[test]
    fn k_and_j_move_one_row_and_stop_at_the_newest() {
        assert_eq!(scrolled_command_log(3, KeyCode::Char('k')), 4);
        assert_eq!(scrolled_command_log(3, KeyCode::Char('j')), 2);
        assert_eq!(scrolled_command_log(0, KeyCode::Char('j')), 0);
    }

    #[test]
    fn pages_jump_and_the_ends_snap() {
        assert_eq!(scrolled_command_log(0, KeyCode::PageUp), COMMAND_LOG_PAGE);
        assert_eq!(scrolled_command_log(4, KeyCode::PageDown), 0);
        assert_eq!(scrolled_command_log(7, KeyCode::Char('g')), usize::MAX);
        assert_eq!(scrolled_command_log(7, KeyCode::Char('G')), 0);
    }

    #[test]
    fn scrolling_up_from_the_far_end_does_not_wrap() {
        assert_eq!(
            scrolled_command_log(usize::MAX, KeyCode::Char('k')),
            usize::MAX
        );
    }

    #[test]
    fn other_keys_leave_the_offset_alone() {
        assert_eq!(scrolled_command_log(5, KeyCode::Char('x')), 5);
    }
}
