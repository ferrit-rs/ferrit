//! What the screens may read of `App`: a bundle of references to its state, and
//! the questions drawing asks of them. Drawing takes a `Scene`, never `App`, so
//! it cannot reach what it does not need and cannot change anything.

use std::ops::Range;
use std::sync::Arc;

use ratatui::text::Line;

use crate::config::Config;
use crate::config::settings::{SettingsRow, SettingsSheet};
use crate::git::Snapshot;
use crate::git::diff::DiffSide;
use crate::git::host::CreateRemote;
use crate::git::port::GitPort;
use crate::theme::palette::Palette;
use crate::tui::App;
use crate::tui::components::create_remote::{Consequences, CreateRemoteView};
use crate::tui::components::dashboard::{Dashboard, Sheet};
use crate::tui::components::diff::{
    CommandLogView, CommitPopupView, DiffView, MenuView, Mode, PopupView, RightPane,
};
use crate::tui::components::help::HelpState;
use crate::tui::components::keybar::{HelpLine, help_lines};
use crate::tui::components::panes::{Nav, Pane, PaneRows};
use crate::tui::components::popups::{Modal, Popup, PopupKind};
use crate::tui::components::settings;
use crate::tui::draw::FullScreen;
use crate::tui::error::AppError;
use crate::tui::keymap::Context;
use crate::tui::prefs::Prefs;
use crate::tui::row_lines;
use crate::tui::widgets::tui_overlay::state::OverlayState;
use crate::tui::workers::Workers;

/// A read-only view of the app for one frame, or one question.
#[derive(Clone, Copy)]
pub(crate) struct Scene<'a> {
    pub(crate) nav: &'a Nav,
    pub(crate) snapshot: &'a Snapshot,
    pub(crate) theme: &'a settings::ThemeEditor,
    pub(crate) prefs: &'a Prefs,
    pub(crate) right: &'a RightPane,
    pub(crate) sheets: &'a crate::tui::components::dashboard::Sheets,
    pub(crate) help: &'a HelpState,
    pub(crate) full_screens: &'a crate::tui::draw::FullScreens,
    pub(crate) modal: &'a Modal,
    pub(crate) hits: &'a crate::tui::components::panes::HitAreas,
    pub(crate) authorship: &'a crate::git::identity::Authorship,
    pub(crate) workers: &'a Workers,
    pub(crate) repo: &'a Option<Box<dyn GitPort>>,
    pub(crate) repo_name: &'a str,
    pub(crate) last_error: &'a Option<Arc<AppError>>,
    pub(crate) status_note: &'a Option<String>,
    pub(crate) new_branch_title: &'a str,
    pub(crate) create_remote: &'a CreateRemote,
}

impl App {
    /// What drawing may read of the app.
    pub(crate) fn scene(&self) -> Scene<'_> {
        Scene {
            nav: &self.nav,
            snapshot: &self.snapshot,
            theme: &self.theme,
            prefs: &self.prefs,
            right: &self.right,
            sheets: &self.sheets,
            help: &self.help,
            full_screens: &self.full_screens,
            modal: &self.modal,
            hits: &self.hits,
            authorship: &self.authorship,
            workers: &self.workers,
            repo: &self.repo,
            repo_name: &self.repo_name,
            last_error: &self.last_error,
            status_note: &self.status_note,
            new_branch_title: &self.new_branch_title,
            create_remote: &self.create_remote,
        }
    }
}

impl<'a> Scene<'a> {
    /// The panes' rows, to read the selection.
    pub(crate) fn rows(self) -> PaneRows<'a> {
        PaneRows {
            nav: self.nav,
            snapshot: self.snapshot,
            palette: &self.prefs.palette,
        }
    }

    /// The palette every screen and line builder colours with.
    pub(crate) fn palette(self) -> Palette {
        self.prefs.palette
    }

    /// Current right-pane diff.
    pub(crate) fn diff_view(self) -> &'a DiffView {
        &self.right.diff
    }

    /// First visible line of the right-pane diff.
    pub(crate) fn right_scroll(self) -> usize {
        self.right.scroll
    }

    /// The view that replaces the panes, `FullScreen::None` for the normal screen.
    pub(crate) fn full_screen(self) -> FullScreen {
        self.full_screens.active
    }

    /// The dashboard's state.
    pub(crate) fn dashboard(self) -> &'a Dashboard {
        &self.sheets.dashboard
    }

    /// Configured Git author name, shown in the Info panel header when set.
    pub(crate) fn git_user_name(self) -> Option<&'a str> {
        self.authorship.git_user_name.as_deref()
    }

    /// Whether this is the repo-free sample.
    pub(crate) fn is_mock(self) -> bool {
        self.repo.is_none()
    }

    /// Whether the right pane was last clicked, for its border highlight.
    pub(crate) fn right_focused(self) -> bool {
        self.nav.right_focused
    }

    /// A left pane's list scroll offset.
    pub(crate) fn list_offset(self, pane: Pane) -> usize {
        self.hits.list_offset(pane)
    }

    /// Whether a wheel scroll left `pane`'s view away from its selection.
    pub(crate) fn view_detached(self, pane: Pane) -> bool {
        self.hits.view_detached(pane, self.nav.selection[pane])
    }

    /// Selectable row count for a pane.
    pub(crate) fn row_count(self, pane: Pane) -> usize {
        self.rows().row_count(pane)
    }

    /// `(current, total)` for the pane's `N of M` border counter.
    pub(crate) fn counter(self, pane: Pane) -> Option<(usize, usize)> {
        self.rows().counter(pane)
    }

    /// Selection cursor for a given pane.
    pub(crate) fn selected(self, pane: Pane) -> usize {
        self.nav.selected(pane)
    }

    pub(crate) fn file_lines(self) -> Vec<Line<'static>> {
        self.rows().file_lines()
    }

    pub(crate) fn files_selection_is_dir(self) -> bool {
        self.rows().files_selection_is_dir()
    }

    pub(crate) fn branch_lines(self) -> Vec<Line<'static>> {
        self.rows()
            .branch_lines(self.workers.remote_branch_status().as_deref())
    }

    pub(crate) fn branches_title(self) -> String {
        self.rows().branches_title()
    }

    pub(crate) fn branches_drilled(self) -> bool {
        self.nav.branches_drilled()
    }

    pub(crate) fn commit_lines(self) -> Vec<Line<'static>> {
        self.rows().commit_lines()
    }

    pub(crate) fn commits_title(self) -> String {
        self.rows().commits_title()
    }

    pub(crate) fn commits_drilled(self) -> bool {
        self.nav.commits_drilled()
    }

    pub(crate) fn stash_lines(self) -> Vec<Line<'static>> {
        self.rows().stash_lines()
    }

    /// Status pane: lazygit's one-liner `ferrit -> main ↑2`, plus a conflict
    /// line only when there are conflicts, or the error when `refresh()` failed.
    pub(crate) fn status_lines(self) -> Vec<Line<'static>> {
        let mut out = if let Some(err) = self.last_error {
            vec![row_lines::error_line(
                &self.prefs.palette,
                &format!("error: {err}"),
            )]
        } else {
            let h = &self.snapshot.header;
            let line = row_lines::status_header(self.repo_name, h);
            let mut lines = vec![row_lines::status_line(&self.prefs.palette, &line)];
            if h.conflicts > 0 {
                lines.push(row_lines::error_line(
                    &self.prefs.palette,
                    &format!("\u{2717} {} merge conflict(s)", h.conflicts),
                ));
            }
            lines
        };
        if let Some(operation) = self.snapshot.operation {
            // Right under the first line, error or header, so it is the
            // first thing read while git waits on the user.
            out.insert(
                out.len().min(1),
                row_lines::operation_line(&self.prefs.palette, &operation.label()),
            );
        }
        if let Some(label) = self.workers.remote_busy_label() {
            out.push(row_lines::busy_line(&self.prefs.palette, label));
        } else if self.last_error.is_none()
            && let Some(note) = self.status_note
        {
            out.push(row_lines::status_line(&self.prefs.palette, note));
        }
        out
    }

    /// The contexts a key is looked up in, most specific first: the diff
    /// cursor while it is up, then the focused pane, then everything.
    pub(crate) fn key_contexts(self) -> Vec<Context> {
        let mut contexts = Vec::with_capacity(3);
        if self.nav.mode == Mode::Diff {
            contexts.push(Context::Diff);
        }
        contexts.extend(Context::for_pane(self.nav.focus));
        contexts.push(Context::Global);
        contexts
    }

    /// The repository has no commit yet, asked of git itself: the app's own list
    /// of commits can be a refresh behind, right after a first commit.
    pub(crate) fn repo_has_no_commit(self) -> bool {
        self.repo.as_ref().is_some_and(|repo| !repo.has_commits())
    }

    /// The help screen's content for the focused pane, from the live keymap.
    pub(crate) fn help_lines(self) -> Vec<HelpLine> {
        help_lines(&self.prefs.keymap, &self.key_contexts())
    }

    /// The commit popup as data, without any animation state.
    pub(crate) fn commit_popup(self) -> Option<CommitPopupView<'a>> {
        self.commit_popup_with(None)
    }

    /// The commit popup for drawing: `overlay` is its animation.
    pub(crate) fn commit_popup_with<'b>(
        self,
        overlay: Option<&'b mut OverlayState>,
    ) -> Option<CommitPopupView<'b>>
    where
        'a: 'b,
    {
        let Some(Popup::Commit(draft)) = self.modal.popup() else {
            return None;
        };
        Some(draft.view(self.authorship.line(), overlay))
    }

    /// Cursor state for the right-pane render: `(side, cursor line, V-select
    /// range)` while `Mode::Diff` is up, else `None`.
    pub(crate) fn diff_cursor(self) -> Option<(DiffSide, usize, Option<Range<usize>>)> {
        (self.nav.mode == Mode::Diff).then(|| self.right.cursor_view())
    }

    /// Right-pane title suffix while `Mode::Diff` is up: `hunk 1/3` or
    /// `lines 41-42`/`line 41`, so it is obvious what `<space>` will hit.
    pub(crate) fn diff_granule_hint(self) -> Option<String> {
        if self.nav.mode != Mode::Diff {
            return None;
        }
        self.right.granule_hint()
    }

    /// The credential prompt as data, when it is up.
    pub(crate) fn askpass_popup(self) -> Option<CommitPopupView<'a>> {
        let Some(Popup::Askpass(ask)) = self.modal.popup() else {
            return None;
        };
        Some(CommitPopupView {
            title: ask.prompt.trim_end().trim_end_matches(':'),
            input: &ask.shown,
            description: None,
            summary_focused: false,
            overlay_state: None,
            lines: ask.shown.lines(),
            cursor: ask.shown.cursor(),
            toggles: None,
            author: None,
            hints: "Send: Enter | Cancel: Esc",
        })
    }

    /// The welcome screen's highlighted choice.
    pub(crate) fn welcome_selected(self) -> usize {
        self.full_screens.welcome_selected
    }

    /// The folder the welcome screen offers to `git init`.
    pub(crate) fn welcome_dir(self) -> Option<&'a std::path::Path> {
        self.full_screens.welcome_dir.as_deref()
    }

    /// Read active popup as one enum, without any animation state.
    pub(crate) fn popup_view(self) -> Option<PopupView<'a>> {
        self.popup_view_with(None)
    }

    /// The active popup for drawing: `overlay` is the commit popup's animation, which
    /// the views of the commit editor and of the stage-everything question carry.
    pub(crate) fn popup_view_with<'b>(
        self,
        overlay: Option<&'b mut OverlayState>,
    ) -> Option<PopupView<'b>>
    where
        'a: 'b,
    {
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
    pub(crate) fn confirm_message(self) -> Option<&'a str> {
        self.modal.confirm().map(|p| p.message.as_str())
    }

    /// The new-branch popup's render data, reusing `ui::draw_commit_popup`'s
    /// shape (`docs/PLAN_8_BRANCHES.md`), or `None` when it is not up.
    pub(crate) fn new_branch_popup(self) -> Option<CommitPopupView<'a>> {
        let Some(Popup::NewBranch(buf)) = self.modal.popup() else {
            return None;
        };
        Some(CommitPopupView {
            title: self.new_branch_title,
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
    pub(crate) fn stash_popup(self) -> Option<CommitPopupView<'a>> {
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
    pub(crate) fn name_popup(self) -> Option<CommitPopupView<'a>> {
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
    pub(crate) fn command_log_popup(self) -> Option<CommandLogView> {
        let Some(Popup::CommandLog { from_bottom }) = self.modal.popup() else {
            return None;
        };
        Some(CommandLogView {
            records: crate::git::command_log::recent(usize::MAX, true),
            from_bottom: *from_bottom,
        })
    }

    /// The menu's render data: each row is `label (shortcut)`.
    pub(crate) fn menu_popup(self) -> Option<MenuView> {
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

    /// A dismissible note's message (`ui::draw_note_popup`), or `None` when
    /// none is up.
    pub(crate) fn note_popup(self) -> Option<&'a str> {
        match self.modal.popup() {
            Some(Popup::Note(msg)) => Some(msg),
            _ => None,
        }
    }

    pub(crate) fn upstream_value(self) -> Option<String> {
        match self.modal.popup() {
            Some(Popup::Upstream(input)) => Some(input.text()),
            _ => None,
        }
    }

    pub(crate) fn upstream_popup(self) -> Option<CommitPopupView<'a>> {
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

    /// The settings sheet's state, for the screen that draws it and for tests.
    pub(crate) fn settings(self) -> &'a SettingsSheet {
        &self.sheets.settings
    }

    /// The live configuration: what ferrit is using now, saved or not.
    #[doc(hidden)]
    pub(crate) fn live_config(self) -> &'a Config {
        &self.prefs.config
    }

    /// The row's value when it is a choice: the index among its names.
    #[must_use]
    pub(crate) fn choice_index(self, row: SettingsRow) -> Option<usize> {
        settings::choice_index(self.theme, row)
    }

    /// The row's value when it is a toggle.
    #[must_use]
    pub(crate) fn toggle_value(self, row: SettingsRow) -> bool {
        settings::toggle_value(&self.prefs.config, row)
    }

    /// The row's value when it is a number.
    #[must_use]
    pub(crate) fn number_value(self, row: SettingsRow) -> u64 {
        settings::number_value(&self.prefs.config, row)
    }

    /// The git config screen's state, for the screen that draws it and for tests.
    pub(crate) fn git_config(self) -> &'a crate::tui::components::git_config::GitConfigScreen {
        &self.full_screens.git_config
    }

    /// The creation's state, for the popups and for tests.
    pub(crate) fn create_remote(self) -> &'a CreateRemote {
        self.create_remote
    }

    /// What the renderer draws, or `None` when this popup is not up.
    pub(crate) fn create_remote_view(self) -> Option<CreateRemoteView<'a>> {
        let Some(Popup::CreateRemote(step)) = self.modal.popup() else {
            return None;
        };
        let aliases = self.create_remote.ssh_aliases();
        Some(step.view(&Consequences {
            first_commit: self.repo_has_no_commit(),
            ssh_host: aliases.first().map(String::as_str),
            branch: &self.snapshot.header.branch,
        }))
    }

    /// `dashboard_is_open` for a drawer animation held elsewhere: drawing owns the
    /// render state while it runs, so it asks with its own.
    pub(crate) fn dashboard_open_in(self, drawer: &OverlayState) -> bool {
        self.sheets.kind == Sheet::Dashboard && !drawer.is_closed() && !drawer.is_closing()
    }
}
