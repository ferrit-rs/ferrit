//! What the tests, the examples and the screens ask of `App` and do to it from
//! outside the run loop: the read-only questions, and the seams that set a
//! piece of state or feed one event.

use crate::theme::palette::Palette;
use crate::tui::components;
use crate::tui::components::diff::views::CommitPopupView;
use crate::tui::components::diff::views::DiffView;
use crate::tui::components::panes::nav::Pane;
use crate::tui::components::panes::rows::PaneRows;
use crate::tui::draw::FullScreen;
use crate::tui::events::AppEvent;
use crate::tui::image::preview::Preview;
use std::path::Path;
use std::sync::mpsc;
use std::time::Duration;

use ratatui::layout::Rect;
use ratatui::text::Line;

use crate::git;
use crate::tui::App;
use ratatui::crossterm::event::{Event, KeyEvent, KeyEventKind, MouseEvent};

impl App {
    /// Current right-pane diff, for `ui::draw_right_pane`.
    pub fn diff_view(&self) -> &DiffView {
        &self.right.diff
    }

    /// Configured Git author name, shown in the Info panel header when set.
    /// The view that replaces the panes, `FullScreen::None` for the normal screen.
    pub fn full_screen(&self) -> FullScreen {
        self.full_screens.active
    }

    /// The dashboard's state, for the screen that draws it and for tests.
    pub fn dashboard(&self) -> &components::dashboard::Dashboard {
        &self.sheets.dashboard
    }

    pub fn git_user_name(&self) -> Option<&str> {
        self.authorship.git_user_name.as_deref()
    }

    /// First visible line of the right-pane diff.
    pub fn right_scroll(&self) -> usize {
        self.right.scroll
    }

    /// Set the right-pane scroll. Test and example helper.
    pub fn set_right_scroll(&mut self, line: usize) {
        self.right.set_scroll(line);
    }

    /// Inner height of the right-pane diff box, written by `ui::draw_right_pane`
    /// each frame so the scroll clamp and page steps track the real size.
    pub fn set_right_viewport(&mut self, rows: usize) {
        self.right.set_viewport(rows);
    }

    /// Whole right-pane rect, written by `ui::draw_right_pane` each frame so a
    /// mouse-wheel event can be routed by its column.
    pub fn set_right_area(&mut self, area: Rect) {
        self.right.area = area;
    }

    /// Store the configured Git author's clickable cells for mouse routing.
    pub fn set_author_click_area(&mut self, area: Rect) {
        self.hits.author = area;
    }

    /// Store the visible Dashboard trigger cells for mouse routing.
    pub fn set_dashboard_click_area(&mut self, area: Rect) {
        self.hits.dashboard = area;
    }

    /// Tell the app what the terminal can show (`ColorDepth::detect`).
    pub fn set_color_depth(&mut self, depth: crate::theme::scheme::ColorDepth) {
        self.prefs.color_depth = depth;
    }

    /// The palette every screen and line builder colours with.
    pub fn palette(&self) -> Palette {
        self.prefs.palette
    }

    /// Whether help is visible or still animating out.
    pub(crate) fn help_is_open(&self) -> bool {
        self.help.is_visible(&self.render.help)
    }

    /// A left pane's bordered rect, written by `ui::draw_left_column` each
    /// frame so a click can be routed to the pane it landed in.
    pub fn set_left_area(&mut self, pane: Pane, area: Rect) {
        self.hits.left[pane] = area;
    }

    /// Whether the right pane was last clicked, for `ui::draw_right_pane`'s
    /// border highlight.
    pub fn right_focused(&self) -> bool {
        self.nav.right_focused
    }

    /// A left pane's list scroll offset, read by `ui::draw_left_column`
    /// before it builds that pane's `ListState`.
    pub fn list_offset(&self, pane: Pane) -> usize {
        self.hits.list_offset(pane)
    }

    /// A left pane's list scroll offset, written by `ui::draw_left_column`
    /// after `render_stateful_widget` so a click in a scrolled list maps to
    /// the right row.
    pub fn set_list_offset(&mut self, pane: Pane, offset: usize) {
        self.hits.set_list_offset(pane, offset);
    }

    /// Whether a wheel scroll left `pane`'s view away from its selection. Read
    /// by `ui::draw_left_column`.
    pub fn view_detached(&self, pane: Pane) -> bool {
        self.hits.view_detached(pane, self.nav.selection[pane])
    }

    /// Feed one key to the handler. Integration-test seam; the running app
    /// calls `on_key` from `run`.
    #[doc(hidden)]
    pub fn feed_key(&mut self, key: KeyEvent) {
        self.on_key(key);
        // Test frames have no event loop to advance the help animation. The
        // real terminal loop keeps the animation; this seam makes one fed key
        // produce a stable frame like the other synchronous test inputs.
        if self.render.help.is_animating() {
            self.render.help.tick(Duration::from_secs(1));
        }
    }

    /// Has a quit key been pressed? Integration-test seam: `run()` reads the
    /// flag itself.
    #[doc(hidden)]
    pub fn is_quitting(&self) -> bool {
        self.should_quit
    }

    /// Point the repository's `git config` calls at a throwaway global file
    /// (`Repo::isolate_config`). Integration-test seam: lets a test write the
    /// global scope without touching the real `~/.gitconfig`.
    #[doc(hidden)]
    pub fn isolate_git_config(&mut self, global: &Path) {
        if let Some(repo) = &mut self.repo {
            repo.isolate_config(global);
        }
    }

    /// Feed one mouse event to the handler. Integration-test seam.
    #[doc(hidden)]
    pub fn feed_mouse(&mut self, ev: MouseEvent) {
        self.on_mouse(ev);
    }

    /// Give the app a way back onto a background remote op's completion
    /// channel, the same one `run()` gets from its own `Events`
    /// (`Events::sender`). Integration-test seam: lets a test drive
    /// `f`/`p`/`P` through `feed_key` and still observe the eventual
    /// `AppEvent::RemoteDone`, with no `run()` loop (and its real
    /// terminal) involved.
    #[doc(hidden)]
    pub fn set_event_sender(&mut self, sender: mpsc::Sender<AppEvent>) {
        self.workers.sender = Some(sender);
    }

    /// Deliver one event the way `run()` would, without a terminal. The replay
    /// harness (`crate::replay`) uses it to complete background work
    /// deterministically. Key presses go to `on_key`; other input is ignored.
    #[doc(hidden)]
    pub fn deliver_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::Input(Event::Key(key)) if key.kind == KeyEventKind::Press => {
                self.on_key(key);
            },
            other => self.on_background_event(other),
        }
    }

    /// No refresh, diff, image or remote operation is running or queued. With
    /// an event sender set, work finishes on a thread; a caller that delivers
    /// events until this is `true` sees a settled screen with no sleeping.
    #[doc(hidden)]
    pub fn is_idle(&self) -> bool {
        !self.workers.refresh.in_flight
            && !self.workers.diff.in_flight
            && !self.workers.image.in_flight
            && self.workers.remote_busy.is_none()
            && !self.sheets.dashboard.is_busy()
    }

    /// Back to synchronous work (undo `set_event_sender`).
    #[doc(hidden)]
    pub fn clear_event_sender(&mut self) {
        self.workers.sender = None;
    }

    /// Advance the toast's animation and its timeout, and the settings sheet's
    /// slide, by `elapsed`, as the run loop does. Integration-test seam: a test has no loop to wait on.
    #[doc(hidden)]
    pub fn advance_clock(&mut self, elapsed: Duration) {
        self.render.tick(elapsed);
    }

    /// Let the settings sheet's slide end now. Integration-test seam for the
    /// replay, which has no clock to wait on.
    #[doc(hidden)]
    pub fn finish_animations(&mut self) {
        self.render.sheet.tick(Duration::from_secs(1));
        self.render.help.tick(Duration::from_secs(1));
    }

    /// The right-pane preview for the current selection.
    pub fn preview(&self) -> &Preview {
        &self.render.preview
    }

    /// Is the right pane currently a native-graphics image? `run` watches this
    /// across frames: when it flips back to `false` the sixel / iTerm2 / kitty
    /// pixels of the old frame outlive a normal buffer diff and need a full
    /// `terminal.clear()`.
    pub(crate) fn preview_is_image(&self) -> bool {
        matches!(self.render.preview, Preview::Image(_))
    }

    /// Focus `pane` and move its cursor to `index`, rebuilding the preview.
    /// Test and example helper; the running app goes through `on_key`.
    pub fn select(&mut self, pane: Pane, index: usize) {
        self.nav.focus = pane;
        let last = self.row_count(pane).saturating_sub(1);
        self.nav.selection[pane] = index.min(last);
        self.update_right_pane();
    }

    /// Selectable row count for a pane, for clamping the cursor and deciding
    /// whether to draw a highlight.
    pub fn row_count(&self, pane: Pane) -> usize {
        self.rows().row_count(pane)
    }

    /// `(current, total)` for the pane's `N of M` border counter, or `None`
    /// when the pane has no selectable rows.
    pub fn counter(&self, pane: Pane) -> Option<(usize, usize)> {
        self.rows().counter(pane)
    }

    /// Selection cursor for a given pane.
    pub fn selected(&self, pane: Pane) -> usize {
        self.nav.selected(pane)
    }

    pub fn file_lines(&self) -> Vec<Line<'static>> {
        self.rows().file_lines()
    }

    pub fn file_display(&self, i: usize) -> String {
        self.rows().file_display(i)
    }

    pub fn files_selection_is_dir(&self) -> bool {
        self.rows().files_selection_is_dir()
    }

    pub fn branch_lines(&self) -> Vec<Line<'static>> {
        self.rows()
            .branch_lines(self.workers.remote_branch_status().as_deref())
    }

    pub fn branches_title(&self) -> String {
        self.rows().branches_title()
    }

    pub fn branches_drilled(&self) -> bool {
        self.nav.branches_drilled()
    }

    pub fn commit_lines(&self) -> Vec<Line<'static>> {
        self.rows().commit_lines()
    }

    pub fn commits_title(&self) -> String {
        self.rows().commits_title()
    }

    pub fn commits_drilled(&self) -> bool {
        self.nav.commits_drilled()
    }

    pub fn stash_lines(&self) -> Vec<Line<'static>> {
        self.rows().stash_lines()
    }

    /// Status pane: lazygit's one-liner `ferrit -> main ↑2`, plus a conflict
    /// line only when there are conflicts, or the error when `refresh()` failed.
    pub fn status_lines(&self) -> Vec<Line<'static>> {
        self.scene().status_lines()
    }

    /// The commit popup as data, without any animation state.
    pub fn commit_popup(&self) -> Option<CommitPopupView<'_>> {
        self.scene().commit_popup()
    }

    /// Replace the identities git knows globally. Integration-test seam: the
    /// real ones come from the machine's own git config.
    #[doc(hidden)]
    pub fn set_global_identities(&mut self, identities: Vec<(String, String)>) {
        self.authorship.profile.settings.global_identities = identities
            .into_iter()
            .map(|(name, email)| git::identity::Identity {
                name,
                email: Some(email),
            })
            .collect();
    }

    /// `[ui] mouse`: should the terminal capture the mouse?
    pub fn mouse_enabled(&self) -> bool {
        self.prefs.mouse_enabled()
    }

    /// Where a settings save writes, if anywhere.
    pub fn config_file(&self) -> Option<&Path> {
        self.prefs.file.as_deref()
    }

    /// Repo-free (`App::mock()`): the right pane's mock sample text applies.
    pub fn is_mock(&self) -> bool {
        self.repo.is_none()
    }

    pub(crate) fn rows(&self) -> PaneRows<'_> {
        PaneRows {
            nav: &self.nav,
            snapshot: &self.snapshot,
            palette: &self.prefs.palette,
        }
    }
}
