//! Keybar selection and rendering.

use crate::tui::components::keybar::{Bar, keybar_layout};
use crate::tui::components::panes::nav::Pane;
use crate::tui::draw::{FullScreen, Landed, RenderState};
use crate::tui::scene::Scene;
use crate::tui::widgets::chrome::KeyBar;
use ratatui::Frame;
use ratatui::layout::Rect;

/// Draw the bottom key-hint bar for the current screen and focus.
pub(crate) fn draw_keybar(
    frame: &mut Frame<'_>,
    area: Rect,
    app: &Scene<'_>,
    render: &RenderState,
    landed: &mut Landed,
) {
    let palette = app.palette();
    let bar = if app.help.is_visible(&render.help) {
        Bar::Help
    } else if app.dashboard_open_in(&render.sheet) {
        Bar::Dashboard
    } else if app.full_screen() == FullScreen::GitConfig {
        Bar::GitConfig
    } else if app.full_screen() == FullScreen::Welcome {
        Bar::Welcome
    } else if app.snapshot.operation.is_some() {
        Bar::Operation
    } else if app.right_focused() {
        Bar::RightPane
    } else if (app.nav.focus == Pane::Branches && app.branches_drilled())
        || (app.nav.focus == Pane::Commits && app.commits_drilled())
    {
        Bar::Drilled
    } else if app.nav.focus == Pane::Branches {
        Bar::Branches
    } else if app.nav.focus == Pane::Stash && app.row_count(Pane::Stash) == 0 {
        Bar::StashEmpty
    } else if app.nav.focus == Pane::Stash {
        Bar::Stash
    } else if app.nav.focus == Pane::Commits {
        Bar::Commits
    } else if app.row_count(Pane::Files) == 0 {
        Bar::FilesEmpty
    } else {
        Bar::Default
    };
    if let Some(message) = app.confirm_message() {
        KeyBar::confirm(message, &palette).render(frame, area);
        return;
    }
    let layout = keybar_layout(&app.prefs.keymap, bar, usize::from(area.width));
    KeyBar::hints(&layout.text, &palette).render(frame, area);
    landed.keybar = Some((area, layout.hits));
}
