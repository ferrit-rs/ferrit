//! Application state and the composition root.
//!
//! Phase 2 wired every left pane (Status, Files, Branches, Commits, Stash) to
//! a real read-only `git::Repo`. `App` owns the repo handle, the cached
//! snapshot, which left pane is focused, and one selection cursor per pane.
//! `App::mock()` is the repo-free path the render tests use.

use crate::config::settings::SettingsRow;
use crate::git::commit::CommitKind;
use crate::tui::components::commit_editor;
use crate::tui::components::create_remote::CreateRemoteState;
use crate::tui::components::git_config::keys::GitConfig;
use crate::tui::components::keybar::help::HelpLine;
use crate::tui::components::keybar::help::help_lines;
use crate::tui::components::popups::Popup;
use crate::tui::components::settings::Settings;
use crate::tui::widgets::toast::Toast;
pub mod events;
pub mod mock;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;

use crate::tui::widgets::chrome::MousePointer;
use ratatui::crossterm::event::KeyEvent;

use crate::git;
use crate::git::error::GitResult;
use crate::git::port::GitPort;
use crate::tui::error::AppError;
use crate::tui::events::AppEvent;
use crate::tui::image::detect;

/// `Operation::noun` as a function pointer for `Option::map_or`.
pub(crate) fn operation_noun(operation: git::model::Operation) -> &'static str {
    operation.noun()
}

pub struct App {
    /// The configuration and what it makes: keymap, palette, colour depth.
    prefs: prefs::Prefs,
    /// The side drawer and the sheets it holds: settings, dashboard.
    sheets: components::dashboard::Sheets,
    /// The views that replace the panes: git config, welcome.
    full_screens: draw::FullScreens,
    /// Where the user is: focus, selection, drill-downs, tabs.
    pub nav: components::panes::nav::Nav,
    /// Whether the help overlay is up.
    pub help: components::help::HelpState,
    should_quit: bool,

    /// `None` in `App::mock()`; otherwise the open repository.
    repo: Option<Box<dyn GitPort>>,
    /// Repository directory name, shown in the status header (`ferrit -> main`).
    repo_name: String,
    /// Who commits are by: the identities git knows and ferrit's pick.
    authorship: git::identity::Authorship,
    pub theme: components::settings::theme::ThemeEditor,
    /// What the last refresh read: header, files, branches, remotes, commits,
    /// stashes and any operation stopped mid-way.
    snapshot: git::Snapshot,
    /// Last `refresh()` failure, shown in the Status pane. Never a panic.
    last_error: Option<Arc<AppError>>,
    /// Optional worktree watcher failure; polling remains active as fallback.
    watch_error: Option<Arc<AppError>>,

    /// The right column: image preview, diff, scroll and line cursor.
    right: components::diff::right_pane::RightPane,
    /// Where the last frame put the clickable things.
    hits: components::panes::hit_areas::HitAreas,
    /// Whether the mouse is currently over that clickable author name.
    mouse_pointer: MousePointer,
    /// What ratatui needs mutable to show the app: animations and the toast.
    render: draw::RenderState,
    /// The new-branch prompt's title, naming the branch it starts from (lazygit).
    new_branch_title: String,
    /// What owns the keys on top of the panes: a popup (commit box, menu,
    /// note; `docs/PLAN_7_COMMIT.md`) or a key-bar question waiting on
    /// `y` / `n` / `Esc`. One at a time, hence one value.
    modal: components::popups::Modal,
    /// The last commit popup's text, kept across an `Esc`-cancel so a
    /// mistyped keystroke never loses a paragraph. Cleared on a successful
    /// commit.
    commit_draft: Option<String>,
    /// Background work in flight: the event channel, the refresh, diff and
    /// image workers, and the one network operation at a time.
    workers: workers::Workers,
    /// Set when the app was rebuilt on a new repository: `run` points the
    /// filesystem watch at this root and clears it.
    watch_request: Option<PathBuf>,
    /// A change the run loop has to carry out in the terminal, once.
    terminal_request: Option<crate::config::settings::TerminalRequest>,
    create_remote: CreateRemoteState,
    /// A background fetch/pull/push's success line ("Fetched origin", "3
    /// commits pushed"), shown in the Status pane until the next remote op
    /// or the next `refresh()`. `last_error`'s sibling for the non-error
    /// case, not a repurposing of that one field with a colour flag.
    status_note: Option<String>,
}

pub mod workers;

pub mod api;
pub mod components;
mod controllers;
pub mod draw;
pub mod error;
pub mod event;
pub mod image;
pub mod input;
pub mod keymap;
pub mod loading;
pub mod prefs;
pub mod publish;
mod reducer;
pub mod row_lines;
mod runtime;
pub mod scene;
pub mod terminal;
pub mod view;
pub mod widgets;

#[cfg(test)]
mod tests;

use crate::tui::workers::RefreshCompletion;
use crate::tui::workers::{WorkerKind, run_worker};
use components::diff::right_pane::Mode;
use components::diff::views::DiffView;
use components::files::tree::{FileRow, drill_tree_rows};
use components::panes::nav::Pane;
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
        let authorship = git::identity::Authorship::of(repo.as_deref());
        Self {
            prefs: prefs::Prefs::new(config, keymap, palette),
            sheets: components::dashboard::Sheets::default(),
            full_screens: draw::FullScreens::default(),
            nav: components::panes::nav::Nav::default(),
            help: components::help::HelpState::default(),
            should_quit: false,
            repo,
            repo_name,
            authorship,
            theme: components::settings::theme::ThemeEditor::new(theme_config),
            right: components::diff::right_pane::RightPane::new(),
            snapshot: git::Snapshot::default(),
            last_error: None,
            watch_error: None,
            hits: components::panes::hit_areas::HitAreas::default(),
            mouse_pointer: MousePointer::default(),
            render: draw::RenderState::default(),
            new_branch_title: String::new(),
            modal: components::popups::Modal::default(),
            commit_draft: None,
            workers: workers::Workers::new(),
            watch_request: None,
            terminal_request: None,
            create_remote: CreateRemoteState::default(),
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
    pub(crate) fn request_refresh(&mut self) {
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

    /// Keep persistent Status text while moving typed error into transient toast.
    pub(crate) fn report_error(&mut self, error: impl Into<AppError>) {
        let error = Arc::new(error.into());
        self.last_error = Some(Arc::clone(&error));
        self.render.toast = Some(Toast::error(error));
    }

    pub(crate) fn report_notice(&mut self, message: impl Into<String>) {
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
}

impl App {
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
