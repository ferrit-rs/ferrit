//! The read-only questions the screens and the tests ask of `App`.

use crate::tui::components::diff::CommandLogView;
use crate::tui::components::diff::MenuView;
use crate::tui::components::diff::PopupView;
use crate::tui::components::popups::PopupKind;
use crate::tui::widgets::tui_overlay::state::OverlayState;
use std::ops::Range;

use crate::config::Config;
use crate::config::settings::{SettingsRow, SettingsSheet};
use crate::git::diff::DiffSide;
use crate::tui::App;
use crate::tui::components::diff::{CommitPopupView, Mode};
use crate::tui::components::popups::Popup;
use crate::tui::components::settings;

impl App {
    /// Cursor state for the right-pane render: `(side, cursor line, V-select
    /// range)` while `Mode::Diff` is up, else `None`.
    pub fn diff_cursor(&self) -> Option<(DiffSide, usize, Option<Range<usize>>)> {
        (self.nav.mode == Mode::Diff).then(|| self.right.cursor_view())
    }

    /// Right-pane title suffix while `Mode::Diff` is up: `hunk 1/3` or
    /// `lines 41-42`/`line 41`, so it is obvious what `<space>` will hit.
    pub fn diff_granule_hint(&self) -> Option<String> {
        if self.nav.mode != Mode::Diff {
            return None;
        }
        self.right.granule_hint()
    }
}

impl App {
    /// The credential prompt as data, when it is up.
    pub fn askpass_popup(&self) -> Option<CommitPopupView<'_>> {
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
}

impl App {
    /// The welcome screen's highlighted choice.
    pub fn welcome_selected(&self) -> usize {
        self.full_screens.welcome_selected
    }

    /// The folder the welcome screen offers to `git init`.
    pub fn welcome_dir(&self) -> Option<&std::path::Path> {
        self.full_screens.welcome_dir.as_deref()
    }
}

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
}

impl App {
    /// The settings sheet's state, for the screen that draws it and for tests.
    pub fn settings(&self) -> &SettingsSheet {
        &self.sheets.settings
    }

    /// The live configuration: what ferrit is using now, saved or not.
    #[doc(hidden)]
    pub fn live_config(&self) -> &Config {
        &self.prefs.config
    }

    /// The row's value when it is a choice: the index among its names.
    #[must_use]
    pub fn choice_index(&self, row: SettingsRow) -> Option<usize> {
        settings::choice_index(&self.theme, row)
    }

    /// The row's value when it is a toggle.
    #[must_use]
    pub fn toggle_value(&self, row: SettingsRow) -> bool {
        settings::toggle_value(&self.prefs.config, row)
    }

    /// The row's value when it is a number.
    #[must_use]
    pub fn number_value(&self, row: SettingsRow) -> u64 {
        settings::number_value(&self.prefs.config, row)
    }
}

impl App {
    /// The git config screen's state, for the screen that draws it and for tests.
    pub fn git_config(&self) -> &crate::tui::components::git_config::GitConfigScreen {
        &self.full_screens.git_config
    }
}
