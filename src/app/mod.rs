//! Application state and the draw / event loop.
//!
//! Phase 2 wired every left pane (Status, Files, Branches, Commits, Stash) to
//! a real read-only `git::Repo`. `App` owns the repo handle, the cached
//! snapshot, which left pane is focused, and one selection cursor per pane.
//! `App::mock()` is the repo-free path the render tests use.

pub mod config;
pub mod events;
pub mod hints;
pub mod keymap;
pub mod mock;
pub mod screens;
pub mod terminal;
pub mod theme;
pub mod theme_config;

use std::fmt::Write as _;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use crate::components::ui::mouse_pointer::MousePointer;
use crate::components::ui::palette::Palette;
use crate::components::ui::toast::Toast;
use crate::domain::profile::Profile;
use crate::domain::profile::settings::Settings;
use color_eyre::Result;
use ratatui::crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::{Position, Rect};
use ratatui::text::{Line, Text};

use crate::app::events::{AppEvent, Events};
use crate::app::screens as ui;
use crate::app::terminal::Tui;
use crate::components::ui::text_input::{TextInput, TextInputMode};
use crate::domain::git;
use crate::domain::git::apply::{ApplyDir, ApplyTarget};
use crate::domain::git::diff::{DiffOpts, DiffSide};
use crate::domain::git::error::GitResult;
use crate::domain::git::port::GitPort;
use crate::domain::image::detect;
use crate::domain::image::preview::{self, Preview};

/// `Operation::noun` as a function pointer for `Option::map_or`.
fn operation_noun(operation: git::model::Operation) -> &'static str {
    operation.noun()
}

/// How often the run loop wakes while an error toast is up, to count its timeout.
const TOAST_TICK_MS: u64 = 250;

pub struct App {
    /// The configuration and what it makes: keymap, palette, colour depth.
    prefs: prefs::Prefs,
    /// The side drawer and the sheets it holds: settings, dashboard.
    sheets: sheet::Sheets,
    /// The views that replace the panes: git config, welcome.
    full_screens: full_screens::FullScreens,
    /// Where the user is: focus, selection, drill-downs, tabs.
    pub nav: nav::Nav,
    /// Whether the help overlay is up.
    pub help: help::HelpState,
    /// First visible line of the help screen, and how many lines it shows
    /// (set by the renderer), so scroll keys can stop at the end.
    /// Text and focus state for the help command search.
    should_quit: bool,

    /// `None` in `App::mock()`; otherwise the open repository.
    repo: Option<Box<dyn GitPort>>,
    /// Repository directory name, shown in the status header (`ferrit -> main`).
    repo_name: String,
    /// Who commits are by: the identities git knows and ferrit's pick.
    authorship: authorship::Authorship,
    pub theme: theme_editor::ThemeEditor,
    /// What the last refresh read: header, files, branches, remotes, commits,
    /// stashes and any operation stopped mid-way.
    snapshot: git::Snapshot,
    /// Last `refresh()` failure, shown in the Status pane. Never a panic.
    last_error: Option<Arc<AppError>>,
    /// Optional worktree watcher failure; polling remains active as fallback.
    watch_error: Option<Arc<AppError>>,

    /// The right column: image preview, diff, scroll and line cursor.
    right: right_pane::RightPane,
    /// Where the last frame put the clickable things.
    hits: hit_areas::HitAreas,
    /// Whether the mouse is currently over that clickable author name.
    mouse_pointer: MousePointer,
    /// What ratatui needs mutable to show the app: animations and the toast.
    render: render_state::RenderState,
    /// The new-branch prompt's title, naming the branch it starts from (lazygit).
    new_branch_title: String,
    /// What owns the keys on top of the panes: a popup (commit box, menu,
    /// note; `docs/PLAN_7_COMMIT.md`) or a key-bar question waiting on
    /// `y` / `n` / `Esc`. One at a time, hence one value.
    modal: modal::Modal,
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
    terminal_request: Option<settings::TerminalRequest>,
    create_remote: create_remote::CreateRemote,
    /// A background fetch/pull/push's success line ("Fetched origin", "3
    /// commits pushed"), shown in the Status pane until the next remote op
    /// or the next `refresh()`. `last_error`'s sibling for the non-error
    /// case, not a repurposing of that one field with a colour flag.
    status_note: Option<String>,
}

mod authorship;
mod confirm;
mod diff_cursor;
mod drill;
pub mod full_screens;
pub mod help;
pub(crate) mod hit_areas;
mod modal;
pub mod nav;
pub mod pane;
mod popup;
mod prefs;
pub mod refresh;
mod render_state;
pub(crate) mod right_pane;
pub mod selection;
pub mod theme_editor;
mod tree;
pub mod views;
mod welcome;
pub mod workers;

mod askpass;
mod branch_actions;
mod commit;
mod context_menu;
pub mod create_remote;
pub mod dashboard;
pub mod diff_query;
mod dispatch;
mod drill_nav;
pub mod error;
pub mod git_config;
mod git_config_edit;
pub mod image_query;
mod input;
mod menu;
mod popups;
mod rebase_actions;
mod remote;
pub mod settings;
pub(crate) mod sheet;
mod staging;
mod stash_actions;

pub(crate) use error::AppError;

#[cfg(test)]
mod tests;

use self::confirm::{ConfirmAction, ConfirmPrompt};
use self::diff_cursor::{
    DiffCursor, Granule, Mode, hunk_content_id, hunk_id_at, hunk_lines_for, selectable_lines,
};
use self::drill::{BranchDrill, CommitDrill};
use self::full_screens::FullScreen;
use self::pane::{BranchesTab, PANES, Pane};
use self::popup::Popup;
use self::refresh::RefreshCompletion;
use self::render_state::RenderedDiff;
use self::selection::{SelectionKey, find_file_row_key, selection_key_for_file_rows};
use self::views::{
    BranchLog, CommandLogView, CommitPopupView, DiffView, FilesDiff, MenuView, PopupView,
};
use self::workers::{WorkerError, WorkerKind, run_worker};
use tree::{FileRow, StageState, commit_drill_files, dir_stage_state, drill_tree_rows, tree_rows};

impl App {
    fn base(repo: Option<Box<dyn GitPort>>, config: config::Config) -> Self {
        let theme_config = config.theme.clone();
        let palette = theme_config.palette();
        let keymap = keymap::Keymap::from_overrides(&config.keys).0;
        let repo_name = repo
            .as_ref()
            .map_or_else(|| "ferrit".to_owned(), |repo| repo.name());
        let git_user_name = repo.as_ref().and_then(|repo| repo.user_name());
        let (global_identities, repository_identity, effective_identity, identity_source) =
            repo.as_ref().map_or_else(
                || {
                    (
                        Vec::new(),
                        None,
                        None,
                        crate::domain::profile::settings::IdentitySource::Unset,
                    )
                },
                |repo| repo.identity_settings(),
            );
        let profile = Profile::new(Settings {
            global_identities,
            repository_identity,
            effective_identity,
            identity_source,
        });
        Self {
            prefs: prefs::Prefs::new(config, keymap, palette),
            sheets: sheet::Sheets::default(),
            full_screens: full_screens::FullScreens::default(),
            nav: nav::Nav::default(),
            help: help::HelpState::default(),
            should_quit: false,
            repo,
            repo_name,
            authorship: authorship::Authorship::new(profile, git_user_name),
            theme: theme_editor::ThemeEditor::new(theme_config),
            right: right_pane::RightPane::new(),
            snapshot: git::Snapshot::default(),
            last_error: None,
            watch_error: None,
            hits: hit_areas::HitAreas::default(),
            mouse_pointer: MousePointer::default(),
            render: render_state::RenderState::default(),
            new_branch_title: String::new(),
            modal: modal::Modal::default(),
            commit_draft: None,
            workers: workers::Workers::new(),
            watch_request: None,
            terminal_request: None,
            create_remote: create_remote::CreateRemote::default(),
            status_note: None,
        }
    }

    /// Open the repo at or above `path` with the default configuration, then
    /// take one snapshot. Reads no config file and writes none: the seam tests
    /// and examples use. The binary uses `open_with` and `Config::load`.
    pub fn open(path: &Path) -> GitResult<Self> {
        Self::open_with(path, config::ConfigLoad::default())
    }

    /// The app on any git port, with the default configuration, after one
    /// snapshot. `open` is this with the real adapter; tests pass a
    /// `domain::git::fake::FakeGit`.
    pub fn with_git(git: Box<dyn GitPort>) -> Self {
        let mut app = Self::base(Some(git), config::Config::default());
        app.refresh();
        app
    }

    /// Open the repo at or above `path` with a loaded configuration. Whatever
    /// was wrong with the file is reported once, as an error toast.
    pub fn open_with(path: &Path, load: config::ConfigLoad) -> GitResult<Self> {
        let config::ConfigLoad {
            config,
            file,
            issues,
        } = load;
        let mut app = Self::base(Some(Box::new(crate::infra::git::Repo::open(path)?)), config);
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
        load: config::ConfigLoad,
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
        if issues.is_empty() {
            return;
        }
        let location = self
            .prefs
            .file
            .as_deref()
            .map_or_else(String::new, |f| format!(" {}", f.display()));
        self.report_error(AppError::ConfigIssues {
            location,
            issues: issues.to_vec(),
        });
    }

    /// The app for a folder with no repository in it or above it: the welcome
    /// screen, offering `git init`. No panes are drawn, so none of their data
    /// is needed. `dir` is made absolute for the question.
    pub fn welcome(dir: &Path, load: config::ConfigLoad) -> Self {
        let config::ConfigLoad {
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
    pub(crate) fn help_lines(&self) -> Vec<hints::HelpLine> {
        hints::help_lines(&self.prefs.keymap, &self.key_contexts())
    }

    /// `[ui] mouse`: should the terminal capture the mouse?
    pub fn mouse_enabled(&self) -> bool {
        self.prefs.config.ui.mouse
    }

    /// `[ui] poll_secs` as a duration.
    pub(crate) fn poll_interval(&self) -> Duration {
        Duration::from_secs(self.prefs.config.ui.poll_secs)
    }

    /// `[diff]` as the options `git diff` / `git show` are run with.
    pub(crate) fn diff_opts(&self) -> DiffOpts {
        DiffOpts {
            context: self.prefs.config.diff.context,
            ignore_whitespace: self.prefs.config.diff.ignore_whitespace,
            rename_threshold: self.prefs.config.diff.rename_threshold,
        }
    }

    /// Where a settings save writes, if anywhere.
    pub fn config_file(&self) -> Option<&Path> {
        self.prefs.file.as_deref()
    }

    /// Repo-free instance backed by `mock` data, for the render tests.
    pub fn mock() -> Self {
        let mut app = Self::base(None, config::Config::default());
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
        let load = config::ConfigLoad {
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
        let branch = self
            .nav
            .branch_drill
            .as_ref()
            .map(|drill| drill.branch.clone());
        let commit = self
            .nav
            .commit_drill
            .as_ref()
            .map(|drill| drill.hash.clone());
        let opts = self.diff_opts();
        let Some(repo) = &mut self.repo else { return };
        let completion = Self::load_refresh(repo.as_mut(), branch, commit, opts);
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
        let branch = self
            .nav
            .branch_drill
            .as_ref()
            .map(|drill| drill.branch.clone());
        let commit = self
            .nav
            .commit_drill
            .as_ref()
            .map(|drill| drill.hash.clone());
        let opts = self.diff_opts();
        self.workers.refresh.in_flight = true;
        thread::spawn(move || {
            let completion = run_worker(WorkerKind::Refresh, || match handle {
                Ok(mut repo) => Self::load_refresh(repo.as_mut(), branch, commit, opts),
                Err(error) => {
                    let error = Arc::new(AppError::from(error));
                    RefreshCompletion {
                        snapshot: Err(Arc::clone(&error)),
                        profile: None,
                        branch_log: branch.map(|name| (name, Err(Arc::clone(&error)))),
                        commit_files: commit.map(|hash| (hash, Err(error))),
                    }
                },
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

    fn load_refresh(
        repo: &mut dyn GitPort,
        branch: Option<String>,
        commit: Option<String>,
        opts: DiffOpts,
    ) -> RefreshCompletion {
        let snapshot = repo
            .snapshot()
            .map_err(|error| Arc::new(AppError::from(error)));
        let branch_log = branch.map(|name| {
            let result = repo
                .branch_log(&name)
                .map_err(|error| Arc::new(AppError::from(error)));
            (name, result)
        });
        let commit_files = commit.map(|hash| {
            let result = repo
                .commit_diff(&hash, opts)
                .map(|diff| commit_drill_files(&diff))
                .map_err(|error| Arc::new(AppError::from(error)));
            (hash, result)
        });
        RefreshCompletion {
            snapshot,
            profile: Some({
                let (global_identities, repository_identity, effective_identity, identity_source) =
                    repo.identity_settings();
                Profile::new(Settings {
                    global_identities,
                    repository_identity,
                    effective_identity,
                    identity_source,
                })
            }),
            branch_log,
            commit_files,
        }
    }

    fn apply_refresh_result(&mut self, completion: RefreshCompletion) {
        if let Some(profile) = completion.profile {
            self.authorship.profile = profile;
        }
        let old_selection: [(Pane, usize, Option<SelectionKey>); 5] =
            PANES.map(|pane| (pane, self.nav.selection[pane], self.selection_key(pane)));
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

        // A drilled branch log (Enter on Branches) stays live across a
        // background refresh instead of going stale; a branch that vanished
        // (deleted, renamed) backs out of the drill-down instead of erroring
        // the whole refresh.
        if let Some((branch, result)) = completion.branch_log
            && self
                .nav
                .branch_drill
                .as_ref()
                .is_some_and(|drill| drill.branch == branch)
        {
            match result {
                Ok(commits) => {
                    if let Some(drill) = &mut self.nav.branch_drill {
                        drill.commits = commits;
                    }
                },
                Err(_) => self.nav.branch_drill = None,
            }
        }

        // Same treatment for a drilled commit's file tree (Enter on
        // Commits): re-read the file list so it reflects the diff as of
        // this refresh; a commit that vanished (e.g. a reword/rebase that
        // changed its hash) backs out rather than erroring the refresh.
        if let Some((hash, result)) = completion.commit_files
            && self
                .nav
                .commit_drill
                .as_ref()
                .is_some_and(|drill| drill.hash == hash)
        {
            match result {
                Ok(files) => {
                    if let Some(drill) = &mut self.nav.commit_drill {
                        drill.files = files;
                    }
                },
                Err(_) => self.nav.commit_drill = None,
            }
        }

        for (pane, old_index, key) in old_selection {
            let last = self.row_count(pane).saturating_sub(1);
            let new_index = key
                .as_ref()
                .and_then(|key| self.find_selection_key(pane, key))
                .unwrap_or(old_index);
            self.nav.selection[pane] = new_index.min(last);
        }
        let mut waiting = std::mem::take(&mut self.nav.select_when_listed);
        waiting.retain(|(pane, key)| match self.find_selection_key(*pane, key) {
            Some(index) => {
                self.nav.selection[*pane] = index;
                false
            },
            None => true,
        });
        self.nav.select_when_listed = waiting;
        self.workers.diff.refresh_requested = true;
        self.invalidate_image_query();
        self.update_right_pane();
    }

    /// After an action that created `key`'s row, select it in `pane` as soon as
    /// a refresh lists it.
    pub(super) fn select_when_listed(&mut self, pane: Pane, key: SelectionKey) {
        self.nav.select_when_listed.retain(|(p, _)| *p != pane);
        self.nav.select_when_listed.push((pane, key));
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
    fn update_right_pane(&mut self) {
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
            self.clamp_right_scroll();
        }
    }

    /// Called after *every* key while `Mode::Diff` is up (`update_diff` runs
    /// on every keystroke, not just after a stage), so a plain `j`/`k` must
    /// come through untouched: only re-find the cursor when the line it
    /// names actually stopped being valid — the hunk it was on shrank out
    /// from under it (a line-level stage) or moved off this side entirely
    /// (a whole-hunk stage), `docs/PLAN_6_STAGING.md` "After the apply:
    /// refresh, keep your place". A still-selectable line, hunk unchanged,
    /// is left exactly where it was.
    ///
    /// Gone -> clamp to the nearest remaining hunk on the *same* side, or
    /// drop to `Mode::Nav` once that side has no more changes to show at
    /// all — even if the other side now does; switching sides on the user's
    /// behalf would silently change what the next `<space>` does.
    fn resync_diff_cursor(&mut self) {
        if self.nav.mode != Mode::Diff {
            return;
        }
        let DiffView::Files(files) = &self.right.diff else {
            self.nav.mode = Mode::Nav;
            return;
        };
        let diff = match self.right.cursor.side {
            DiffSide::Worktree => &files.unstaged,
            DiffSide::Staged => &files.staged,
        };
        let hunks = hunk_lines_for(diff);

        if let Some(hl) = hunks
            .iter()
            .find(|hl| hunk_content_id(diff, hl.hunk_index) == self.right.cursor.hunk_id)
        {
            if hl.selectable.contains(&self.right.cursor.line) {
                self.right.cursor.anchor =
                    self.right.cursor.anchor.filter(|a| hl.lines.contains(a));
                self.ensure_cursor_visible();
                return;
            }
            if let Some(&line) = hl.selectable.first() {
                self.right.cursor.line = line;
                self.right.cursor.anchor = None;
                self.ensure_cursor_visible();
                return;
            }
        }

        if let Some(hl) = hunks.iter().find(|hl| !hl.selectable.is_empty())
            && let Some(&line) = hl.selectable.first()
        {
            self.right.cursor.line = line;
            self.right.cursor.anchor = None;
            self.right.cursor.hunk_id = hunk_content_id(diff, hl.hunk_index);
            self.ensure_cursor_visible();
        } else {
            self.nav.mode = Mode::Nav;
        }
    }

    /// Files pane rows, lazygit-style directory tree: single-child directory
    /// chains folded, a root ("/") first only when it has two or more
    /// children, changed files grouped under directory header rows. Empty when nothing changed. Built fresh from
    /// `self.snapshot.files` and `self.nav.collapsed_dirs` on every call; cheap at
    /// working-tree sizes, same choice `branch_lines`/`commit_lines` make.
    fn files_tree_rows(&self) -> Vec<FileRow> {
        tree_rows(&self.snapshot.files, &self.nav.collapsed_dirs)
    }

    /// Same tree shape as `files_tree_rows`, over a drilled commit's own
    /// changed files instead of the worktree's. Empty while not drilled.
    fn commit_tree_rows(&self) -> Vec<FileRow> {
        match &self.nav.commit_drill {
            Some(drill) => drill_tree_rows(&drill.files, &drill.collapsed),
            None => Vec::new(),
        }
    }

    /// Line count of the current diff text, 0 for `None` / `Note`.
    fn diff_line_count(&self) -> usize {
        match &self.right.diff {
            // Both columns share one scroll; the taller sets how far it goes.
            DiffView::Files(f) => f
                .unstaged
                .text
                .lines()
                .count()
                .max(f.staged.text.lines().count()),
            DiffView::Commit(_, d) | DiffView::Stash(_, d) => d.text.lines().count(),
            DiffView::BranchLog(log) => log.commits.iter().map(theme::branch_log_block_lines).sum(),
            DiffView::None | DiffView::Note(_) => 0,
        }
    }

    /// Largest first-visible line that still fills the viewport: the last diff
    /// line lands at the bottom of the pane, never above it. Falls back to
    /// "line count minus one screen" until the first draw sets a real height.
    fn max_right_scroll(&self) -> usize {
        self.diff_line_count()
            .saturating_sub(self.right.viewport.max(1))
    }

    /// Clamp `right_scroll` into `0..=max_right_scroll()`.
    fn clamp_right_scroll(&mut self) {
        self.right.scroll = self.right.scroll.min(self.max_right_scroll());
    }

    /// Move the right-pane viewport by `delta` lines, clamped so it stops with
    /// the last line at the bottom of the pane. `isize::MIN` / `isize::MAX`
    /// snap to the top / bottom.
    fn scroll_right(&mut self, delta: isize) {
        let mag = delta.unsigned_abs();
        self.right.scroll = if delta >= 0 {
            self.right
                .scroll
                .saturating_add(mag)
                .min(self.max_right_scroll())
        } else {
            self.right.scroll.saturating_sub(mag)
        };
    }

    /// Is the right pane scrollable right now — a real diff, or a branch's
    /// log preview? The scroll keys and the wheel are inert over an image, a
    /// `Note`, and the mock bodies; without this, they leak through to the
    /// left pane's own selection instead (moving the wrong thing).
    fn right_is_diff(&self) -> bool {
        matches!(
            self.right.diff,
            DiffView::Files(_)
                | DiffView::Commit(..)
                | DiffView::Stash(..)
                | DiffView::BranchLog(_)
        )
    }

    /// Jump `right_scroll` to the next (`dir > 0`) or previous hunk / file
    /// header, lazygit's `]` / `[`. `diff --git` headers for a commit diff;
    /// a no-op on the Files split, which has two diffs and no single anchor
    /// list to jump through.
    fn jump_diff_anchor(&mut self, dir: isize) {
        let anchors = match &self.right.diff {
            DiffView::Commit(_, d) | DiffView::Stash(_, d) => d.file_lines(),
            DiffView::None | DiffView::Note(_) | DiffView::BranchLog(_) | DiffView::Files(_) => {
                return;
            },
        };
        let cur = self.right.scroll;
        let target = if dir > 0 {
            anchors.iter().find(|&&l| l > cur).copied()
        } else {
            anchors.iter().rev().find(|&&l| l < cur).copied()
        };
        if let Some(line) = target {
            self.right.scroll = line;
            self.clamp_right_scroll();
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
    pub fn dashboard(&self) -> &dashboard::Dashboard {
        &self.sheets.dashboard
    }

    pub fn git_user_name(&self) -> Option<&str> {
        self.authorship.git_user_name.as_deref()
    }

    /// Return cached styled diff. Cache invalidates on selection, diff text,
    /// focus range, or pane width; pure scrolling reuses `Text`. Only a
    /// commit diff goes through this cache: it is keyed for one `Diff` at a
    /// time, and the Files split renders its two sides directly instead
    /// (`ui::draw_files_columns`).
    pub(crate) fn rendered_diff(
        &self,
        cache: &mut Option<RenderedDiff>,
        focus: Option<&Range<usize>>,
        width: usize,
    ) -> Option<(Text<'static>, usize, git::diff::DiffStat)> {
        let key = &self.right.key;
        match &self.right.diff {
            DiffView::Commit(_, diff) | DiffView::Stash(_, diff) => {
                let cache_hit = cache.as_ref().is_some_and(|cached| {
                    cached.key.as_ref() == key.as_ref()
                        && cached.source == diff.text
                        && cached.focus.as_ref() == focus
                        && cached.width == width
                });
                if !cache_hit {
                    let text = diff.delta_output(width).map_or_else(
                        || theme::render_diff(&self.prefs.palette, diff, focus, width),
                        |formatted| theme::render_delta(&formatted, width),
                    );
                    *cache = Some(RenderedDiff {
                        key: key.clone(),
                        source: diff.text.clone(),
                        focus: focus.cloned(),
                        width,
                        text,
                    });
                }
                cache
                    .as_ref()
                    .map(|cached| (cached.text.clone(), cached.text.lines.len(), diff.stat()))
            },
            DiffView::None | DiffView::Note(_) | DiffView::BranchLog(_) | DiffView::Files(_) => {
                None
            },
        }
    }

    /// First visible line of the right-pane diff.
    pub fn right_scroll(&self) -> usize {
        self.right.scroll
    }

    /// Set the right-pane scroll. Test and example helper.
    pub fn set_right_scroll(&mut self, line: usize) {
        self.right.scroll = line;
        self.clamp_right_scroll();
    }

    /// Inner height of the right-pane diff box, written by `ui::draw_right_pane`
    /// each frame so the scroll clamp and page steps track the real size.
    pub fn set_right_viewport(&mut self, rows: usize) {
        self.right.viewport = rows;
        self.clamp_right_scroll();
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
    pub fn set_color_depth(&mut self, depth: crate::components::ui::scheme::ColorDepth) {
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
        self.hits.list_offset[pane]
    }

    /// A left pane's list scroll offset, written by `ui::draw_left_column`
    /// after `render_stateful_widget` so a click in a scrolled list maps to
    /// the right row.
    pub fn set_list_offset(&mut self, pane: Pane, offset: usize) {
        self.hits.list_offset[pane] = offset;
    }

    /// Whether a wheel scroll left `pane`'s view away from its selection. Read
    /// by `ui::draw_left_column`; forgets a detachment the selection has left.
    pub fn view_detached(&self, pane: Pane) -> bool {
        self.hits.view_detached_at[pane] == Some(self.nav.selection[pane])
    }

    /// Scroll `pane`'s list by `rows` (negative is up) and keep its selection
    /// where it is, which may leave it off screen. `draw_left_column` clamps
    /// the offset to the list's length on the next frame.
    pub(super) fn scroll_list(&mut self, pane: Pane, rows: isize) {
        self.hits.view_detached_at[pane] = Some(self.nav.selection[pane]);
        self.hits.list_offset[pane] = self.hits.list_offset[pane].saturating_add_signed(rows);
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
            AppEvent::Askpass { prompt, reply } => self.on_askpass(prompt, reply),
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

    /// Selection cursor for a given pane.
    pub fn selected(&self, pane: Pane) -> usize {
        self.nav.selection[pane]
    }

    /// Selectable row count for a pane, for clamping the cursor and deciding
    /// whether to draw a highlight.
    pub fn row_count(&self, pane: Pane) -> usize {
        match pane {
            Pane::Status => 0,
            Pane::Files => self.files_tree_rows().len(),
            Pane::Branches if self.nav.branches_tab == BranchesTab::Remotes => 0,
            Pane::Branches => self
                .nav
                .branch_drill
                .as_ref()
                .map_or(self.snapshot.branches.len(), |drill| drill.commits.len()),
            Pane::Commits => match &self.nav.commit_drill {
                Some(_) => self.commit_tree_rows().len(),
                None => self.snapshot.commits.len(),
            },
            Pane::Stash => self.snapshot.stashes.len(),
        }
    }

    fn selection_key(&self, pane: Pane) -> Option<SelectionKey> {
        match pane {
            Pane::Status => None,
            Pane::Files => selection_key_for_file_rows(
                &self.files_tree_rows(),
                &self.snapshot.files,
                self.selected(pane),
            ),
            Pane::Branches if self.nav.branches_tab == BranchesTab::Remotes => None,
            Pane::Branches => self.nav.branch_drill.as_ref().map_or_else(
                || {
                    self.snapshot
                        .branches
                        .get(self.selected(pane))
                        .map(|entry| SelectionKey::Branch(entry.name.clone()))
                },
                |drill| {
                    drill
                        .commits
                        .get(self.selected(pane))
                        .map(|entry| SelectionKey::Commit(entry.full_hash.clone()))
                },
            ),
            Pane::Commits => self.nav.commit_drill.as_ref().map_or_else(
                || {
                    self.snapshot
                        .commits
                        .get(self.selected(pane))
                        .map(|entry| SelectionKey::Commit(entry.full_hash.clone()))
                },
                |drill| {
                    selection_key_for_file_rows(
                        &self.commit_tree_rows(),
                        &drill.files,
                        self.selected(pane),
                    )
                },
            ),
            Pane::Stash => self
                .snapshot
                .stashes
                .get(self.selected(pane))
                .map(|entry| SelectionKey::Stash(entry.oid.clone())),
        }
    }

    fn find_selection_key(&self, pane: Pane, key: &SelectionKey) -> Option<usize> {
        match (pane, key) {
            (Pane::Files, SelectionKey::File(_) | SelectionKey::Directory(_)) => {
                find_file_row_key(&self.files_tree_rows(), &self.snapshot.files, key)
            },
            (Pane::Branches, SelectionKey::Branch(name)) if self.nav.branch_drill.is_none() => self
                .snapshot
                .branches
                .iter()
                .position(|entry| entry.name == *name),
            (Pane::Branches, SelectionKey::Commit(hash)) => self
                .nav
                .branch_drill
                .as_ref()?
                .commits
                .iter()
                .position(|entry| entry.full_hash == *hash),
            (Pane::Commits, SelectionKey::Commit(hash)) if self.nav.commit_drill.is_none() => self
                .snapshot
                .commits
                .iter()
                .position(|entry| entry.full_hash == *hash),
            (Pane::Commits, SelectionKey::File(_) | SelectionKey::Directory(_)) => {
                let drill = self.nav.commit_drill.as_ref()?;
                find_file_row_key(&self.commit_tree_rows(), &drill.files, key)
            },
            (Pane::Stash, SelectionKey::Stash(oid)) => self
                .snapshot
                .stashes
                .iter()
                .position(|entry| entry.oid == *oid),
            _ => None,
        }
    }

    /// `(current, total)` for the pane's `N of M` border counter, or `None`
    /// when the pane has no selectable rows.
    pub fn counter(&self, pane: Pane) -> Option<(usize, usize)> {
        let total = self.row_count(pane);
        (total > 0).then(|| (self.selected(pane).min(total - 1) + 1, total))
    }

    /// Status pane: lazygit's one-liner `ferrit -> main ↑2`, plus a conflict
    /// line only when there are conflicts, or the error when `refresh()` failed.
    pub fn status_lines(&self) -> Vec<Line<'static>> {
        let mut out = if let Some(err) = &self.last_error {
            vec![theme::error_line(
                &self.prefs.palette,
                &format!("error: {err}"),
            )]
        } else {
            let h = &self.snapshot.header;
            let mut line = format!("{} \u{2192} {}", self.repo_name, h.branch);
            if h.ahead > 0 {
                let _ = write!(line, " \u{2191}{}", h.ahead);
            }
            if h.behind > 0 {
                let _ = write!(line, " \u{2193}{}", h.behind);
            }
            if h.upstream.is_some() && h.ahead == 0 && h.behind == 0 {
                line.push_str(" \u{2713}");
            }
            let mut lines = vec![theme::status_line(&self.prefs.palette, &line)];
            if h.conflicts > 0 {
                lines.push(theme::error_line(
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
                theme::operation_line(&self.prefs.palette, &operation.label()),
            );
        }
        if let Some(label) = self.remote_busy_label() {
            out.push(theme::busy_line(&self.prefs.palette, label));
        } else if self.last_error.is_none()
            && let Some(note) = &self.status_note
        {
            out.push(theme::status_line(&self.prefs.palette, note));
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

    /// Branches pane rows: the branch list, or one branch's own commit log
    /// while drilled in (`branch_drill`, `enter_branch_log`), each with its
    /// own empty-state line.
    pub fn branch_lines(&self) -> Vec<Line<'static>> {
        if let Some(drill) = &self.nav.branch_drill {
            if drill.commits.is_empty() {
                return vec![Line::raw("no commits yet")];
            }
            return drill
                .commits
                .iter()
                .map(|entry| theme::commit_line(&self.prefs.palette, entry))
                .collect();
        }
        if self.nav.branches_tab == BranchesTab::Remotes {
            if self.snapshot.remotes.is_empty() {
                return vec![Line::raw("no remotes configured")];
            }
            return self
                .snapshot
                .remotes
                .iter()
                .map(|entry| theme::remote_line(&self.prefs.palette, entry))
                .collect();
        }
        if self.snapshot.branches.is_empty() {
            return vec![Line::raw("no local branches")];
        }
        self.snapshot
            .branches
            .iter()
            .map(|branch| {
                let operation = branch
                    .is_head
                    .then(|| self.remote_branch_status())
                    .flatten();
                theme::branch_line_with_status(&self.prefs.palette, branch, operation.as_deref())
            })
            .collect()
    }

    /// `[3] Local branches - Remotes - Tags`, or `[3] Commits (<branch>)`
    /// while drilled into a branch's log (Enter on a branch, `Esc` to back
    /// out; see `enter_branch_log`).
    pub fn branches_title(&self) -> String {
        match &self.nav.branch_drill {
            Some(drill) => format!("[3] Commits ({})", drill.branch),
            None => Pane::Branches.title().to_owned(),
        }
    }

    /// Whether the Branches pane is drilled into one branch's own commit
    /// log right now. `ui::draw_keybar` uses this to fall back to the
    /// default keybar there — `<space>`/`n`/`d`/`u`/`M` act on a branch
    /// list row, not a commit row, so the Branches-specific hints would be
    /// misleading while drilled in.
    pub fn branches_drilled(&self) -> bool {
        self.nav.branch_drill.is_some()
    }

    /// Is the selected Files row a directory (the root row included)?
    pub fn files_selection_is_dir(&self) -> bool {
        matches!(
            self.files_tree_rows().get(self.selected(Pane::Files)),
            Some(FileRow::Dir { .. })
        )
    }

    /// Is the Commits pane showing one commit's changed files instead of the
    /// commit list? The commit rewrite keys and their keybar apply only to the
    /// list.
    pub fn commits_drilled(&self) -> bool {
        self.nav.commit_drill.is_some()
    }

    /// Commits pane rows: the commit list, or one commit's own changed-file
    /// tree while drilled in (`commit_drill`, `enter_commit_files`), same
    /// shape `branch_lines` gives the Branches pane.
    pub fn commit_lines(&self) -> Vec<Line<'static>> {
        if let Some(drill) = &self.nav.commit_drill {
            return self
                .commit_tree_rows()
                .iter()
                .filter_map(|row| match row {
                    FileRow::Dir {
                        name,
                        depth,
                        expanded,
                        ..
                    } => Some(theme::dir_line(
                        &self.prefs.palette,
                        name,
                        *depth,
                        *expanded,
                        StageState::None,
                    )),
                    FileRow::File { index, depth } => drill
                        .files
                        .get(*index)
                        .map(|entry| theme::file_line(&self.prefs.palette, entry, *depth)),
                })
                .collect();
        }
        if self.snapshot.commits.is_empty() {
            return vec![Line::raw("no commits yet")];
        }
        self.snapshot
            .commits
            .iter()
            .map(|entry| theme::commit_line(&self.prefs.palette, entry))
            .collect()
    }

    /// `[4] Commits - Reflog`, or `[4] Diff files (<hash> <summary>)` while
    /// drilled into a commit's own changed-file tree (Enter on a commit,
    /// `Esc` to back out; see `enter_commit_files`).
    pub fn commits_title(&self) -> String {
        match &self.nav.commit_drill {
            Some(drill) => format!("[4] Diff files ({})", drill.title),
            None => Pane::Commits.title().to_owned(),
        }
    }

    /// Stash pane rows, or the empty-state line.
    pub fn stash_lines(&self) -> Vec<Line<'static>> {
        if self.snapshot.stashes.is_empty() {
            return vec![Line::raw("(no stash entries)")];
        }
        self.snapshot
            .stashes
            .iter()
            .map(|entry| theme::stash_line(&self.prefs.palette, entry))
            .collect()
    }

    /// Porcelain-style `XY path` text for one Files tree row, or an empty
    /// string for a directory row. Debug/probe helper; keyed by the same
    /// row index `file_lines`/`row_count` use, not a flat index into
    /// `self.snapshot.files`.
    pub fn file_display(&self, i: usize) -> String {
        match self.files_tree_rows().get(i) {
            Some(&FileRow::File { index, .. }) => self
                .snapshot
                .files
                .get(index)
                .map(git::model::FileEntry::display)
                .unwrap_or_default(),
            _ => String::new(),
        }
    }

    /// Files pane rows, or a single "working tree clean" line: a flat list,
    /// or lazygit's directory tree once any changed file sits below the
    /// repo root (`files_tree_rows`).
    pub fn file_lines(&self) -> Vec<Line<'static>> {
        if self.snapshot.files.is_empty() {
            return vec![Line::raw("working tree clean")];
        }
        self.files_tree_rows()
            .iter()
            .filter_map(|row| match row {
                FileRow::Dir {
                    path,
                    name,
                    depth,
                    expanded,
                } => Some(theme::dir_line(
                    &self.prefs.palette,
                    name,
                    *depth,
                    *expanded,
                    dir_stage_state(&self.snapshot.files, path),
                )),
                FileRow::File { index, depth } => self
                    .snapshot
                    .files
                    .get(*index)
                    .map(|entry| theme::file_line(&self.prefs.palette, entry, *depth)),
            })
            .collect()
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
    fn repo_handle(&self) -> Option<Box<dyn GitPort>> {
        self.repo.as_ref()?.reopen().ok()
    }

    /// A second handle on the repository for a worker thread, or why it could
    /// not be opened; `None` without a repository.
    fn reopen_repo(&self) -> Option<GitResult<Box<dyn GitPort>>> {
        self.repo.as_ref().map(|repo| repo.reopen())
    }

    /// Draw, then block for the next event batch, until `should_quit`. Events
    /// come from terminal input, a recursive worktree watch, and a poll (`[ui]
    /// poll_secs`, 10s by default).
    /// Bounded batches avoid repainting for every auto-repeat key while still
    /// guaranteeing regular redraws during sustained input.
    pub fn run(&mut self, terminal: &mut Tui) -> Result<()> {
        let mut events = Events::new(self.watch_root().as_deref(), self.poll_interval())?;
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

            let was_animating = self.render.sheet.is_animating();
            let help_was_animating = self.render.help.is_animating();
            let toast_animating = self.render.toast.as_ref().is_some_and(Toast::is_animating);
            let remote_animating = self.workers.remote_busy.is_some();
            // Frames while something animates; a slower tick while a toast is up,
            // so it can time out without waiting for a key.
            let timeout =
                (was_animating || help_was_animating || toast_animating || remote_animating)
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
                        let elapsed = overlay_tick.elapsed();
                        self.render.sheet.tick(elapsed);
                        self.render.help.tick(elapsed);
                        self.tick_toast(elapsed);
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
                        let toast_consumed = self
                            .render
                            .toast
                            .as_mut()
                            .is_some_and(|toast| toast.on_mouse(m));
                        if toast_consumed {
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
            if let Some(settings::TerminalRequest::Mouse(on)) = self.take_terminal_request()
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
            if self.render.sheet.is_animating() && was_animating {
                self.render.sheet.tick(overlay_tick.elapsed());
            }
            if self.render.help.is_animating() && help_was_animating {
                self.render.help.tick(overlay_tick.elapsed());
            }
            self.tick_toast(overlay_tick.elapsed());
            overlay_tick = Instant::now();
        }
        if self.workers.remote_worker.is_some() {
            self.workers
                .remote_cancel
                .store(true, std::sync::atomic::Ordering::Release);
            if let Some(worker) = self.workers.remote_worker.take() {
                let _ = worker.join();
            }
            self.workers.remote_busy = None;
        }
        self.sheets.dashboard.stop_and_join();
        Ok(())
    }

    /// Advance the toast's animation and its timeout, and the settings sheet's
    /// slide, by `elapsed`, as the run loop does. Integration-test seam: a test has no loop to wait on.
    #[doc(hidden)]
    pub fn advance_clock(&mut self, elapsed: Duration) {
        self.tick_toast(elapsed);
        self.render.sheet.tick(elapsed);
        self.render.help.tick(elapsed);
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
        match &mut self.render.toast {
            Some(toast) if !toast.is_closing() => {
                toast.dismiss();
                true
            },
            _ => false,
        }
    }

    fn tick_toast(&mut self, elapsed: Duration) {
        if let Some(toast) = &mut self.render.toast {
            toast.tick(elapsed);
            if toast.is_closed() {
                self.render.toast = None;
            }
        }
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
