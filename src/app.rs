//! Application state and the draw / event loop.
//!
//! Phase 2 wired every left pane (Status, Files, Branches, Commits, Stash) to
//! a real read-only `git::Repo`. `App` owns the repo handle, the cached
//! snapshot, which left pane is focused, and one selection cursor per pane.
//! `App::mock()` is the repo-free path the render tests use.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use color_eyre::Result;
use enum_map::{Enum, EnumMap};
use ratatui::crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::{Position, Rect};
use ratatui::text::Line;
use ratatui_image::picker::Picker;

use crate::events::{AppEvent, Events};
use crate::git::{self, DiffOpts, DiffSide, GitResult};
use crate::image::detect;
use crate::image::preview::{self, Preview};
use crate::tui::Tui;
use crate::{mock, theme, ui};

/// What the right pane shows behind the image preview. A second cached,
/// rebuilt-on-nav value alongside `preview`, not a replacement: an image
/// selection still wins. See `docs/PLAN_3_DIFF_VIEW.md`.
#[derive(Debug, Clone, Default)]
pub enum DiffView {
    /// Status / Branches / Stash focused: no real diff, the mock text shows.
    #[default]
    None,
    /// Read failed, or nothing to show. A dim single line, never a panic.
    Note(String),
    /// Files pane: one file's `git diff`.
    Files(git::Diff),
    /// Commits pane: one commit's metadata and `git show` diff.
    Commit(git::CommitEntry, git::Diff),
}

/// Identity of what `DiffView` describes. `update_right_pane` resets the scroll
/// only when this changes, so a background refresh of an unchanged selection
/// keeps its viewport.
#[derive(Debug, Clone, PartialEq, Eq)]
enum RightKey {
    File { path: PathBuf, side: DiffSide },
    Commit { full_hash: String },
}

/// Mouse-wheel step for the right pane, in lines. Matches gitu's default
/// `mouse_scroll_lines`.
const WHEEL_LINES: isize = 3;

/// `(discriminant, diff text)` for cheap "did the right pane actually change"
/// checks: `String` equality on a few KB, no hashing.
fn view_sig(v: &DiffView) -> (u8, &str) {
    match v {
        DiffView::None => (0, ""),
        DiffView::Note(m) => (1, m.as_str()),
        DiffView::Files(d) => (2, d.text.as_str()),
        DiffView::Commit(_, d) => (3, d.text.as_str()),
    }
}

/// The five left panes, in top-to-bottom screen order.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug, Enum)]
pub enum Pane {
    #[default]
    Status,
    Files,
    Branches,
    Commits,
    Stash,
}

/// Panes in order. Index into this is also the index into `App::selection`.
pub const PANES: [Pane; 5] = [
    Pane::Status,
    Pane::Files,
    Pane::Branches,
    Pane::Commits,
    Pane::Stash,
];

impl Pane {
    /// Position in `PANES`, for the focus-cycling arithmetic in `pane_offset`.
    /// The variant order is the `PANES` order, so the discriminant is it.
    pub fn index(self) -> usize {
        self as usize
    }

    /// Bordered-box title, lazygit style: `[N] Tab - Tab - Tab`. The extra tab
    /// names are inert labels for now; only the first is a real view.
    pub fn title(self) -> &'static str {
        match self {
            Self::Status => "[1] Status",
            Self::Files => "[2] Files - Worktrees - Submodules",
            Self::Branches => "[3] Local branches - Remotes - Tags",
            Self::Commits => "[4] Commits - Reflog",
            Self::Stash => "[5] Stash",
        }
    }

    /// Contextual title for the right pane when this left pane has focus,
    /// matching what lazygit shows there.
    pub fn right_title(self) -> &'static str {
        match self {
            Self::Status => " Status ",
            Self::Files => " Unstaged changes ",
            Self::Branches => " Log ",
            Self::Commits => " Commit ",
            Self::Stash => " Stash ",
        }
    }
}

pub struct App {
    /// Which left pane has focus.
    pub focus: Pane,
    /// Selection cursor per pane, keyed by `Pane`.
    pub selection: EnumMap<Pane, usize>,
    /// Whether the help overlay is up.
    pub show_help: bool,
    should_quit: bool,

    /// `None` in `App::mock()`; otherwise the open repository.
    repo: Option<git::Repo>,
    /// Repository directory name, shown in the status header (`ferrit -> main`).
    repo_name: String,
    header: git::StatusHeader,
    files: Vec<git::FileEntry>,
    branches: Vec<git::BranchEntry>,
    commits: Vec<git::CommitEntry>,
    stashes: Vec<git::StashEntry>,
    /// Last `refresh()` failure, shown in the Status pane. Never a panic.
    last_error: Option<String>,

    /// Terminal graphics backend for the image preview. Starts on half-blocks
    /// (works everywhere); `detect_graphics()` upgrades it to sixel / kitty /
    /// iterm2 when the real terminal supports one.
    picker: Picker,
    /// Right-pane image preview for the current selection, rebuilt on nav.
    preview: Preview,
    /// Right-pane diff for the current selection, behind any image preview.
    /// Rebuilt on nav and on background `Refresh`.
    diff: DiffView,
    /// What `diff` currently describes. `None` when no diff applies.
    right_key: Option<RightKey>,
    /// First visible line of the right-pane diff. Kept across a `Refresh` of
    /// an unchanged selection; reset to 0 when the selection changes.
    right_scroll: usize,
    /// Inner height of the right-pane diff box, written by `ui::draw_right_pane`
    /// each frame. Drives the viewport-aware scroll clamp and the page steps.
    /// 0 before the first draw: the clamp is then permissive by one screen and
    /// the next frame corrects it.
    right_viewport: usize,
    /// Whole right-pane rect from the last frame, for routing the mouse wheel
    /// to the diff (over the right column) or the selection (over the left).
    right_area: Rect,
    /// Each left pane's bordered rect from the last frame, for routing a
    /// click to the pane it landed in. `Rect::ZERO` before the first draw.
    left_areas: EnumMap<Pane, Rect>,
    /// `ListState::offset` for each left pane, copied back by
    /// `ui::draw_left_column` after `render_stateful_widget` moves it to
    /// keep the selection on screen. Lets a click in a scrolled list map to
    /// the right row. Only valid post-render; 0 before the first draw.
    list_offset: EnumMap<Pane, usize>,
    /// A click landed on the right pane. Purely a border-highlight flag for
    /// now (see `docs/PLAN_5_CLICK_BEHAVIOR.md`, "right-pane-focus plan");
    /// left-pane navigation and selection are untouched. Cleared by `Esc` or
    /// a click back on a left pane.
    right_focused: bool,
}

impl App {
    fn base(repo: Option<git::Repo>) -> Self {
        let repo_name = repo
            .as_ref()
            .map_or_else(|| "ferrit".to_owned(), git::Repo::name);
        Self {
            focus: Pane::default(),
            selection: EnumMap::default(),
            show_help: false,
            should_quit: false,
            repo,
            repo_name,
            header: git::StatusHeader::default(),
            files: Vec::new(),
            branches: Vec::new(),
            commits: Vec::new(),
            stashes: Vec::new(),
            last_error: None,
            picker: Picker::halfblocks(),
            preview: Preview::None,
            diff: DiffView::None,
            right_key: None,
            right_scroll: 0,
            right_viewport: 0,
            right_area: Rect::ZERO,
            left_areas: EnumMap::default(),
            list_offset: EnumMap::default(),
            right_focused: false,
        }
    }

    /// Open the repo at or above `path`, then take one snapshot.
    pub fn open(path: &Path) -> GitResult<Self> {
        let mut app = Self::base(Some(git::Repo::open(path)?));
        app.refresh();
        Ok(app)
    }

    /// Repo-free instance backed by `mock` data, for the render tests.
    pub fn mock() -> Self {
        let mut app = Self::base(None);
        app.header = mock::mock_header();
        app.files = mock::mock_files();
        app.branches = mock::mock_branches();
        app.commits = mock::mock_commits();
        app.stashes = mock::mock_stashes();
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
                self.last_error = Some(line);
            }
            self.picker = found.picker;
            self.update_right_pane();
        }
    }

    /// Re-read the wired panes. On error keep the old snapshot and stash the
    /// message; never propagate, never panic. No-op without a repo.
    pub fn refresh(&mut self) {
        let Some(repo) = &mut self.repo else { return };
        match repo.snapshot() {
            Ok(snap) => {
                self.header = snap.header;
                self.files = snap.files;
                self.branches = snap.branches;
                self.commits = snap.commits;
                self.stashes = snap.stashes;
                self.last_error = None;
            },
            Err(e) => self.last_error = Some(e.to_string()),
        }
        for pane in PANES {
            let last = self.row_count(pane).saturating_sub(1);
            let cursor = &mut self.selection[pane];
            *cursor = (*cursor).min(last);
        }
        self.update_right_pane();
    }

    /// Rebuild both cached right-pane values (`preview`, then `diff`) for the
    /// current focus and selection. Cheap when nothing changed.
    fn update_right_pane(&mut self) {
        self.preview = self.build_preview();
        self.update_diff();
    }

    /// Rebuild `diff` from the current focus and selection. An image selection
    /// owns the right pane, so it clears the diff. A `Refresh` of an unchanged
    /// selection rebuilds the text but keeps `right_scroll`; a changed
    /// selection resets the scroll to the top.
    fn update_diff(&mut self) {
        if matches!(self.preview, Preview::Image(_)) {
            self.diff = DiffView::None;
            self.right_key = None;
            return;
        }
        match self.right_key_for() {
            None => {
                self.diff = DiffView::None;
                self.right_key = None;
                self.right_scroll = 0;
            },
            Some(key) if self.right_key.as_ref() == Some(&key) => {
                let rebuilt = self.build_diff(&key);
                if view_sig(&rebuilt) != view_sig(&self.diff) {
                    self.diff = rebuilt;
                }
                self.clamp_right_scroll();
            },
            Some(key) => {
                self.right_scroll = 0;
                self.diff = self.build_diff(&key);
                self.right_key = Some(key);
            },
        }
    }

    /// The diff identity for the current focus and selection: a worktree /
    /// staged file for Files, a commit for Commits, nothing elsewhere.
    fn right_key_for(&self) -> Option<RightKey> {
        match self.focus {
            Pane::Files => {
                let entry = self.files.get(self.selected(Pane::Files))?;
                let side = if entry.worktree == git::Change::None {
                    DiffSide::Staged
                } else {
                    DiffSide::Worktree
                };
                Some(RightKey::File {
                    path: entry.path.clone(),
                    side,
                })
            },
            Pane::Commits => {
                let entry = self.commits.get(self.selected(Pane::Commits))?;
                Some(RightKey::Commit {
                    full_hash: entry.full_hash.clone(),
                })
            },
            _ => None,
        }
    }

    /// Run the diff subprocess for `key`. No repo (mock) means no diff, so the
    /// mock sample text keeps showing. An empty result or an error becomes a
    /// dim one-line `Note`, never a panic.
    fn build_diff(&self, key: &RightKey) -> DiffView {
        let Some(repo) = &self.repo else {
            return DiffView::None;
        };
        let opts = DiffOpts::default();
        match key {
            RightKey::File { path, side } => match repo.file_diff(path, *side, opts) {
                Ok(diff) if diff.files.is_empty() => DiffView::Note("no changes to show".into()),
                Ok(diff) => DiffView::Files(diff),
                Err(e) => DiffView::Note(e.to_string()),
            },
            RightKey::Commit { full_hash } => match repo.commit_diff(full_hash, opts) {
                Ok(diff) => match self.commits.iter().find(|c| &c.full_hash == full_hash) {
                    Some(entry) => DiffView::Commit(entry.clone(), diff),
                    None => DiffView::Note("commit not in the list".into()),
                },
                Err(e) => DiffView::Note(e.to_string()),
            },
        }
    }

    /// Line count of the current diff text, 0 for `None` / `Note`.
    fn diff_line_count(&self) -> usize {
        match &self.diff {
            DiffView::Files(d) | DiffView::Commit(_, d) => d.text.lines().count(),
            DiffView::None | DiffView::Note(_) => 0,
        }
    }

    /// Largest first-visible line that still fills the viewport: the last diff
    /// line lands at the bottom of the pane, never above it. Falls back to
    /// "line count minus one screen" until the first draw sets a real height.
    fn max_right_scroll(&self) -> usize {
        self.diff_line_count()
            .saturating_sub(self.right_viewport.max(1))
    }

    /// Clamp `right_scroll` into `0..=max_right_scroll()`.
    fn clamp_right_scroll(&mut self) {
        self.right_scroll = self.right_scroll.min(self.max_right_scroll());
    }

    /// Move the right-pane viewport by `delta` lines, clamped so it stops with
    /// the last line at the bottom of the pane. `isize::MIN` / `isize::MAX`
    /// snap to the top / bottom.
    fn scroll_right(&mut self, delta: isize) {
        let mag = delta.unsigned_abs();
        self.right_scroll = if delta >= 0 {
            self.right_scroll
                .saturating_add(mag)
                .min(self.max_right_scroll())
        } else {
            self.right_scroll.saturating_sub(mag)
        };
    }

    /// Is the right pane a scrollable real diff right now? The scroll keys and
    /// the wheel are inert over an image, a `Note`, and the mock bodies.
    fn right_is_diff(&self) -> bool {
        matches!(self.diff, DiffView::Files(_) | DiffView::Commit(..))
    }

    /// Jump `right_scroll` to the next (`dir > 0`) or previous hunk / file
    /// header, lazygit's `]` / `[`. Hunk headers for a file diff, `diff --git`
    /// headers for a commit diff.
    fn jump_diff_anchor(&mut self, dir: isize) {
        let anchors = match &self.diff {
            DiffView::Files(d) => d.hunk_lines(),
            DiffView::Commit(_, d) => d.file_lines(),
            DiffView::None | DiffView::Note(_) => return,
        };
        let cur = self.right_scroll;
        let target = if dir > 0 {
            anchors.iter().find(|&&l| l > cur).copied()
        } else {
            anchors.iter().rev().find(|&&l| l < cur).copied()
        };
        if let Some(line) = target {
            self.right_scroll = line;
            self.clamp_right_scroll();
        }
    }

    /// Current right-pane diff, for `ui::draw_right_pane`.
    pub fn diff_view(&self) -> &DiffView {
        &self.diff
    }

    /// First visible line of the right-pane diff.
    pub fn right_scroll(&self) -> usize {
        self.right_scroll
    }

    /// Set the right-pane scroll. Test and example helper.
    pub fn set_right_scroll(&mut self, line: usize) {
        self.right_scroll = line;
        self.clamp_right_scroll();
    }

    /// Inner height of the right-pane diff box, written by `ui::draw_right_pane`
    /// each frame so the scroll clamp and page steps track the real size.
    pub fn set_right_viewport(&mut self, rows: usize) {
        self.right_viewport = rows;
        self.clamp_right_scroll();
    }

    /// Whole right-pane rect, written by `ui::draw_right_pane` each frame so a
    /// mouse-wheel event can be routed by its column.
    pub fn set_right_area(&mut self, area: Rect) {
        self.right_area = area;
    }

    /// A left pane's bordered rect, written by `ui::draw_left_column` each
    /// frame so a click can be routed to the pane it landed in.
    pub fn set_left_area(&mut self, pane: Pane, area: Rect) {
        self.left_areas[pane] = area;
    }

    /// Whether the right pane was last clicked, for `ui::draw_right_pane`'s
    /// border highlight.
    pub fn right_focused(&self) -> bool {
        self.right_focused
    }

    /// A left pane's list scroll offset, read by `ui::draw_left_column`
    /// before it builds that pane's `ListState`.
    pub fn list_offset(&self, pane: Pane) -> usize {
        self.list_offset[pane]
    }

    /// A left pane's list scroll offset, written by `ui::draw_left_column`
    /// after `render_stateful_widget` so a click in a scrolled list maps to
    /// the right row.
    pub fn set_list_offset(&mut self, pane: Pane, offset: usize) {
        self.list_offset[pane] = offset;
    }

    /// Feed one key to the handler. Integration-test seam; the running app
    /// calls `on_key` from `run`.
    #[doc(hidden)]
    pub fn feed_key(&mut self, key: KeyEvent) {
        self.on_key(key);
    }

    /// Feed one mouse event to the handler. Integration-test seam.
    #[doc(hidden)]
    pub fn feed_mouse(&mut self, ev: MouseEvent) {
        self.on_mouse(ev);
    }

    /// Is the right pane currently a native-graphics image? `run` watches this
    /// across frames: when it flips back to `false` the sixel / iTerm2 / kitty
    /// pixels of the old frame outlive a normal buffer diff and need a full
    /// `terminal.clear()`.
    fn preview_is_image(&self) -> bool {
        matches!(self.preview, Preview::Image(_))
    }

    fn build_preview(&self) -> Preview {
        if self.focus != Pane::Files {
            return Preview::None;
        }
        let Some(entry) = self.files.get(self.selected(Pane::Files)) else {
            return Preview::None;
        };
        if !preview::is_image_path(&entry.path) {
            return Preview::None;
        }
        let bytes = match &self.repo {
            Some(repo) => match repo.blob_bytes(&entry.path, git::Rev::Workdir) {
                Ok(bytes) => bytes,
                Err(e) => {
                    return Preview::Note(format!("[image] {}  ({e})", entry.path.display()));
                },
            },
            None => mock::mock_image_bytes(&entry.path)
                .map(<[u8]>::to_vec)
                .unwrap_or_default(),
        };
        preview::from_bytes(&self.picker, &entry.path, &bytes)
    }

    /// The right-pane preview for the current selection.
    pub fn preview(&self) -> &Preview {
        &self.preview
    }

    /// Mutable preview, for `ui::draw`: `StatefulImage` resizes and re-encodes
    /// the protocol in place at render time (the ratatui-image example pattern).
    pub fn preview_mut(&mut self) -> &mut Preview {
        &mut self.preview
    }

    /// Focus `pane` and move its cursor to `index`, rebuilding the preview.
    /// Test and example helper; the running app goes through `on_key`.
    pub fn select(&mut self, pane: Pane, index: usize) {
        self.focus = pane;
        let last = self.row_count(pane).saturating_sub(1);
        self.selection[pane] = index.min(last);
        self.update_right_pane();
    }

    /// Selection cursor for a given pane.
    pub fn selected(&self, pane: Pane) -> usize {
        self.selection[pane]
    }

    /// Selectable row count for a pane, for clamping the cursor and deciding
    /// whether to draw a highlight.
    pub fn row_count(&self, pane: Pane) -> usize {
        match pane {
            Pane::Status => 0,
            Pane::Files => self.files.len(),
            Pane::Branches => self.branches.len(),
            Pane::Commits => self.commits.len(),
            Pane::Stash => self.stashes.len(),
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
        if let Some(err) = &self.last_error {
            return vec![theme::error_line(&format!("error: {err}"))];
        }
        let h = &self.header;
        let mut line = format!("{} \u{2192} {}", self.repo_name, h.branch);
        if h.ahead > 0 {
            let _ = write!(line, " \u{2191}{}", h.ahead);
        }
        if h.behind > 0 {
            let _ = write!(line, " \u{2193}{}", h.behind);
        }
        let mut out = vec![theme::status_line(&line)];
        if h.conflicts > 0 {
            out.push(theme::error_line(&format!(
                "\u{2717} {} merge conflict(s)",
                h.conflicts
            )));
        }
        out
    }

    /// Branches pane rows, or the empty-state line.
    pub fn branch_lines(&self) -> Vec<Line<'static>> {
        if self.branches.is_empty() {
            return vec![Line::raw("no local branches")];
        }
        self.branches.iter().map(theme::branch_line).collect()
    }

    /// Commits pane rows, or the empty-state line (fresh repo).
    pub fn commit_lines(&self) -> Vec<Line<'static>> {
        if self.commits.is_empty() {
            return vec![Line::raw("no commits yet")];
        }
        self.commits.iter().map(theme::commit_line).collect()
    }

    /// Stash pane rows, or the empty-state line.
    pub fn stash_lines(&self) -> Vec<Line<'static>> {
        if self.stashes.is_empty() {
            return vec![Line::raw("(no stash entries)")];
        }
        self.stashes.iter().map(theme::stash_line).collect()
    }

    /// Porcelain-style `XY path` text for one Files row. Debug/probe helper.
    pub fn file_display(&self, i: usize) -> String {
        self.files
            .get(i)
            .map(git::FileEntry::display)
            .unwrap_or_default()
    }

    /// Files pane rows, or a single "working tree clean" line.
    pub fn file_lines(&self) -> Vec<Line<'static>> {
        if self.files.is_empty() {
            return vec![Line::raw("working tree clean")];
        }
        self.files.iter().map(theme::file_line).collect()
    }

    /// Worktree root to hand the filesystem watcher, or `None` for a bare
    /// repo (and for `App::mock`, which has no repo).
    fn watch_root(&self) -> Option<PathBuf> {
        self.repo
            .as_ref()
            .and_then(git::Repo::workdir)
            .map(Path::to_path_buf)
    }

    /// Draw, then block for the next event, until `should_quit`. Events come
    /// from three sources multiplexed by `Events`: terminal input, a recursive
    /// filesystem watch on the worktree, and a 10s poll fallback. A change
    /// staged from another shell arrives as `AppEvent::Refresh`, so the panes
    /// track the repo the way lazygit's do.
    pub fn run(&mut self, terminal: &mut Tui) -> Result<()> {
        let events = Events::new(self.watch_root().as_deref())?;
        let mut prev_was_image = false;
        while !self.should_quit {
            let is_image = self.preview_is_image();
            if prev_was_image && !is_image {
                // Graphics pixels from the last image frame sit outside the
                // cell buffer; a full clear is the only way to wipe them.
                terminal.clear()?;
            }
            prev_was_image = is_image;
            terminal.draw(|frame| ui::draw(frame, self))?;

            match events.next()? {
                AppEvent::Input(Event::Key(key)) if key.kind == KeyEventKind::Press => {
                    self.on_key(key);
                },
                AppEvent::Input(Event::Mouse(m)) => self.on_mouse(m),
                AppEvent::Input(_) => {},
                AppEvent::Refresh => self.refresh(),
            }
        }
        Ok(())
    }

    fn on_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.should_quit = true;
            return;
        }

        if self.show_help {
            if matches!(key.code, KeyCode::Char('?' | 'q') | KeyCode::Esc) {
                self.show_help = false;
            }
            return;
        }

        // Right-pane diff scroll, lazygit's "scroll the main view without
        // leaving the side panel": J / K by a line, PageUp / PageDown by a
        // page, Ctrl-u / Ctrl-d by a half page, < / > to the ends, ] / [
        // between hunks (or files, for a commit). Steps come from the tracked
        // viewport height. None of these change the selection, so they skip
        // the `update_right_pane` rebuild and its diff subprocess. Inert unless
        // the right pane is a real diff.
        if self.right_is_diff() {
            let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
            let half = isize::try_from((self.right_viewport / 2).max(1)).unwrap_or(isize::MAX);
            let page =
                isize::try_from(self.right_viewport.saturating_sub(1).max(1)).unwrap_or(isize::MAX);
            match key.code {
                KeyCode::Char('d') if ctrl => return self.scroll_right(half),
                KeyCode::Char('u') if ctrl => return self.scroll_right(-half),
                KeyCode::Char('J') => return self.scroll_right(1),
                KeyCode::Char('K') => return self.scroll_right(-1),
                KeyCode::PageDown => return self.scroll_right(page),
                KeyCode::PageUp => return self.scroll_right(-page),
                KeyCode::Char('>') => return self.scroll_right(isize::MAX),
                KeyCode::Char('<') => return self.scroll_right(isize::MIN),
                KeyCode::Char(']') => return self.jump_diff_anchor(1),
                KeyCode::Char('[') => return self.jump_diff_anchor(-1),
                _ => {},
            }
        }

        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('?') => self.show_help = true,
            KeyCode::Esc => self.right_focused = false,
            KeyCode::Char('r') => self.refresh(),
            KeyCode::Char(c @ '1'..='5') => {
                if let Some(&pane) = PANES.get(c as usize - '1' as usize) {
                    self.focus = pane;
                }
            },
            KeyCode::Tab | KeyCode::Right => self.focus = self.pane_offset(1),
            KeyCode::BackTab | KeyCode::Left => self.focus = self.pane_offset(PANES.len() - 1),
            KeyCode::Char('j') | KeyCode::Down => self.select_down(),
            KeyCode::Char('k') | KeyCode::Up => self.select_up(),
            _ => {},
        }

        // Focus or selection may have moved; keep the right-pane preview in sync.
        self.update_right_pane();
    }

    /// A left click focuses the pane it lands in and, when it lands on a
    /// list row, moves that pane's selection cursor there too (lazygit's
    /// `HandleClick`, steps 3 / 4 / 5 / 7). Any click dismisses the help
    /// overlay first. Right click, middle click, drag and move are no-ops
    /// for now.
    fn on_mouse(&mut self, ev: MouseEvent) {
        match ev.kind {
            MouseEventKind::ScrollDown => return self.wheel(ev, 1),
            MouseEventKind::ScrollUp => return self.wheel(ev, -1),
            MouseEventKind::Down(MouseButton::Left) => {},
            MouseEventKind::Down(MouseButton::Right) => return, // phase 12: `x` context menu
            _ => return,                                        // middle click, drag, move
        }

        if self.show_help {
            self.show_help = false; // any click dismisses the overlay
            return;
        }

        if let Some(pane) = self.pane_at(ev.column, ev.row) {
            self.right_focused = false; // a left click always returns focus left
            self.click_pane(pane, ev.row);
            self.update_right_pane(); // step 7: rebuild for the new focus/selection
        } else if self.right_area.contains(Position::new(ev.column, ev.row)) {
            self.right_focused = true;
        }
        // else: command log / keybar / gap. no-op.
    }

    /// Which left pane a screen cell is in, `None` for the right pane, the
    /// command log, the keybar or an inter-pane gap.
    fn pane_at(&self, col: u16, row: u16) -> Option<Pane> {
        let point = Position::new(col, row);
        PANES
            .into_iter()
            .find(|&pane| self.left_areas[pane].contains(point))
    }

    /// Focus `pane`, then move its cursor to `screen_row` if that row maps
    /// to a real entry. Returns whether the cursor moved: `false` for the
    /// border / title row and for a click past the last entry.
    fn click_pane(&mut self, pane: Pane, screen_row: u16) -> bool {
        self.focus = pane; // focus first, even on the border or past the tail
        let Some(idx) = self.click_row(pane, screen_row) else {
            return false;
        };
        self.selection[pane] = idx;
        true
    }

    /// Screen row -> model index for a left pane. `None` for the border /
    /// title row, or a click past the last entry.
    fn click_row(&self, pane: Pane, screen_row: u16) -> Option<usize> {
        let area = self.left_areas[pane];
        let inner_row = screen_row.checked_sub(area.y.saturating_add(1))?;
        let idx = self.list_offset[pane].saturating_add(usize::from(inner_row));
        (idx < self.row_count(pane)).then_some(idx)
    }

    /// Mouse wheel over the right column scrolls the diff (lazygit's "wheel
    /// over the main view"); over the left column it nudges the focused
    /// pane's selection.
    fn wheel(&mut self, ev: MouseEvent, step: isize) {
        let a = self.right_area;
        let over_right = ev.column >= a.x && ev.column < a.x.saturating_add(a.width);
        if over_right && self.right_is_diff() {
            self.scroll_right(step * WHEEL_LINES);
            return;
        }
        if step > 0 {
            self.select_down();
        } else {
            self.select_up();
        }
        self.update_right_pane();
    }

    fn pane_offset(&self, delta: usize) -> Pane {
        let idx = (self.focus.index() + delta) % PANES.len();
        PANES.get(idx).copied().unwrap_or(self.focus)
    }

    fn select_down(&mut self) {
        let last = self.row_count(self.focus).saturating_sub(1);
        let cursor = &mut self.selection[self.focus];
        *cursor = (*cursor + 1).min(last);
    }

    fn select_up(&mut self) {
        let cursor = &mut self.selection[self.focus];
        *cursor = cursor.saturating_sub(1);
    }
}

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing,
    reason = "unit test: a failed setup or a bad index is the assertion"
)]
mod tests {
    use super::*;

    fn press(app: &mut App, code: KeyCode) {
        app.on_key(KeyEvent::from(code));
    }

    #[test]
    fn arrows_cycle_panes_and_wrap() {
        let mut app = App::mock();
        press(&mut app, KeyCode::Right);
        assert_eq!(app.focus, Pane::Files);
        press(&mut app, KeyCode::Left);
        assert_eq!(app.focus, Pane::Status);
        press(&mut app, KeyCode::Left);
        assert_eq!(app.focus, Pane::Stash, "Left from the first pane wraps");
    }

    #[test]
    fn selection_clamps_at_both_ends() {
        let mut app = App::mock();
        let last = mock::mock_files().len() - 1;
        press(&mut app, KeyCode::Char('2')); // focus Files
        for _ in 0..20 {
            press(&mut app, KeyCode::Down);
        }
        assert_eq!(app.selected(Pane::Files), last);
        for _ in 0..20 {
            press(&mut app, KeyCode::Up);
        }
        assert_eq!(app.selected(Pane::Files), 0);
    }

    #[test]
    fn image_selection_builds_an_image_preview() {
        let mut app = App::mock();
        let png = mock::mock_files()
            .iter()
            .position(|f| f.path.extension().is_some_and(|e| e == "png"))
            .expect("mock has a .png entry");

        press(&mut app, KeyCode::Char('2')); // focus Files
        assert!(
            matches!(app.preview(), Preview::None),
            "src/main.rs is not an image"
        );

        for _ in 0..png {
            press(&mut app, KeyCode::Down);
        }
        assert!(
            matches!(app.preview(), Preview::Image(_)),
            "the embedded PNG decodes on the half-block picker"
        );

        press(&mut app, KeyCode::Char('1')); // leave Files
        assert!(matches!(app.preview(), Preview::None));
    }

    #[test]
    fn help_overlay_swallows_navigation() {
        let mut app = App::mock();
        press(&mut app, KeyCode::Char('?'));
        assert!(app.show_help);
        press(&mut app, KeyCode::Right);
        assert_eq!(app.focus, Pane::Status, "nav is inert while help is up");
        press(&mut app, KeyCode::Char('?'));
        assert!(!app.show_help);
    }

    #[test]
    fn refresh_without_repo_is_a_noop() {
        let mut app = App::mock();
        let before = app.file_lines().len();
        app.refresh();
        assert_eq!(app.file_lines().len(), before);
        assert_eq!(before, mock::mock_files().len());
    }
}
