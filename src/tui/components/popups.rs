//! What can sit over the panes: a popup, a question, a note, and the keys they take.

use crate::git::apply::Granule;
use crate::git::error::GitError;
use crate::git::model::{CommitEntry, StashEntry};
use crate::git::rebase::RebaseEdit;
use crate::git::rebase::Step;
use crate::git::remote::RemoteRequest;
use crate::theme::palette::Palette;
use crate::tui::components::menu;
use crate::tui::components::remote as askpass;
use crate::tui::components::{commit_editor, create_remote};
use crate::tui::components::{commits, files};
use crate::tui::event::{Env, Event};
use crate::tui::widgets::chrome::dialog::Dialog;
use crate::tui::widgets::text_input::{TextInput, TextInputMode};
use ratatui::Frame;
use ratatui::crossterm::event::{KeyCode, KeyEvent};
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Wrap};
use std::path::{Path, PathBuf};

/// `@`: open the command log viewer, scrolled to the newest entry.
pub(crate) fn open_command_log(env: &Env<'_>) -> Vec<Event> {
    if env.popup_up {
        return Vec::new();
    }
    vec![Event::OpenPopup(Popup::CommandLog { from_bottom: 0 })]
}

/// Every key while a popup that is just a text box, a note or the command log is
/// up. The ones with their own keys (the commit editor, the menus, the credential
/// prompt, the create-a-repository flow) are routed before.
pub(crate) fn text_key(popup: &mut Popup, key: KeyEvent) -> Vec<Event> {
    match popup {
        Popup::Note(_) => {
            if matches!(key.code, KeyCode::Esc | KeyCode::Enter) {
                vec![Event::ClosePopup]
            } else {
                Vec::new()
            }
        },
        Popup::NewBranch(buf) => match key.code {
            KeyCode::Esc => vec![Event::ClosePopup],
            KeyCode::Enter => vec![Event::SubmitNewBranch(buf.text())],
            _ => edit(buf, key),
        },
        Popup::CommandLog { from_bottom } => match key.code {
            KeyCode::Esc | KeyCode::Char('@' | 'q') => vec![Event::ClosePopup],
            other => {
                *from_bottom = scrolled_command_log(*from_bottom, other);
                Vec::new()
            },
        },
        Popup::Name(target, input) => match key.code {
            KeyCode::Esc => vec![Event::ClosePopup],
            KeyCode::Enter => vec![Event::SubmitName {
                kind: target.kind.clone(),
                text: input.text(),
            }],
            _ => edit(input, key),
        },
        Popup::Stash(buf) => match key.code {
            KeyCode::Esc => vec![Event::ClosePopup],
            KeyCode::Enter => vec![Event::PushStash(buf.text())],
            _ => edit(buf, key),
        },
        Popup::Upstream(input) => match key.code {
            KeyCode::Esc => vec![Event::ClosePopup],
            KeyCode::Enter => vec![Event::SubmitUpstream(input.text())],
            _ => edit(input, key),
        },
        Popup::Commit(_)
        | Popup::CommitAllConfirm
        | Popup::Menu(_)
        | Popup::Askpass(_)
        | Popup::CreateRemote(_) => Vec::new(),
    }
}

fn edit(input: &mut TextInput, key: KeyEvent) -> Vec<Event> {
    input.handle_key_event(key, TextInputMode::SingleLine);
    Vec::new()
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
    /// the edit that asked carries on (`git_config::edit`).
    ConfigGlobal(crate::tui::components::git_config::edit::GlobalResume),
    /// `d` on the git config screen: unset one value.
    ConfigUnset(crate::tui::components::git_config::edit::ConfigOp),
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

/// The question was answered yes: do what it asked. A branch delete refused for
/// being unmerged re-opens the question one more time asking to force it, rather
/// than reporting the refusal and stopping: `git branch -d` is offering a choice,
/// not failing outright.
pub(crate) fn confirm(action: ConfirmAction, env: &Env<'_>) -> Vec<Event> {
    match action {
        ConfirmAction::DiscardFile(path) => files::discard_file(env, &path),
        ConfirmAction::DiscardGranule(granule) => files::discard_granule(env, granule),
        ConfirmAction::DeleteBranch { name, force } => {
            let Some(repo) = env.repo else {
                return Vec::new();
            };
            match repo.delete_branch(&name, force) {
                Ok(()) => vec![Event::Refresh],
                Err(GitError::BranchNotMerged(_)) if !force => {
                    vec![Event::Ask(ConfirmPrompt {
                        message: format!(
                            "'{name}' is not fully merged. Force delete? This may lose \
                             commits with no other reference to them."
                        ),
                        action: ConfirmAction::DeleteBranch { name, force: true },
                    })]
                },
                Err(e) => vec![Event::Report(e.into())],
            }
        },
        ConfirmAction::DropStash { oid } => vec![Event::DropStash(oid)],
        ConfirmAction::RestoreStash { oid, pop } => vec![Event::RestoreStash { oid, pop }],
        ConfirmAction::DropCommit { hash } => {
            commits::rebase_edit(env.repo, &hash, &RebaseEdit::Drop)
        },
        ConfirmAction::SquashCommit { hash } => {
            commits::rebase_edit(env.repo, &hash, &RebaseEdit::Squash)
        },
        ConfirmAction::AbortOperation => vec![Event::OperationStep(Step::Abort)],
        ConfirmAction::ConfigGlobal(resume) => vec![Event::ResumeGitConfigEdit(resume)],
        ConfirmAction::InitRepo(dir) => vec![Event::InitRepo(dir)],
        ConfirmAction::ConfigUnset(op) => vec![Event::ConfigUnset(op)],
        ConfirmAction::ForcePush => vec![Event::StartRemote(RemoteRequest::force_push())],
    }
}

#[cfg(test)]
mod tests {
    use crate::tui::components::popups::{COMMAND_LOG_PAGE, scrolled_command_log};
    use ratatui::crossterm::event::KeyCode;

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
