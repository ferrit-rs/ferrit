//! Application state and the draw / event loop.
//!
//! Phase 2 wired every left pane (Status, Files, Branches, Commits, Stash) to
//! a real read-only `git::Repo`. `App` owns the repo handle, the cached
//! snapshot, which left pane is focused, and one selection cursor per pane.
//! `App::mock()` is the repo-free path the render tests use.

use crate::config::settings::SettingsRow;
use crate::git::commit::CommitKind;
use crate::tui::components::askpass;
use crate::tui::components::commit_editor;
use crate::tui::components::diff::CommitPopupView;
use crate::tui::components::git_config::GitConfig;
use crate::tui::components::keybar::HelpLine;
use crate::tui::components::keybar::help_lines;
use crate::tui::components::popups::Popup;
use crate::tui::components::settings::Settings;
use crate::tui::widgets::tui_overlay::state::OverlayState;
pub mod events;
pub mod mock;

use std::path::{Path, PathBuf};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use crate::theme::palette::Palette;
use crate::tui::widgets::mouse_pointer::MousePointer;
use crate::tui::widgets::toast::Toast;
use color_eyre::Result;
use ratatui::crossterm::event::{Event, KeyEvent, KeyEventKind, MouseEvent};
use ratatui::layout::Rect;
use ratatui::text::Line;

use crate::git;
use crate::git::error::GitResult;
use crate::git::image::detect;
use crate::git::image::preview::Preview;
use crate::git::port::GitPort;
use crate::tui::draw as ui;
use crate::tui::events::{AppEvent, Events};
use terminal::Tui;

/// `Operation::noun` as a function pointer for `Option::map_or`.
pub(crate) fn operation_noun(operation: git::model::Operation) -> &'static str {
    operation.noun()
}

/// How often the run loop wakes while an error toast is up, to count its timeout.
const TOAST_TICK_MS: u64 = 250;

pub struct App {
    /// The configuration and what it makes: keymap, palette, colour depth.
    pub(crate) prefs: prefs::Prefs,
    /// The side drawer and the sheets it holds: settings, dashboard.
    pub(crate) sheets: components::dashboard::Sheets,
    /// The views that replace the panes: git config, welcome.
    pub(crate) full_screens: draw::FullScreens,
    /// Where the user is: focus, selection, drill-downs, tabs.
    pub nav: components::panes::Nav,
    /// Whether the help overlay is up.
    pub help: components::help::HelpState,
    /// First visible line of the help screen, and how many lines it shows
    /// (set by the renderer), so scroll keys can stop at the end.
    /// Text and focus state for the help command search.
    pub(crate) should_quit: bool,

    /// `None` in `App::mock()`; otherwise the open repository.
    pub(crate) repo: Option<Box<dyn GitPort>>,
    /// Repository directory name, shown in the status header (`ferrit -> main`).
    pub(crate) repo_name: String,
    /// Who commits are by: the identities git knows and ferrit's pick.
    pub(crate) authorship: git::authorship::Authorship,
    pub theme: components::settings::ThemeEditor,
    /// What the last refresh read: header, files, branches, remotes, commits,
    /// stashes and any operation stopped mid-way.
    pub(crate) snapshot: git::Snapshot,
    /// Last `refresh()` failure, shown in the Status pane. Never a panic.
    pub(crate) last_error: Option<Arc<AppError>>,
    /// Optional worktree watcher failure; polling remains active as fallback.
    pub(crate) watch_error: Option<Arc<AppError>>,

    /// The right column: image preview, diff, scroll and line cursor.
    pub(crate) right: components::diff::RightPane,
    /// Where the last frame put the clickable things.
    pub(crate) hits: components::panes::HitAreas,
    /// Whether the mouse is currently over that clickable author name.
    pub(crate) mouse_pointer: MousePointer,
    /// What ratatui needs mutable to show the app: animations and the toast.
    pub(crate) render: draw::RenderState,
    /// The new-branch prompt's title, naming the branch it starts from (lazygit).
    pub(crate) new_branch_title: String,
    /// What owns the keys on top of the panes: a popup (commit box, menu,
    /// note; `docs/PLAN_7_COMMIT.md`) or a key-bar question waiting on
    /// `y` / `n` / `Esc`. One at a time, hence one value.
    pub(crate) modal: components::popups::Modal,
    /// The last commit popup's text, kept across an `Esc`-cancel so a
    /// mistyped keystroke never loses a paragraph. Cleared on a successful
    /// commit.
    pub(crate) commit_draft: Option<String>,
    /// Background work in flight: the event channel, the refresh, diff and
    /// image workers, and the one network operation at a time.
    pub(crate) workers: workers::Workers,
    /// Set when the app was rebuilt on a new repository: `run` points the
    /// filesystem watch at this root and clears it.
    pub(crate) watch_request: Option<PathBuf>,
    /// A change the run loop has to carry out in the terminal, once.
    pub(crate) terminal_request: Option<crate::config::settings::TerminalRequest>,
    pub(crate) create_remote: git::create_remote::CreateRemote,
    /// A background fetch/pull/push's success line ("Fetched origin", "3
    /// commits pushed"), shown in the Status pane until the next remote op
    /// or the next `refresh()`. `last_error`'s sibling for the non-error
    /// case, not a repurposing of that one field with a colour flag.
    pub(crate) status_note: Option<String>,
}

pub mod workers;

pub mod components;
pub mod draw;
pub mod error;
pub mod event;
pub mod input;
pub mod keymap;
pub mod prefs;
pub mod publish;
pub mod row_lines;
pub mod terminal;
pub mod view;
pub mod widgets;

pub(crate) use error::AppError;

#[cfg(test)]
mod tests;

use crate::tui::workers::RefreshCompletion;
use crate::tui::workers::{WorkerKind, run_worker};
use components::diff::DiffView;
use components::diff::Mode;
use components::panes::Pane;
use components::panes::PaneRows;
use components::panes::{FileRow, drill_tree_rows};
use draw::FullScreen;

use crate::tui::draw::Landed;

impl App {
    /// Take in what a frame learned (`docs/PLAN_24_DRAW_VIEW.md`): each part
    /// goes to the component it is about.
    pub(crate) fn land(&mut self, mut landed: Landed) {
        self.hits.land(&mut landed, &self.nav.selection);
        if let Some(area) = landed.right_area {
            self.right.area = area;
        }
        if let Some(rows) = landed.right_viewport {
            self.set_right_viewport(rows);
        }
        if let Some((scroll, followed)) = landed.settings_scroll {
            self.sheets.settings.scroll = scroll;
            if followed {
                self.sheets.settings.follow = false;
            }
        }
        if let Some(offset) = landed.git_config_offset {
            self.set_git_config_offset(offset);
        }
        if let Some(rows) = landed.help_rows {
            self.help.set_rows(rows);
        }
        if let Some(max) = landed.dashboard_max_scroll {
            self.sheets.dashboard.clamp_scroll(max);
        }
    }

    fn base(repo: Option<Box<dyn GitPort>>, config: crate::config::Config) -> Self {
        let theme_config = config.theme.clone();
        let palette = theme_config.palette();
        let keymap = keymap::Keymap::from_overrides(&config.keys).0;
        let repo_name = repo
            .as_ref()
            .map_or_else(|| "ferrit".to_owned(), |repo| repo.name());
        let authorship = git::authorship::Authorship::of(repo.as_deref());
        Self {
            prefs: prefs::Prefs::new(config, keymap, palette),
            sheets: components::dashboard::Sheets::default(),
            full_screens: draw::FullScreens::default(),
            nav: components::panes::Nav::default(),
            help: components::help::HelpState::default(),
            should_quit: false,
            repo,
            repo_name,
            authorship,
            theme: components::settings::ThemeEditor::new(theme_config),
            right: components::diff::RightPane::new(),
            snapshot: git::Snapshot::default(),
            last_error: None,
            watch_error: None,
            hits: components::panes::HitAreas::default(),
            mouse_pointer: MousePointer::default(),
            render: draw::RenderState::default(),
            new_branch_title: String::new(),
            modal: components::popups::Modal::default(),
            commit_draft: None,
            workers: workers::Workers::new(),
            watch_request: None,
            terminal_request: None,
            create_remote: git::create_remote::CreateRemote::default(),
            status_note: None,
        }
    }

    /// Open the repo at or above `path` with the default configuration, then
    /// take one snapshot. Reads no config file and writes none: the seam tests
    /// and examples use. The binary uses `open_with` and `Config::load`.
    pub fn open(path: &Path) -> GitResult<Self> {
        Self::open_with(path, crate::config::ConfigLoad::default())
    }

    /// The app on any git port, with the default configuration, after one
    /// snapshot. `open` is this with the real adapter; tests pass a
    /// `git::fake::FakeGit`.
    pub fn with_git(git: Box<dyn GitPort>) -> Self {
        let mut app = Self::base(Some(git), crate::config::Config::default());
        app.refresh();
        app
    }

    /// Open the repo at or above `path` with a loaded configuration. Whatever
    /// was wrong with the file is reported once, as an error toast.
    pub fn open_with(path: &Path, load: crate::config::ConfigLoad) -> GitResult<Self> {
        let crate::config::ConfigLoad {
            config,
            file,
            issues,
        } = load;
        let mut app = Self::base(Some(Box::new(git::repo::Repo::open(path)?)), config);
        app.prefs.file = file;
        app.refresh();
        app.report_config_issues(&issues);
        Ok(app)
    }

    /// The app `ferrit` starts with: the repository at or above `path`, or, when
    /// there is none and `explicit` is false (no `--path` was given), the
    /// welcome screen that offers `git init`. A path named on purpose keeps the
    /// error: scripts rely on it, and a typo must not offer to create a
    /// repository somewhere else. Any other failure is an error either way.
    pub fn open_or_welcome(
        path: &Path,
        explicit: bool,
        load: crate::config::ConfigLoad,
    ) -> GitResult<Self> {
        match Self::open_with(path, load.clone()) {
            Err(git::error::GitError::NotARepository(_)) if !explicit => {
                Ok(Self::welcome(path, load))
            },
            other => other,
        }
    }

    /// Whatever was wrong with the configuration file, reported once, as an
    /// error toast.
    fn report_config_issues(&mut self, issues: &[String]) {
        // What the file says about the keys is the keymap's to judge.
        let mut issues = issues.to_vec();
        issues.extend(keymap::Keymap::from_overrides(&self.prefs.config.keys).1);
        if issues.is_empty() {
            return;
        }
        let location = self
            .prefs
            .file
            .as_deref()
            .map_or_else(String::new, |f| format!(" {}", f.display()));
        self.report_error(AppError::ConfigIssues { location, issues });
    }

    /// The app for a folder with no repository in it or above it: the welcome
    /// screen, offering `git init`. No panes are drawn, so none of their data
    /// is needed. `dir` is made absolute for the question.
    pub fn welcome(dir: &Path, load: crate::config::ConfigLoad) -> Self {
        let crate::config::ConfigLoad {
            config,
            file,
            issues,
        } = load;
        let mut app = Self::base(None, config);
        app.prefs.file = file;
        app.full_screens.active = FullScreen::Welcome;
        app.full_screens.welcome_dir =
            Some(dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf()));
        app.report_config_issues(&issues);
        app
    }

    /// The help screen's content for the focused pane, from the live keymap.
    pub(crate) fn help_lines(&self) -> Vec<HelpLine> {
        help_lines(&self.prefs.keymap, &self.key_contexts())
    }

    /// `[ui] mouse`: should the terminal capture the mouse?
    pub fn mouse_enabled(&self) -> bool {
        self.prefs.mouse_enabled()
    }

    /// Where a settings save writes, if anywhere.
    pub fn config_file(&self) -> Option<&Path> {
        self.prefs.file.as_deref()
    }

    /// Repo-free instance backed by `mock` data, for the render tests.
    pub fn mock() -> Self {
        let mut app = Self::base(None, crate::config::Config::default());
        app.snapshot.header = mock::mock_header();
        app.snapshot.files = mock::mock_files();
        app.snapshot.branches = mock::mock_branches();
        app.snapshot.remotes = mock::mock_remotes();
        app.snapshot.commits = mock::mock_commits();
        app.snapshot.stashes = mock::mock_stashes();
        app.update_right_pane();
        app
    }

    /// Repo-free (`App::mock()`): the right pane's mock sample text applies.
    pub fn is_mock(&self) -> bool {
        self.repo.is_none()
    }

    /// Query the real terminal for a graphics protocol and, if it has one,
    /// swap it in for the half-block fallback. Call once, before `run`. See
    /// `image::detect` for hosts that lie about support.
    pub fn detect_graphics(&mut self) {
        if let Some(found) = detect::pick() {
            if let Some(line) = found.debug_line() {
                self.report_notice(line);
            }
            self.right.picker = found.picker;
            self.invalidate_image_query();
            self.update_right_pane();
        }
    }

    /// Open the repository at `path` and become an app on it: the identity, the
    /// profile and the first snapshot are all computed from the repository, so
    /// the app is rebuilt rather than patched. What only `main` and `run` had
    /// set (the graphics probe, the event sender) is carried over, the loaded
    /// configuration is kept, and `run` is asked to watch the new worktree.
    /// Used after a `git init` from the welcome screen
    /// (`docs/PLAN_16_START_WITHOUT_REPO.md`). On error nothing changes.
    pub fn attach_repository(&mut self, path: &Path) -> GitResult<()> {
        let load = crate::config::ConfigLoad {
            config: self.prefs.config.clone(),
            file: self.prefs.file.clone(),
            issues: Vec::new(),
        };
        let mut fresh = Self::open_with(path, load)?;
        fresh.workers.sender = self.workers.sender.take();
        fresh.right.picker = self.right.picker.clone();
        fresh.create_remote.carry_program_from(&self.create_remote);
        fresh.watch_request = fresh.watch_root();
        *self = fresh;
        Ok(())
    }

    /// The worktree `run` should watch, once, after `attach_repository`.
    pub(crate) fn take_watch_request(&mut self) -> Option<PathBuf> {
        self.watch_request.take()
    }

    /// Re-read the wired panes. On error keep the old snapshot and stash the
    /// message; never propagate, never panic. No-op without a repo.
    pub fn refresh(&mut self) {
        let (branch, commit) = self.nav.drill_targets();
        let opts = self.prefs.diff_opts();
        let Some(repo) = &mut self.repo else { return };
        let completion = RefreshCompletion::load(repo.as_mut(), branch, commit, opts);
        self.apply_refresh_result(completion);
    }

    /// Request refresh without blocking the TUI. Outside `run()` (tests and
    /// startup helpers), retain synchronous behavior.
    pub(super) fn request_refresh(&mut self) {
        let Some(sender) = self.workers.sender.clone() else {
            self.refresh();
            return;
        };
        if self.workers.refresh.in_flight {
            self.workers.refresh.pending = true;
            return;
        }
        let Some(handle) = self.reopen_repo() else {
            return;
        };
        let (branch, commit) = self.nav.drill_targets();
        let opts = self.prefs.diff_opts();
        self.workers.refresh.in_flight = true;
        thread::spawn(move || {
            let completion = run_worker(WorkerKind::Refresh, || match handle {
                Ok(mut repo) => RefreshCompletion::load(repo.as_mut(), branch, commit, opts),
                Err(error) => RefreshCompletion::failed(error, branch, commit),
            })
            .unwrap_or_else(|error| RefreshCompletion {
                snapshot: Err(Arc::new(error.into())),
                profile: None,
                branch_log: None,
                commit_files: None,
            });
            let _ = sender.send(AppEvent::RefreshDone(Box::new(completion)));
        });
    }

    fn apply_refresh_result(&mut self, completion: RefreshCompletion) {
        if let Some(profile) = completion.profile {
            self.authorship.profile = profile;
        }
        let old_selection = self.nav.remember(&self.snapshot, &self.prefs.palette);
        match completion.snapshot {
            Ok(snap) => {
                self.snapshot = snap;
                self.last_error = None;
                self.workers.refresh_failure = None;
            },
            // A failure that keeps repeating (every poll, every file change)
            // opens one toast, not a new one each time it is seen again.
            Err(error)
                if self.workers.refresh_failure.as_deref() == Some(error.to_string().as_str()) =>
            {
                self.last_error = Some(Arc::new(AppError::Refresh(error)));
            },
            Err(error) => {
                self.workers.refresh_failure = Some(error.to_string());
                self.report_error(AppError::Refresh(error));
            },
        }

        if let Some((branch, result)) = completion.branch_log {
            self.nav.refresh_branch_log(&branch, result);
        }
        if let Some((hash, result)) = completion.commit_files {
            self.nav.refresh_commit_files(&hash, result);
        }
        self.nav
            .restore(&self.snapshot, &self.prefs.palette, old_selection);
        self.workers.diff.refresh_requested = true;
        self.invalidate_image_query();
        self.update_right_pane();
    }

    fn on_refresh_done(&mut self, completion: RefreshCompletion) {
        self.workers.refresh.in_flight = false;
        let rerun = std::mem::take(&mut self.workers.refresh.pending);
        self.apply_refresh_result(completion);
        if rerun {
            self.request_refresh();
        } else if let Some(error) = self.workers.remote_refresh_error.take() {
            // The refresh just replaced the Status line: put the failed
            // operation's own message back. Its toast was shown when it failed;
            // it is not shown again, and it is not a refresh failure.
            self.last_error = Some(Arc::new(AppError::Background(error)));
        }
        if self.last_error.is_none() {
            self.last_error = self.watch_error.clone();
        }
    }

    /// Rebuild both cached right-pane values (`preview`, then `diff`) for the
    /// current focus and selection. Cheap when nothing changed.
    pub(crate) fn update_right_pane(&mut self) {
        self.update_preview();
        self.update_diff();
        self.sync_commit_file_scroll();
    }

    /// While drilled into a commit's file tree, jump the right pane's scroll
    /// to the selected file's own section (`Diff::file_lines()`), lazygit's
    /// "the file list drives the main view" behaviour. A no-op off the
    /// Commits pane, undrilled, or on a directory row.
    fn sync_commit_file_scroll(&mut self) {
        if self.nav.focus != Pane::Commits {
            return;
        }
        let Some(drill) = &self.nav.commit_drill else {
            return;
        };
        let rows = drill_tree_rows(&drill.files, &drill.collapsed);
        let Some(FileRow::File { index, .. }) = rows.get(self.selected(Pane::Commits)) else {
            return;
        };
        let DiffView::Commit(_, diff) = &self.right.diff else {
            return;
        };
        if let Some(&line) = diff.file_lines().get(*index) {
            self.right.scroll = line;
            self.right.clamp_scroll();
        }
    }

    /// Called after *every* key while `Mode::Diff` is up: keeps the cursor
    /// where it was, or drops to `Mode::Nav` once the right pane has no line
    /// left to put it on (`RightPane::resync_cursor`).
    pub(crate) fn resync_diff_cursor(&mut self) {
        if self.nav.mode == Mode::Diff && !self.right.resync_cursor() {
            self.nav.mode = Mode::Nav;
        }
    }

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

    /// The events a background worker or the watcher sends, as opposed to
    /// terminal input. `run()` and `deliver_event` share this.
    fn on_background_event(&mut self, event: AppEvent) {
        match event {
            AppEvent::Refresh => self.request_refresh(),
            AppEvent::RefreshDone(completion) => self.on_refresh_done(*completion),
            AppEvent::DiffDone(completion) => self.on_diff_done(completion),
            AppEvent::ImageDone(completion) => self.on_image_done(completion),
            AppEvent::RemoteDone { op, message } => self.on_remote_done(op, message),
            AppEvent::RemoteCreated(result) => self.on_remote_created(result),
            AppEvent::GhChecked { generation, status } => self.on_gh_checked(generation, status),
            AppEvent::Askpass { prompt, reply } => {
                let events = askpass::ask(prompt, reply, self.modal.popup().is_some());
                self.apply(events);
            },
            AppEvent::StatsDone(completion) => self.on_stats_done(completion),
            AppEvent::Input(_) => {},
        }
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

    /// Is the right pane currently a native-graphics image? `run` watches this
    /// across frames: when it flips back to `false` the sixel / iTerm2 / kitty
    /// pixels of the old frame outlive a normal buffer diff and need a full
    /// `terminal.clear()`.
    fn preview_is_image(&self) -> bool {
        matches!(self.render.preview, Preview::Image(_))
    }

    /// The right-pane preview for the current selection.
    pub fn preview(&self) -> &Preview {
        &self.render.preview
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

    pub(crate) fn rows(&self) -> PaneRows<'_> {
        PaneRows {
            nav: &self.nav,
            snapshot: &self.snapshot,
            palette: &self.prefs.palette,
        }
    }

    /// Status pane: lazygit's one-liner `ferrit -> main ↑2`, plus a conflict
    /// line only when there are conflicts, or the error when `refresh()` failed.
    pub fn status_lines(&self) -> Vec<Line<'static>> {
        let mut out = if let Some(err) = &self.last_error {
            vec![row_lines::error_line(
                &self.prefs.palette,
                &format!("error: {err}"),
            )]
        } else {
            let h = &self.snapshot.header;
            let line = row_lines::status_header(&self.repo_name, h);
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
        if let Some(label) = self.remote_busy_label() {
            out.push(row_lines::busy_line(&self.prefs.palette, label));
        } else if self.last_error.is_none()
            && let Some(note) = &self.status_note
        {
            out.push(row_lines::status_line(&self.prefs.palette, note));
        }
        out
    }

    /// Keep persistent Status text while moving typed error into transient toast.
    pub(super) fn report_error(&mut self, error: impl Into<AppError>) {
        let error = Arc::new(error.into());
        self.last_error = Some(Arc::clone(&error));
        self.render.toast = Some(Toast::error(error));
    }

    pub(super) fn report_notice(&mut self, message: impl Into<String>) {
        self.last_error = Some(Arc::new(AppError::Notice(message.into())));
    }

    /// Worktree root to hand the filesystem watcher, or `None` for a bare
    /// repo (and for `App::mock`, which has no repo).
    fn watch_root(&self) -> Option<PathBuf> {
        self.repo
            .as_ref()
            .and_then(|repo| repo.workdir())
            .map(Path::to_path_buf)
    }

    /// A fresh handle onto the same repository, for a background thread
    /// that cannot borrow `self.repo` across `thread::spawn`'s `'static`
    /// bound. `git::Repo::open` is cheap (`git2::Repository::discover`, no
    /// I/O beyond opening `.git`), so reopening the same path is simpler
    /// than sharing state — the same call `App::open` already makes once
    /// at startup. `None` for `App::mock()` and for a bare repo (no
    /// worktree root to reopen from), same reach as `watch_root` already
    /// has.
    pub(crate) fn repo_handle(&self) -> Option<Box<dyn GitPort>> {
        self.repo.as_ref()?.reopen().ok()
    }

    /// A second handle on the repository for a worker thread, or why it could
    /// not be opened; `None` without a repository.
    pub(crate) fn reopen_repo(&self) -> Option<GitResult<Box<dyn GitPort>>> {
        self.repo.as_ref().map(|repo| repo.reopen())
    }

    /// Draw, then block for the next event batch, until `should_quit`. Events
    /// come from terminal input, a recursive worktree watch, and a poll (`[ui]
    /// poll_secs`, 10s by default).
    /// Bounded batches avoid repainting for every auto-repeat key while still
    /// guaranteeing regular redraws during sustained input.
    pub fn run(&mut self, terminal: &mut Tui) -> Result<()> {
        let mut events = Events::new(self.watch_root().as_deref(), self.prefs.poll_interval())?;
        self.watch_error = watcher_error(&events);
        self.last_error = self.watch_error.clone();
        // A background fetch/pull/push (`start_remote_op`) needs its own
        // way back onto this channel; only `run()` has an `Events` to ask
        // for one, so it hands `on_key` this clone rather than `on_key`
        // taking `&Events` directly (it is also called from `feed_key`,
        // which has none).
        self.workers.sender = Some(events.sender());
        // Answers ssh/git credential prompts in a popup (`app::askpass`);
        // without it a passphrase question would hang on the raw terminal.
        let askpass_sender = events.sender();
        let _askpass = git::askpass::serve(move |prompt| {
            let (reply, answer) = mpsc::channel();
            askpass_sender
                .send(AppEvent::Askpass { prompt, reply })
                .ok()?;
            answer.recv().ok().flatten()
        });
        let mut prev_was_image = false;
        let mut overlay_tick = Instant::now();
        while !self.should_quit {
            let is_image = self.preview_is_image();
            if prev_was_image && !is_image {
                // Graphics pixels from the last image frame sit outside the
                // cell buffer; a full clear is the only way to wipe them.
                terminal.clear()?;
            }
            prev_was_image = is_image;
            terminal.draw(|frame| ui::draw_painted(frame, self))?;

            let animating = self.render.animating();
            // Frames while something animates; a slower tick while a toast is up,
            // so it can time out without waiting for a key.
            let timeout = (animating.any() || self.workers.remote_busy.is_some())
                .then_some(Duration::from_millis(16))
                .or_else(|| {
                    self.render
                        .toast
                        .is_some()
                        .then_some(Duration::from_millis(TOAST_TICK_MS))
                });
            let batch = if let Some(timeout) = timeout {
                match events.next_batch_timeout(timeout) {
                    Err(error) => return Err(error),
                    Ok(Some(batch)) => batch,
                    Ok(None) => {
                        self.render.tick(overlay_tick.elapsed());
                        overlay_tick = Instant::now();
                        continue;
                    },
                }
            } else {
                events.next_batch()?
            };

            for event in batch {
                match event {
                    AppEvent::Input(Event::Key(key)) if key.kind == KeyEventKind::Press => {
                        self.on_key(key);
                    },
                    AppEvent::Input(Event::Mouse(m)) => {
                        if self.render.toast_mouse(m) {
                            self.mouse_pointer.request(false);
                        } else {
                            self.on_mouse(m);
                        }
                        self.mouse_pointer.sync()?;
                    },
                    AppEvent::Input(_) => {},
                    background => self.on_background_event(background),
                }
                if self.should_quit {
                    break;
                }
            }
            // A setting changed that only this loop can carry out.
            if let Some(crate::config::settings::TerminalRequest::Mouse(on)) =
                self.terminal_request.take()
                && let Err(error) = terminal::set_mouse(on)
            {
                self.report_notice(format!("cannot switch the mouse: {error}"));
            }
            // The app was rebuilt on a new repository: watch its worktree.
            if let Some(root) = self.take_watch_request() {
                events.watch(&root);
                self.watch_error = watcher_error(&events);
                self.last_error = self.watch_error.clone();
            }
            self.render
                .tick_after_batch(overlay_tick.elapsed(), animating);
            overlay_tick = Instant::now();
        }
        self.workers.stop_remote();
        self.sheets.dashboard.stop_and_join();
        Ok(())
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

    /// Close the error toast now (`Esc`), unless something else owns the key:
    /// a popup, a question or the help. `true` when there was one to close.
    pub(super) fn dismiss_toast(&mut self) -> bool {
        if self.modal.is_some() || self.help_is_open() {
            return false;
        }
        self.render.dismiss_toast()
    }
}

/// The Status line for a filesystem watcher that could not start, if any.
fn watcher_error(events: &Events) -> Option<Arc<AppError>> {
    events.watch_error().map(|detail| {
        Arc::new(AppError::WatcherUnavailable {
            detail: detail.to_owned(),
        })
    })
}

impl App {
    /// The commit popup as data, without any animation state.
    pub fn commit_popup(&self) -> Option<CommitPopupView<'_>> {
        self.commit_popup_with(None)
    }

    /// The commit popup for drawing: `overlay` is its animation.
    pub(crate) fn commit_popup_with<'a>(
        &'a self,
        overlay: Option<&'a mut OverlayState>,
    ) -> Option<CommitPopupView<'a>> {
        let Some(Popup::Commit(draft)) = self.modal.popup() else {
            return None;
        };
        Some(draft.view(self.authorship.line(), overlay))
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

    /// `c` / `A` / `w`.
    pub(crate) fn open_commit(&mut self, kind: CommitKind) {
        if self.modal.popup().is_some() {
            return;
        }
        let events = commit_editor::open(kind, &mut self.open_ctx());
        self.apply(events);
    }

    /// Open the editor to reword the older commit `hash` with a rebase.
    pub(crate) fn open_reword_editor(&mut self, hash: String, title: String, message: &str) {
        let events =
            commit_editor::open_reword(hash, title, message, self.prefs.config.commit.sign_off);
        self.apply(events);
    }

    /// The "commit all" question shown when the index is empty.
    pub(crate) fn commit_all_confirm_key(&mut self, key: KeyEvent) {
        let events = commit_editor::all_confirm_key(key, &mut self.open_ctx());
        self.apply(events);
    }

    /// Keys while the commit editor owns input.
    pub(crate) fn commit_popup_key(&mut self, key: KeyEvent) {
        let Some(Popup::Commit(draft)) = self.modal.popup_mut() else {
            return;
        };
        let ctx = commit_editor::SubmitCtx {
            repo: self.repo.as_deref(),
            commits: &self.snapshot.commits,
            author: self.authorship.author_arg(),
            drilled: self.nav.commit_drill.is_some(),
        };
        let events = commit_editor::popup_key(draft, key, &ctx);
        self.apply(events);
    }

    fn open_ctx(&mut self) -> commit_editor::OpenCtx<'_> {
        commit_editor::OpenCtx {
            repo: self.repo.as_deref(),
            files: &self.snapshot.files,
            commits: &self.snapshot.commits,
            sign_off: self.prefs.config.commit.sign_off,
            saved: &mut self.commit_draft,
        }
    }
}

impl App {
    /// `git init` in `dir`, then open the repository it made.
    pub(crate) fn init_here(&mut self, dir: &Path) {
        let made = git::repo::Repo::init(dir).map(|_| dir.to_path_buf());
        match made.and_then(|dir| self.attach_repository(&dir)) {
            Ok(()) => {},
            Err(error) => self.report_error(error),
        }
    }
}

impl App {
    /// The settings sheet, with the parts of the app it changes.
    pub(crate) fn settings_ctx(&mut self) -> Settings<'_> {
        Settings {
            config: &mut self.prefs.config,
            theme: &mut self.theme,
            palette: &mut self.prefs.palette,
            sheet: &mut self.sheets.settings,
            file: self.prefs.file.as_deref(),
            terminal_request: &mut self.terminal_request,
            diff_cache: &mut self.render.diff_cache,
            events: Vec::new(),
        }
    }

    /// Change a setting as the sheet does: applied at once, saved at once.
    /// Integration-test seam.
    pub fn change_setting(&mut self, row: SettingsRow, up: bool) {
        let mut settings = self.settings_ctx();
        settings.change_setting(row, up);
        let events = settings.events;
        self.apply(events);
    }

    /// Set a choice row to the name at `index`, as a click on a radio does.
    pub fn set_choice(&mut self, row: SettingsRow, index: usize) {
        let mut settings = self.settings_ctx();
        settings.set_choice(row, index);
        let events = settings.events;
        self.apply(events);
    }
}

impl App {
    /// The git config screen, with the parts of the app it needs.
    pub(crate) fn git_config_ctx(&mut self) -> GitConfig<'_> {
        GitConfig {
            screen: &mut self.full_screens.git_config,
            repo: self.repo.as_deref(),
            events: Vec::new(),
        }
    }

    /// Show the git config screen over the panes, with a fresh listing.
    pub fn open_git_config(&mut self) {
        let mut config = self.git_config_ctx();
        config.open();
        let events = config.events;
        self.apply(events);
    }

    /// The renderer's word on where the git config list starts: keep it.
    pub(crate) const fn set_git_config_offset(&mut self, offset: usize) {
        self.full_screens.git_config.offset = offset;
    }
}
