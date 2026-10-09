//! The read-only questions the tests ask of `App`, answered through its `Scene`.
//! The screens ask the `Scene` directly.

use std::ops::Range;

use crate::config::Config;
use crate::config::settings::{SettingsRow, SettingsSheet};
use crate::git::create_remote::CreateRemote;
use crate::git::diff::DiffSide;
use crate::tui::App;
use crate::tui::components::create_remote::CreateRemoteView;
use crate::tui::components::diff::{CommandLogView, CommitPopupView, MenuView, PopupView};

impl App {
    pub fn diff_cursor(&self) -> Option<(DiffSide, usize, Option<Range<usize>>)> {
        self.scene().diff_cursor()
    }

    pub fn diff_granule_hint(&self) -> Option<String> {
        self.scene().diff_granule_hint()
    }

    pub fn askpass_popup(&self) -> Option<CommitPopupView<'_>> {
        self.scene().askpass_popup()
    }

    pub fn welcome_selected(&self) -> usize {
        self.scene().welcome_selected()
    }

    pub fn welcome_dir(&self) -> Option<&std::path::Path> {
        self.scene().welcome_dir()
    }

    pub fn popup_view(&self) -> Option<PopupView<'_>> {
        self.scene().popup_view()
    }

    pub fn confirm_message(&self) -> Option<&str> {
        self.scene().confirm_message()
    }

    pub fn new_branch_popup(&self) -> Option<CommitPopupView<'_>> {
        self.scene().new_branch_popup()
    }

    pub fn stash_popup(&self) -> Option<CommitPopupView<'_>> {
        self.scene().stash_popup()
    }

    pub fn name_popup(&self) -> Option<CommitPopupView<'_>> {
        self.scene().name_popup()
    }

    pub fn command_log_popup(&self) -> Option<CommandLogView> {
        self.scene().command_log_popup()
    }

    pub fn menu_popup(&self) -> Option<MenuView> {
        self.scene().menu_popup()
    }

    pub fn note_popup(&self) -> Option<&str> {
        self.scene().note_popup()
    }

    pub fn upstream_value(&self) -> Option<String> {
        self.scene().upstream_value()
    }

    pub fn upstream_popup(&self) -> Option<CommitPopupView<'_>> {
        self.scene().upstream_popup()
    }

    pub fn settings(&self) -> &SettingsSheet {
        self.scene().settings()
    }

    pub fn live_config(&self) -> &Config {
        self.scene().live_config()
    }

    pub fn choice_index(&self, row: SettingsRow) -> Option<usize> {
        self.scene().choice_index(row)
    }

    pub fn toggle_value(&self, row: SettingsRow) -> bool {
        self.scene().toggle_value(row)
    }

    pub fn number_value(&self, row: SettingsRow) -> u64 {
        self.scene().number_value(row)
    }

    pub fn git_config(&self) -> &crate::tui::components::git_config::GitConfigScreen {
        self.scene().git_config()
    }

    pub fn create_remote(&self) -> &CreateRemote {
        self.scene().create_remote()
    }

    pub fn create_remote_view(&self) -> Option<CreateRemoteView<'_>> {
        self.scene().create_remote_view()
    }

    /// Whether the dashboard is up: sliding in or in, not on its way out.
    #[must_use]
    pub fn dashboard_is_open(&self) -> bool {
        self.scene().dashboard_open_in(&self.render.sheet)
    }

    /// Whether a sheet is on screen or sliding.
    #[must_use]
    pub fn sheet_is_open(&self) -> bool {
        !self.render.sheet.is_closed()
    }
}
