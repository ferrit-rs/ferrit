//! Application state and the draw / event loop.
//!
//! Phase 2 wired every left pane (Status, Files, Branches, Commits, Stash) to
//! a real read-only `git::Repo`. `App` owns the repo handle, the cached
//! snapshot, which left pane is focused, and one selection cursor per pane.
//! `App::mock()` is the repo-free path the render tests use.

use std::collections::{BTreeMap, HashSet};
use std::fmt::Write as _;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;

use color_eyre::Result;
use enum_map::{Enum, EnumMap};
use ratatui::crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::{Position, Rect};
use ratatui::text::{Line, Text};
use ratatui_image::picker::Picker;

use crate::events::{self, AppEvent, Events};
use crate::git::{self, ApplyDir, ApplyTarget, DiffOpts, DiffSide, GitResult};
use crate::image::detect;
use crate::image::preview::{self, Preview};
use crate::tui::Tui;
use crate::{mock, theme, ui};

/// What the right pane shows behind the image preview. A second cached,
/// rebuilt-on-nav value alongside `preview`, not a replacement: an image
/// selection still wins. See `docs/PLAN_3_DIFF_VIEW.md`.
#[derive(Debug, Clone, Default)]
pub enum DiffView {
    /// Status / Stash focused: no real diff, the mock text shows.
    #[default]
    None,
    /// Read failed, or nothing to show. A dim single line, never a panic.
    Note(String),
    /// Files pane: one file's `git diff`, both sides at once (lazygit's own
    /// Unstaged Changes / Staged Changes split).
    Files(FilesDiff),
    /// Commits pane, or a drilled branch's log: one commit's metadata and
    /// `git show` diff.
    Commit(git::CommitEntry, git::Diff),
    /// Branches pane, not drilled in: the selected branch's own log, shown
    /// passively (no Enter needed), lazygit's live branch -> log preview.
    BranchLog(BranchLog),
}

/// A Files-pane selection's two sides at once, lazygit's own Unstaged
/// Changes / Staged Changes split: a file half-staged shows real content in
/// both, a file entirely on one side shows an empty diff on the other.
#[derive(Debug, Clone)]
pub struct FilesDiff {
    pub unstaged: git::Diff,
    pub staged: git::Diff,
}

/// Read-only view of a single-`TextBuffer` popup for `ui::draw_commit_popup`
/// (`docs/PLAN_7_COMMIT.md`), reused as-is for the new-branch popup
/// (`docs/PLAN_8_BRANCHES.md`) — same shape, different title/footer.
/// Borrows the draft's lines, so it is cheap to build fresh every frame
/// rather than cached.
pub struct CommitPopupView<'a> {
    pub title: &'static str,
    pub lines: &'a [String],
    /// `(row, char column)`, `TextBuffer`'s own cursor coordinates.
    pub cursor: (usize, usize),
    /// `Some((sign_off, no_verify))` for the commit popup's toggle line;
    /// `None` for the new-branch popup, which has nothing to toggle.
    pub toggles: Option<(bool, bool)>,
    /// Footer key hints, e.g. `"Commit: Ctrl-S | ... | Cancel: Esc"`.
    pub hints: &'static str,
}

/// A branch's own commit log for the passive `DiffView::BranchLog` preview.
#[derive(Debug, Clone)]
pub struct BranchLog {
    pub branch: String,
    pub commits: Vec<git::CommitEntry>,
    /// Cheap content signature for `view_sig`'s "did this actually change"
    /// check: there is no single diff `text` to compare here.
    sig: String,
}

/// Identity of what `DiffView` describes. `update_right_pane` resets the scroll
/// only when this changes, so a background refresh of an unchanged selection
/// keeps its viewport.
#[derive(Debug, Clone, PartialEq, Eq)]
enum RightKey {
    File { path: PathBuf },
    Commit { full_hash: String },
    BranchLog { branch: String },
}

struct RenderedDiff {
    key: Option<RightKey>,
    source: String,
    focus: Option<Range<usize>>,
    width: usize,
    text: Text<'static>,
}

/// State for the Branches pane's Enter-to-drill-down (lazygit's branch ->
/// log): the pane itself swaps its branch list for one branch's commit list,
/// in place, rather than moving focus elsewhere. Distinct from the passive
/// `DiffView::BranchLog` preview, which needs no Enter at all.
struct BranchDrill {
    branch: String,
    commits: Vec<git::CommitEntry>,
    /// The branch-list cursor to restore when `Esc` backs out.
    return_index: usize,
}

/// Where keystrokes go while a Files diff is up. `Nav` is phase 1..5
/// behaviour unchanged; `Diff` is `docs/PLAN_6_STAGING.md`'s "focus the diff
/// to stage within it", scoped to the Files pane — the only one with
/// anything to stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
enum Mode {
    #[default]
    Nav,
    Diff,
}

/// The right-pane diff cursor, meaningful only in `Mode::Diff`. `line` and
/// `anchor` are indices into `side`'s own `Diff::text` lines: the Files
/// split always shows at most one file per side, so there is no "flatten
/// every file's hunks" step, just the diff's own line numbering.
#[derive(Debug, Clone, Default)]
struct DiffCursor {
    side: DiffSide,
    line: usize,
    /// V-select anchor. `None` is a single line, `Some(a)` is the range
    /// `a..=line` (order-independent: whichever end moves).
    anchor: Option<usize>,
    /// Content hash of the hunk the cursor sits in (header + body text), so
    /// a background refresh can re-find the same hunk even if surrounding
    /// hunks changed line count. gitu hashes the same way for its `Item.id`.
    hunk_id: u64,
}

/// What `<space>` / `d` act on in `Mode::Diff`: the whole hunk under the
/// cursor, or a V-selected subset of its `+`/`-` lines.
enum Granule {
    Hunk {
        patch: String,
    },
    Lines {
        file_header: String,
        hunk_header: String,
        hunk_body: String,
        lines: Vec<usize>,
    },
}

/// One hunk's body as global (whole-`Diff::text`) line indices, plus which
/// of those lines are selectable (`+`/`-`; context is read but never
/// chosen). Built fresh per diff-mode operation from the current `Diff` —
/// cheap at working-tree sizes, the same "no cache" choice `files_tree_rows`
/// already makes.
struct HunkLines {
    hunk_index: usize,
    lines: Range<usize>,
    selectable: Vec<usize>,
}

/// A pending confirmation: a `d` discard (phase 6) or a branch delete
/// (`docs/PLAN_8_BRANCHES.md`), the first *other* thing that needed a
/// yes/no gate — generalized from phase 6's `DiscardPrompt`, which was
/// exactly this shape with `action` fixed to a discard. `PLAN_0_GENERAL.md`:
/// "anything that loses work asks first". `y` runs `action`, `n` / `Esc`
/// cancels; nothing else can happen while it is up, same as the help
/// overlay.
struct ConfirmPrompt {
    message: String,
    action: ConfirmAction,
}

enum ConfirmAction {
    /// The whole file's worktree change (`d` in `Mode::Nav`, Files focused).
    DiscardFile(PathBuf),
    /// A hunk or a line selection (`d` in `Mode::Diff`, worktree side).
    DiscardGranule(Granule),
    /// `d` in `Mode::Nav`, Branches focused: `git branch -d` / `-D`. `force`
    /// is `false` on the first confirm, `true` on the second one offered
    /// after an unmerged-branch refusal (`App::run_confirm`).
    DeleteBranch { name: String, force: bool },
}

/// Body-line ranges (global `diff.text` line indices) for every hunk of a
/// single-file `Diff`, plus which of those lines are selectable.
fn hunk_lines_for(diff: &git::Diff) -> Vec<HunkLines> {
    let Some(file) = diff.files.first() else {
        return Vec::new();
    };
    let lines: Vec<&str> = diff.text.lines().collect();
    let headers = diff.hunk_lines();
    file.hunks
        .iter()
        .enumerate()
        .map(|(hunk_index, hunk)| {
            let body_start = headers.get(hunk_index).map_or(0, |&l| l + 1);
            let body_len = diff
                .text
                .get(hunk.body.clone())
                .unwrap_or_default()
                .lines()
                .count();
            let range = body_start..body_start + body_len;
            let selectable = range
                .clone()
                .filter(|&l| {
                    matches!(
                        lines.get(l).and_then(|s| s.as_bytes().first()),
                        Some(b'+' | b'-')
                    )
                })
                .collect();
            HunkLines {
                hunk_index,
                lines: range,
                selectable,
            }
        })
        .collect()
}

/// Every selectable line across every hunk of `diff`, in order. `j` / `k` in
/// `Mode::Diff` step through this list, skipping context lines entirely.
fn selectable_lines(diff: &git::Diff) -> Vec<usize> {
    hunk_lines_for(diff)
        .into_iter()
        .flat_map(|hl| hl.selectable)
        .collect()
}

/// The content id of whichever hunk contains global line `line`, or `None`
/// if it falls outside every hunk (should not happen for a selectable
/// line). Used to keep `DiffCursor::hunk_id` pointing at the hunk the
/// cursor is actually on whenever it moves, so `resync_diff_cursor` (which
/// runs after *every* key, not just a stage) does not mistake "moved to a
/// different hunk" for "the old hunk vanished" and snap back to it.
fn hunk_id_at(diff: &git::Diff, line: usize) -> Option<u64> {
    let hl = hunk_lines_for(diff)
        .into_iter()
        .find(|hl| hl.lines.contains(&line))?;
    Some(hunk_content_id(diff, hl.hunk_index))
}

/// Stable id for hunk `hunk_index` of `diff`: a hash of its header + body
/// text, so a background refresh can re-find the same hunk even once
/// staging moved a *different* hunk out from under it (gitu's `Item.id`).
fn hunk_content_id(diff: &git::Diff, hunk_index: usize) -> u64 {
    use std::hash::{Hash as _, Hasher as _};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    if let Some(hunk) = diff.files.first().and_then(|f| f.hunks.get(hunk_index)) {
        diff.text
            .get(hunk.header.start..hunk.body.end)
            .unwrap_or_default()
            .hash(&mut hasher);
    }
    hasher.finish()
}

/// A hand-rolled multi-line text buffer for the commit-message popup.
/// Not `tui-textarea`: its only published version needs `ratatui = "0.29"`,
/// incompatible with the `0.30` in this tree (see the deviation note atop
/// `docs/PLAN_7_COMMIT.md`). Lines of text plus a `(row, char column)`
/// cursor — enough for a commit message: printable insert, backspace,
/// `Enter` for a new line, arrow movement. No wrapping, no selection, no
/// undo; the Goal section of that plan already scoped the message box to
/// exactly this.
#[derive(Debug, Clone)]
struct TextBuffer {
    lines: Vec<String>,
    row: usize,
    /// Character index into `lines[row]`, not a byte offset — UTF-8 safe
    /// insert/delete always look this up via `char_indices`.
    col: usize,
}

impl Default for TextBuffer {
    fn default() -> Self {
        Self {
            lines: vec![String::new()],
            row: 0,
            col: 0,
        }
    }
}

impl TextBuffer {
    /// Pre-fill from an existing message (Amend / Reword), cursor at the
    /// very end — the common place to keep typing from.
    fn from_text(text: &str) -> Self {
        let mut lines: Vec<String> = text.lines().map(str::to_owned).collect();
        if lines.is_empty() {
            lines.push(String::new());
        }
        let row = lines.len() - 1;
        let col = lines.get(row).map_or(0, |l| l.chars().count());
        Self { lines, row, col }
    }

    fn text(&self) -> String {
        self.lines.join("\n")
    }

    /// No subject line typed at all (only blank/whitespace lines) — `git
    /// commit` refuses this, and so does the popup (`do_commit`).
    fn is_blank(&self) -> bool {
        self.lines.iter().all(|l| l.trim().is_empty())
    }

    fn current_line(&self) -> &str {
        self.lines.get(self.row).map_or("", String::as_str)
    }

    /// Byte offset of `self.col` (a char count) within the current line.
    fn byte_col(&self) -> usize {
        self.current_line()
            .char_indices()
            .nth(self.col)
            .map_or_else(|| self.current_line().len(), |(b, _)| b)
    }

    fn insert_char(&mut self, c: char) {
        let byte = self.byte_col();
        if let Some(line) = self.lines.get_mut(self.row) {
            line.insert(byte, c);
            self.col += 1;
        }
    }

    fn insert_newline(&mut self) {
        let byte = self.byte_col();
        if let Some(line) = self.lines.get_mut(self.row) {
            let rest = line.split_off(byte);
            self.lines.insert(self.row + 1, rest);
        }
        self.row += 1;
        self.col = 0;
    }

    /// Delete the char behind the cursor, or merge with the previous line
    /// at column 0. A no-op at the very start of the buffer.
    fn backspace(&mut self) {
        if self.col > 0 {
            let end = self.byte_col();
            let Some(line) = self.lines.get_mut(self.row) else {
                return;
            };
            let start = line.char_indices().nth(self.col - 1).map_or(0, |(b, _)| b);
            line.replace_range(start..end, "");
            self.col -= 1;
        } else if self.row > 0 {
            let current = self.lines.remove(self.row);
            self.row -= 1;
            let prev_len = self.lines.get(self.row).map_or(0, |l| l.chars().count());
            if let Some(line) = self.lines.get_mut(self.row) {
                line.push_str(&current);
            }
            self.col = prev_len;
        }
    }

    fn move_left(&mut self) {
        if self.col > 0 {
            self.col -= 1;
        } else if self.row > 0 {
            self.row -= 1;
            self.col = self.current_line().chars().count();
        }
    }

    fn move_right(&mut self) {
        let len = self.current_line().chars().count();
        if self.col < len {
            self.col += 1;
        } else if self.row + 1 < self.lines.len() {
            self.row += 1;
            self.col = 0;
        }
    }

    fn move_up(&mut self) {
        if self.row > 0 {
            self.row -= 1;
            self.col = self.col.min(self.current_line().chars().count());
        }
    }

    fn move_down(&mut self) {
        if self.row + 1 < self.lines.len() {
            self.row += 1;
            self.col = self.col.min(self.current_line().chars().count());
        }
    }
}

/// Modal state that owns all input while it is up, the same idea as
/// `show_help` today but richer (`docs/PLAN_7_COMMIT.md`).
enum Popup {
    Commit(CommitDraft),
    /// New-branch name input (`docs/PLAN_8_BRANCHES.md`). `Enter` *submits*
    /// here, unlike the commit popup, where `Enter` inserts a newline —
    /// the only behavioural difference from reusing `TextBuffer` outright.
    NewBranch(TextBuffer),
    /// `P` with no upstream and 2+ remotes configured: pick which one to
    /// push (and set as upstream) to. `docs/PLAN_9_REMOTE.md`'s "No
    /// upstream" flow; the 0- and 1-remote cases short-circuit before a
    /// popup is ever needed.
    RemotePick(RemotePick),
    /// A dismissible message: a commit failure, "empty commit message", a
    /// branch-op failure, or a merge conflict.
    Note(String),
}

struct RemotePick {
    remotes: Vec<git::RemoteEntry>,
    selected: usize,
}

struct CommitDraft {
    text: TextBuffer,
    kind: git::CommitKind,
    sign_off: bool,
    no_verify: bool,
}

/// One visible row of the Files pane's directory tree (lazygit style).
/// `App::files_tree_rows` builds these fresh from `self.files` and
/// `self.collapsed_dirs` on every call — cheap at working-tree sizes, same
/// "no cache" choice `branch_lines`/`commit_lines` already make.
enum FileRow {
    /// A directory header, including the always-present root ("/", the
    /// repo's own worktree). `path` is empty for the root.
    Dir {
        path: PathBuf,
        name: String,
        depth: usize,
        expanded: bool,
    },
    /// A changed file. `index` into `App.files`.
    File { index: usize, depth: usize },
}

/// One level of the Files tree, name -> child. `BTreeMap` for free
/// alphabetical iteration (matches lazygit: siblings sorted by name,
/// directories and files interleaved, not directories-first).
enum TreeNode {
    Dir(BTreeMap<String, Self>),
    File(usize),
}

/// Group `files` by directory into a tree keyed by path component. A path
/// component that collides with an existing file entry (pathological: git
/// cannot really produce this) drops that one file rather than panicking.
fn build_file_tree(files: &[git::FileEntry]) -> BTreeMap<String, TreeNode> {
    let mut root: BTreeMap<String, TreeNode> = BTreeMap::new();
    'entries: for (index, entry) in files.iter().enumerate() {
        let mut components: Vec<_> = entry.path.components().collect();
        let Some(file_name) = components.pop() else {
            continue;
        };
        let mut dir = &mut root;
        for component in &components {
            let name = component.as_os_str().to_string_lossy().into_owned();
            let child = dir
                .entry(name)
                .or_insert_with(|| TreeNode::Dir(BTreeMap::new()));
            dir = match child {
                TreeNode::Dir(children) => children,
                TreeNode::File(_) => continue 'entries,
            };
        }
        let name = file_name.as_os_str().to_string_lossy().into_owned();
        dir.insert(name, TreeNode::File(index));
    }
    root
}

/// Depth-first flatten of `nodes` (a `build_file_tree` level) into visible
/// rows, skipping the children of any directory in `collapsed`.
fn flatten_file_tree(
    nodes: &BTreeMap<String, TreeNode>,
    dir_path: &Path,
    depth: usize,
    collapsed: &HashSet<PathBuf>,
    rows: &mut Vec<FileRow>,
) {
    for (name, node) in nodes {
        match node {
            TreeNode::Dir(children) => {
                let path = dir_path.join(name);
                let expanded = !collapsed.contains(&path);
                rows.push(FileRow::Dir {
                    path: path.clone(),
                    name: name.clone(),
                    depth,
                    expanded,
                });
                if expanded {
                    flatten_file_tree(children, &path, depth + 1, collapsed, rows);
                }
            },
            TreeNode::File(index) => rows.push(FileRow::File {
                index: *index,
                depth,
            }),
        }
    }
}

/// Mouse-wheel step for the right pane, in lines. Matches gitu's default
/// `mouse_scroll_lines`.
const WHEEL_LINES: isize = 3;

/// `(discriminant, diff text)` for cheap "did the right pane actually change"
/// checks: `String` equality on a few KB, no hashing.
fn view_sig(v: &DiffView) -> (u8, &str, &str) {
    match v {
        DiffView::None => (0, "", ""),
        DiffView::Note(m) => (1, m.as_str(), ""),
        DiffView::Files(f) => (2, f.unstaged.text.as_str(), f.staged.text.as_str()),
        DiffView::Commit(_, d) => (3, d.text.as_str(), ""),
        DiffView::BranchLog(log) => (4, log.sig.as_str(), ""),
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
            Self::Commits => " Patch ",
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
    /// Directories collapsed in the Files pane's tree view (`FileRow`,
    /// `files_tree_rows`). Empty means "everything expanded", lazygit's own
    /// default; paths persist across `refresh()`, only `Enter` on a
    /// directory row changes this.
    collapsed_dirs: HashSet<PathBuf>,
    branches: Vec<git::BranchEntry>,
    /// `Some` while the Branches pane is drilled into one branch's own log
    /// (Enter on a branch, `Esc` to back out); `None` shows the branch list.
    branch_drill: Option<BranchDrill>,
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
    /// Cached styled diff. Scroll changes only Paragraph offset, so it must
    /// not rerun syntax highlighting or rebuild every line.
    rendered_diff: Option<RenderedDiff>,
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
    /// Whether keys go to the left panes or to the Files diff cursor
    /// (`docs/PLAN_6_STAGING.md`).
    mode: Mode,
    /// The diff cursor, meaningful only while `mode == Mode::Diff`.
    cursor: DiffCursor,
    /// A discard or branch-delete confirmation waiting on `y` / `n` / `Esc`.
    pending_confirm: Option<ConfirmPrompt>,
    /// A commit popup or dismissible note; owns all input while `Some`
    /// (`docs/PLAN_7_COMMIT.md`).
    popup: Option<Popup>,
    /// The last commit popup's text, kept across an `Esc`-cancel so a
    /// mistyped keystroke never loses a paragraph. Cleared on a successful
    /// commit.
    commit_draft: Option<String>,
    /// `Some` while a background fetch/pull/push is running. `f`/`p`/`P`
    /// pressed again while `Some` are ignored outright — not queued —
    /// sidestepping two git processes racing over the same `index.lock`.
    /// See `docs/PLAN_9_REMOTE.md`.
    remote_busy: Option<events::RemoteOp>,
    /// A background fetch/pull/push's success line ("Fetched origin", "3
    /// commits pushed"), shown in the Status pane until the next remote op
    /// or the next `refresh()`. `last_error`'s sibling for the non-error
    /// case, not a repurposing of that one field with a colour flag.
    status_note: Option<String>,
    /// A handle onto `Events`' own channel, so `on_key`'s `f`/`p`/`P` can
    /// hand a background thread a way back onto it. `None` in `App::mock()`
    /// and right after `App::open` — only `run()` has an `Events` to ask
    /// for one, so it sets this once before its own loop starts; a
    /// `feed_key`-driven test with no `run()` leaves `f`/`p`/`P` inert
    /// unless it calls `start_remote_op` directly with its own channel.
    event_sender: Option<mpsc::Sender<AppEvent>>,
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
            collapsed_dirs: HashSet::new(),
            branches: Vec::new(),
            branch_drill: None,
            commits: Vec::new(),
            stashes: Vec::new(),
            last_error: None,
            picker: Picker::halfblocks(),
            preview: Preview::None,
            diff: DiffView::None,
            right_key: None,
            rendered_diff: None,
            right_scroll: 0,
            right_viewport: 0,
            right_area: Rect::ZERO,
            left_areas: EnumMap::default(),
            list_offset: EnumMap::default(),
            right_focused: false,
            mode: Mode::default(),
            cursor: DiffCursor::default(),
            pending_confirm: None,
            popup: None,
            commit_draft: None,
            remote_busy: None,
            status_note: None,
            event_sender: None,
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

        // A drilled branch log (Enter on Branches) stays live across a
        // background refresh instead of going stale; a branch that vanished
        // (deleted, renamed) backs out of the drill-down instead of erroring
        // the whole refresh.
        if let Some(branch) = self.branch_drill.as_ref().map(|d| d.branch.clone()) {
            match self.repo.as_ref().map(|r| r.branch_log(&branch)) {
                Some(Ok(commits)) => {
                    if let Some(drill) = &mut self.branch_drill {
                        drill.commits = commits;
                    }
                },
                _ => self.branch_drill = None,
            }
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
            self.mode = Mode::Nav;
            return;
        }
        match self.right_key_for() {
            None => {
                self.diff = DiffView::None;
                self.right_key = None;
                self.right_scroll = 0;
                self.mode = Mode::Nav;
            },
            Some(key) if self.right_key.as_ref() == Some(&key) => {
                let rebuilt = self.build_diff(&key);
                if view_sig(&rebuilt) != view_sig(&self.diff) {
                    self.diff = rebuilt;
                }
                self.clamp_right_scroll();
                self.resync_diff_cursor();
            },
            Some(key) => {
                self.right_scroll = 0;
                self.diff = self.build_diff(&key);
                self.right_key = Some(key);
                self.mode = Mode::Nav;
            },
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
        if self.mode != Mode::Diff {
            return;
        }
        let DiffView::Files(files) = &self.diff else {
            self.mode = Mode::Nav;
            return;
        };
        let diff = match self.cursor.side {
            DiffSide::Worktree => &files.unstaged,
            DiffSide::Staged => &files.staged,
        };
        let hunks = hunk_lines_for(diff);

        if let Some(hl) = hunks
            .iter()
            .find(|hl| hunk_content_id(diff, hl.hunk_index) == self.cursor.hunk_id)
        {
            if hl.selectable.contains(&self.cursor.line) {
                self.cursor.anchor = self.cursor.anchor.filter(|a| hl.lines.contains(a));
                self.ensure_cursor_visible();
                return;
            }
            if let Some(&line) = hl.selectable.first() {
                self.cursor.line = line;
                self.cursor.anchor = None;
                self.ensure_cursor_visible();
                return;
            }
        }

        if let Some(hl) = hunks.iter().find(|hl| !hl.selectable.is_empty())
            && let Some(&line) = hl.selectable.first()
        {
            self.cursor.line = line;
            self.cursor.anchor = None;
            self.cursor.hunk_id = hunk_content_id(diff, hl.hunk_index);
            self.ensure_cursor_visible();
        } else {
            self.mode = Mode::Nav;
        }
    }

    /// Files pane rows, lazygit-style directory tree: a flat list when every
    /// changed file sits directly at the repo root (nothing to nest — most
    /// working trees most of the time), otherwise grouped under directory
    /// header rows plus an always-present root ("/"). Built fresh from
    /// `self.files` and `self.collapsed_dirs` on every call; cheap at
    /// working-tree sizes, same choice `branch_lines`/`commit_lines` make.
    fn files_tree_rows(&self) -> Vec<FileRow> {
        let nested = self
            .files
            .iter()
            .any(|f| f.path.parent().is_some_and(|p| p != Path::new("")));
        if !nested {
            return (0..self.files.len())
                .map(|index| FileRow::File { index, depth: 0 })
                .collect();
        }

        let tree = build_file_tree(&self.files);
        let root_expanded = !self.collapsed_dirs.contains(Path::new(""));
        let mut rows = vec![FileRow::Dir {
            path: PathBuf::new(),
            name: "/".to_owned(),
            depth: 0,
            expanded: root_expanded,
        }];
        if root_expanded {
            flatten_file_tree(&tree, Path::new(""), 1, &self.collapsed_dirs, &mut rows);
        }
        rows
    }

    /// The diff identity for the current focus and selection: a worktree /
    /// staged file for Files, a commit for Commits, nothing elsewhere.
    fn right_key_for(&self) -> Option<RightKey> {
        match self.focus {
            Pane::Files => {
                let rows = self.files_tree_rows();
                let FileRow::File { index, .. } = rows.get(self.selected(Pane::Files))? else {
                    return None;
                };
                let entry = self.files.get(*index)?;
                Some(RightKey::File {
                    path: entry.path.clone(),
                })
            },
            Pane::Commits => {
                let entry = self.commits.get(self.selected(Pane::Commits))?;
                Some(RightKey::Commit {
                    full_hash: entry.full_hash.clone(),
                })
            },
            // Drilled: the selected row is a commit, same as Commits. Not
            // drilled: no Enter yet, so preview the selected branch's own
            // log passively (lazygit's live branch -> log, no key needed).
            Pane::Branches => {
                if let Some(drill) = &self.branch_drill {
                    let entry = drill.commits.get(self.selected(Pane::Branches))?;
                    Some(RightKey::Commit {
                        full_hash: entry.full_hash.clone(),
                    })
                } else {
                    let entry = self.branches.get(self.selected(Pane::Branches))?;
                    Some(RightKey::BranchLog {
                        branch: entry.name.clone(),
                    })
                }
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
            RightKey::File { path } => {
                match (
                    repo.file_diff(path, DiffSide::Worktree, opts),
                    repo.file_diff(path, DiffSide::Staged, opts),
                ) {
                    (Ok(unstaged), Ok(staged))
                        if unstaged.files.is_empty() && staged.files.is_empty() =>
                    {
                        DiffView::Note("no changes to show".into())
                    },
                    (Ok(unstaged), Ok(staged)) => DiffView::Files(FilesDiff { unstaged, staged }),
                    (Err(e), _) | (_, Err(e)) => DiffView::Note(e.to_string()),
                }
            },
            RightKey::Commit { full_hash } => match repo.commit_diff(full_hash, opts) {
                Ok(diff) => {
                    let drill_commits = self.branch_drill.iter().flat_map(|d| d.commits.iter());
                    match self
                        .commits
                        .iter()
                        .chain(drill_commits)
                        .find(|c| &c.full_hash == full_hash)
                    {
                        Some(entry) => DiffView::Commit(entry.clone(), diff),
                        None => DiffView::Note("commit not in the list".into()),
                    }
                },
                Err(e) => DiffView::Note(e.to_string()),
            },
            RightKey::BranchLog { branch } => match repo.branch_log(branch) {
                Ok(commits) => {
                    let sig = commits.iter().map(|c| c.full_hash.as_str()).collect();
                    DiffView::BranchLog(BranchLog {
                        branch: branch.clone(),
                        commits,
                        sig,
                    })
                },
                Err(e) => DiffView::Note(e.to_string()),
            },
        }
    }

    /// Line count of the current diff text, 0 for `None` / `Note`.
    fn diff_line_count(&self) -> usize {
        match &self.diff {
            // Both columns share one scroll; the taller sets how far it goes.
            DiffView::Files(f) => f
                .unstaged
                .text
                .lines()
                .count()
                .max(f.staged.text.lines().count()),
            DiffView::Commit(_, d) => d.text.lines().count(),
            DiffView::BranchLog(log) => log.commits.len() * theme::BRANCH_LOG_BLOCK_LINES,
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

    /// Is the right pane scrollable right now — a real diff, or a branch's
    /// log preview? The scroll keys and the wheel are inert over an image, a
    /// `Note`, and the mock bodies; without this, they leak through to the
    /// left pane's own selection instead (moving the wrong thing).
    fn right_is_diff(&self) -> bool {
        matches!(
            self.diff,
            DiffView::Files(_) | DiffView::Commit(..) | DiffView::BranchLog(_)
        )
    }

    /// Jump `right_scroll` to the next (`dir > 0`) or previous hunk / file
    /// header, lazygit's `]` / `[`. `diff --git` headers for a commit diff;
    /// a no-op on the Files split, which has two diffs and no single anchor
    /// list to jump through.
    fn jump_diff_anchor(&mut self, dir: isize) {
        let anchors = match &self.diff {
            DiffView::Commit(_, d) => d.file_lines(),
            DiffView::None | DiffView::Note(_) | DiffView::BranchLog(_) | DiffView::Files(_) => {
                return;
            },
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

    /// Return cached styled diff. Cache invalidates on selection, diff text,
    /// focus range, or pane width; pure scrolling reuses `Text`. Only a
    /// commit diff goes through this cache: it is keyed for one `Diff` at a
    /// time, and the Files split renders its two sides directly instead
    /// (`ui::draw_files_columns`).
    pub fn rendered_diff(
        &mut self,
        focus: Option<&Range<usize>>,
        width: usize,
    ) -> Option<(Text<'static>, usize, git::DiffStat)> {
        let key = &self.right_key;
        let cache = &mut self.rendered_diff;
        match &self.diff {
            DiffView::Commit(_, diff) => {
                let cache_hit = cache.as_ref().is_some_and(|cached| {
                    cached.key.as_ref() == key.as_ref()
                        && cached.source == diff.text
                        && cached.focus.as_ref() == focus
                        && cached.width == width
                });
                if !cache_hit {
                    let text = diff.delta_output(width).map_or_else(
                        || theme::render_diff(diff, focus, width),
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

    /// Give the app a way back onto a background remote op's completion
    /// channel, the same one `run()` gets from its own `Events`
    /// (`Events::sender`). Integration-test seam: lets a test drive
    /// `f`/`p`/`P` through `feed_key` and still observe the eventual
    /// `AppEvent::RemoteDone`, with no `run()` loop (and its real
    /// terminal) involved.
    #[doc(hidden)]
    pub fn set_event_sender(&mut self, sender: mpsc::Sender<AppEvent>) {
        self.event_sender = Some(sender);
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
        let rows = self.files_tree_rows();
        let Some(FileRow::File { index, .. }) = rows.get(self.selected(Pane::Files)) else {
            return Preview::None;
        };
        let Some(entry) = self.files.get(*index) else {
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
            Pane::Files => self.files_tree_rows().len(),
            Pane::Branches => self
                .branch_drill
                .as_ref()
                .map_or(self.branches.len(), |drill| drill.commits.len()),
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
        if let Some(label) = self.remote_busy_label() {
            out.push(theme::busy_line(label));
        } else if let Some(note) = &self.status_note {
            out.push(theme::status_line(note));
        }
        out
    }

    /// Branches pane rows: the branch list, or one branch's own commit log
    /// while drilled in (`branch_drill`, `enter_branch_log`), each with its
    /// own empty-state line.
    pub fn branch_lines(&self) -> Vec<Line<'static>> {
        if let Some(drill) = &self.branch_drill {
            if drill.commits.is_empty() {
                return vec![Line::raw("no commits yet")];
            }
            return drill.commits.iter().map(theme::commit_line).collect();
        }
        if self.branches.is_empty() {
            return vec![Line::raw("no local branches")];
        }
        self.branches.iter().map(theme::branch_line).collect()
    }

    /// `[3] Local branches - Remotes - Tags`, or `[3] Commits (<branch>)`
    /// while drilled into a branch's log (Enter on a branch, `Esc` to back
    /// out; see `enter_branch_log`).
    pub fn branches_title(&self) -> String {
        match &self.branch_drill {
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
        self.branch_drill.is_some()
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

    /// Porcelain-style `XY path` text for one Files tree row, or an empty
    /// string for a directory row. Debug/probe helper; keyed by the same
    /// row index `file_lines`/`row_count` use, not a flat index into
    /// `self.files`.
    pub fn file_display(&self, i: usize) -> String {
        match self.files_tree_rows().get(i) {
            Some(&FileRow::File { index, .. }) => self
                .files
                .get(index)
                .map(git::FileEntry::display)
                .unwrap_or_default(),
            _ => String::new(),
        }
    }

    /// Files pane rows, or a single "working tree clean" line: a flat list,
    /// or lazygit's directory tree once any changed file sits below the
    /// repo root (`files_tree_rows`).
    pub fn file_lines(&self) -> Vec<Line<'static>> {
        if self.files.is_empty() {
            return vec![Line::raw("working tree clean")];
        }
        self.files_tree_rows()
            .iter()
            .filter_map(|row| match row {
                FileRow::Dir {
                    name,
                    depth,
                    expanded,
                    ..
                } => Some(theme::dir_line(name, *depth, *expanded)),
                FileRow::File { index, depth } => self
                    .files
                    .get(*index)
                    .map(|entry| theme::file_line(entry, *depth)),
            })
            .collect()
    }

    /// Worktree root to hand the filesystem watcher, or `None` for a bare
    /// repo (and for `App::mock`, which has no repo).
    fn watch_root(&self) -> Option<PathBuf> {
        self.repo
            .as_ref()
            .and_then(git::Repo::workdir)
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
    fn repo_handle(&self) -> Option<git::Repo> {
        git::Repo::open(&self.watch_root()?).ok()
    }

    /// Draw, then block for the next event, until `should_quit`. Events come
    /// from three sources multiplexed by `Events`: terminal input, a recursive
    /// filesystem watch on the worktree, and a 10s poll fallback. A change
    /// staged from another shell arrives as `AppEvent::Refresh`, so the panes
    /// track the repo the way lazygit's do.
    pub fn run(&mut self, terminal: &mut Tui) -> Result<()> {
        let events = Events::new(self.watch_root().as_deref())?;
        // A background fetch/pull/push (`start_remote_op`) needs its own
        // way back onto this channel; only `run()` has an `Events` to ask
        // for one, so it hands `on_key` this clone rather than `on_key`
        // taking `&Events` directly (it is also called from `feed_key`,
        // which has none).
        self.event_sender = Some(events.sender());
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
                AppEvent::RemoteDone { op, message } => self.on_remote_done(op, message),
            }
        }
        Ok(())
    }

    /// `f` / `p`: fetch / pull. Global, not Branches-only — unlike phase
    /// 8's branch actions, these act on the repo and its current branch,
    /// not a selected row. A no-op with no `event_sender` set
    /// (`App::mock()`, or a test driving `on_key` without `run()`).
    fn trigger_remote_op(&mut self, op: events::RemoteOp) {
        let Some(sender) = self.event_sender.clone() else {
            return;
        };
        self.start_remote_op(op, None, sender);
    }

    /// `P`: push. Unlike `f`/`p`, push needs to know *before* running
    /// whether the current branch has an upstream at all (`self.header
    /// .upstream`, already read by phase 2 for the ahead/behind count) —
    /// `Repo::push`'s own `NoUpstream` detection exists as a defensive
    /// fallback, not the primary path, because ferrit already knows the
    /// answer without asking git. No upstream: 0 remotes is an immediate
    /// `last_error`, 1 remote pushes straight there with `-u`, 2+ opens
    /// `Popup::RemotePick` to choose one.
    fn push_current_branch(&mut self) {
        if self.popup.is_some() {
            return;
        }
        if self.header.upstream.is_some() {
            self.trigger_remote_op(events::RemoteOp::Push);
            return;
        }
        let Some(repo) = &self.repo else { return };
        match repo.remotes() {
            Ok(remotes) if remotes.is_empty() => {
                self.last_error = Some("no remote configured".to_owned());
            },
            Ok(mut remotes) if remotes.len() == 1 => {
                self.push_with_upstream(remotes.remove(0).name);
            },
            Ok(remotes) => {
                self.popup = Some(Popup::RemotePick(RemotePick {
                    remotes,
                    selected: 0,
                }));
            },
            Err(e) => self.last_error = Some(e.to_string()),
        }
    }

    /// `git push -u <remote> <branch>`: the 1-remote short-circuit and
    /// `Popup::RemotePick`'s `Enter` both land here.
    fn push_with_upstream(&mut self, remote: String) {
        let Some(sender) = self.event_sender.clone() else {
            return;
        };
        self.start_remote_op(events::RemoteOp::Push, Some(remote), sender);
    }

    /// Spawn `op` on its own thread, `sender` its way back onto the same
    /// channel `Events::next()` reads (`docs/PLAN_9_REMOTE.md`, "Approach
    /// part 2": network calls are the first slow ones in `git::`, and
    /// running one on this thread would freeze the whole UI). One at a
    /// time: a second call while one is already running is ignored
    /// outright, not queued — two git processes racing over the same
    /// `index.lock` is a real failure mode, not a hypothetical one. A
    /// no-op with no repo to reopen (`App::mock()`, a bare repo).
    /// `push_upstream` is only ever `Some` for `RemoteOp::Push`, from
    /// `push_with_upstream`; `f`/`p`/a plain `P` all pass `None`.
    ///
    /// Takes `sender` as a parameter rather than reading `self.event_sender`
    /// directly so a test can call this with its own channel, no `run()`
    /// (and its `Events`) required. `pub`, integration-test seam like
    /// `feed_key`: a test drives the resulting `AppEvent::RemoteDone`
    /// itself, into `on_remote_done`, with no `run()` loop to receive it.
    #[doc(hidden)]
    pub fn start_remote_op(
        &mut self,
        op: events::RemoteOp,
        push_upstream: Option<String>,
        sender: mpsc::Sender<AppEvent>,
    ) {
        if self.remote_busy.is_some() {
            return;
        }
        let Some(repo) = self.repo_handle() else {
            return;
        };
        self.remote_busy = Some(op);
        self.status_note = None;
        thread::spawn(move || {
            let result = match op {
                events::RemoteOp::Fetch => repo.fetch(None),
                events::RemoteOp::Pull => repo.pull(),
                events::RemoteOp::Push => repo.push(push_upstream.as_deref()),
            };
            let message = result.map_err(|e| e.to_string());
            let _ = sender.send(AppEvent::RemoteDone { op, message });
        });
    }

    /// `AppEvent::RemoteDone` arrived: clear the busy flag, show a success
    /// line or the failure, then refresh — ahead/behind, branches, commits
    /// and files may all have moved (`pull` can fast-forward or rebase
    /// local commits; `push` moves nothing local but the ahead count
    /// changes). `pub`: `App::run`'s own match arm calls this, and so does
    /// a test that drove `start_remote_op` with its own channel and has no
    /// `run()` loop to receive the result for it.
    pub fn on_remote_done(&mut self, _op: events::RemoteOp, message: Result<String, String>) {
        self.remote_busy = None;
        // `refresh()` first, not last: it sets `last_error` on its own
        // (`None` on a successful snapshot, `Some` on a failed one), and
        // the remote op's own message is the one that should have the
        // final word on what the Status pane shows — reversing the order
        // would let a routine post-op `refresh()` silently clear the
        // very failure line it is meant to report.
        self.refresh();
        match message {
            Ok(line) => {
                self.last_error = None;
                self.status_note = Some(line);
            },
            Err(line) => {
                self.status_note = None;
                self.last_error = Some(line);
            },
        }
    }

    /// A short label for the Status pane while a fetch/pull/push is in
    /// flight, or `None` when none is. Not a progress bar: ferrit has no
    /// way to know fetch/push percentages without parsing git's
    /// `--progress` stream, which is meant for a terminal's own
    /// carriage-return redraws, not structured data.
    pub fn remote_busy_label(&self) -> Option<&'static str> {
        match self.remote_busy? {
            events::RemoteOp::Fetch => Some("Fetching\u{2026}"),
            events::RemoteOp::Pull => Some("Pulling\u{2026}"),
            events::RemoteOp::Push => Some("Pushing\u{2026}"),
        }
    }

    fn on_key(&mut self, key: KeyEvent) {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            self.should_quit = true;
            return;
        }

        // A popup (commit message box, or a dismissible note) owns all
        // input while it is up, same idea as the help overlay below but
        // richer (`docs/PLAN_7_COMMIT.md`).
        if self.popup.is_some() {
            self.popup_key(key);
            return;
        }

        // A discard / branch-delete confirmation swallows every key but its
        // own answer, same as the help overlay below.
        if self.pending_confirm.is_some() {
            match key.code {
                KeyCode::Char('y') => self.run_confirm(),
                KeyCode::Char('n') | KeyCode::Esc => self.pending_confirm = None,
                _ => {},
            }
            self.update_right_pane();
            return;
        }

        if self.show_help {
            if matches!(key.code, KeyCode::Char('?' | 'q') | KeyCode::Esc) {
                self.show_help = false;
            }
            return;
        }

        // `Mode::Diff` keys (Files pane, cursor focused into the diff) take
        // priority; unhandled ones fall through to the ordinary scroll block
        // and the generic match below, same as `Mode::Nav`.
        if self.on_diff_key(key) {
            self.update_right_pane();
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
            KeyCode::Esc => {
                self.right_focused = false;
                if let Some(drill) = self.branch_drill.take() {
                    self.selection[Pane::Branches] = drill.return_index;
                }
            },
            KeyCode::Enter => {
                self.enter_branch_log();
                self.toggle_files_dir();
                self.enter_diff_mode();
            },
            KeyCode::Char('l') => self.enter_diff_mode(),
            KeyCode::Char(' ') if self.focus == Pane::Branches => self.checkout_selected_branch(),
            KeyCode::Char(' ') => self.stage_selected_file(),
            KeyCode::Char('a') => self.stage_all_files(),
            KeyCode::Char('n') if self.focus == Pane::Branches => self.open_new_branch_popup(),
            KeyCode::Char('u') if self.focus == Pane::Branches => {
                self.fast_forward_selected_branch();
            },
            KeyCode::Char('M') if self.focus == Pane::Branches => self.merge_selected_branch(),
            KeyCode::Char('d') if self.focus == Pane::Branches && self.mode == Mode::Nav => {
                self.delete_branch_prompt();
            },
            KeyCode::Char('d') => self.discard_prompt(),
            KeyCode::Char('c') => self.open_commit(git::CommitKind::Normal),
            KeyCode::Char('A') => self.open_commit(git::CommitKind::Amend),
            KeyCode::Char('w') => self.open_commit(git::CommitKind::Reword),
            KeyCode::Char('r') => self.refresh(),
            KeyCode::Char('f') => self.trigger_remote_op(events::RemoteOp::Fetch),
            KeyCode::Char('p') => self.trigger_remote_op(events::RemoteOp::Pull),
            KeyCode::Char('P') => self.push_current_branch(),
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
    /// `HandleClick`, steps 3 / 4 / 5 / 7); on a Files directory row, it
    /// also toggles it collapsed/expanded, same as `Enter`. Any click
    /// dismisses the help overlay first. Right click, middle click, drag
    /// and move are no-ops for now.
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
            self.mode = Mode::Nav; // a click is a Nav-mode gesture, not diff-cursor movement
            let landed = self.click_pane(pane, ev.row);
            // lazygit toggles a Files directory row on click, not just on
            // Enter — the whole row is the target, not just its arrow
            // glyph, same as it already is for plain selection.
            if landed && pane == Pane::Files {
                self.toggle_files_dir();
            }
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

    /// Enter on the Branches pane: lazygit's branch -> log drill-down. Swaps
    /// the pane's own branch list for the selected branch's commit history,
    /// in place — focus stays on Branches, only its rows and title change
    /// (`branches_title`). Read only, no checkout. `Esc` backs out (`on_key`).
    fn enter_branch_log(&mut self) {
        if self.focus != Pane::Branches || self.branch_drill.is_some() {
            return;
        }
        let Some(repo) = &self.repo else { return };
        let return_index = self.selected(Pane::Branches);
        let Some(branch) = self.branches.get(return_index) else {
            return;
        };
        let name = branch.name.clone();
        match repo.branch_log(&name) {
            Ok(commits) => {
                self.branch_drill = Some(BranchDrill {
                    branch: name,
                    commits,
                    return_index,
                });
                self.selection[Pane::Branches] = 0;
            },
            Err(e) => self.last_error = Some(e.to_string()),
        }
    }

    /// Enter on a directory row in the Files pane: toggle it collapsed or
    /// expanded (lazygit's tree). A no-op on a file row (`enter_diff_mode`
    /// handles that one instead).
    fn toggle_files_dir(&mut self) {
        if self.focus != Pane::Files {
            return;
        }
        let rows = self.files_tree_rows();
        let Some(FileRow::Dir { path, .. }) = rows.get(self.selected(Pane::Files)) else {
            return;
        };
        if !self.collapsed_dirs.remove(path) {
            self.collapsed_dirs.insert(path.clone());
        }
        let last = self.row_count(Pane::Files).saturating_sub(1);
        self.selection[Pane::Files] = self.selection[Pane::Files].min(last);
    }

    /// The `FileEntry` behind the Files pane's current selection, or `None`
    /// on a directory row or an empty pane.
    fn selected_file(&self) -> Option<&git::FileEntry> {
        let rows = self.files_tree_rows();
        let FileRow::File { index, .. } = rows.get(self.selected(Pane::Files))? else {
            return None;
        };
        self.files.get(*index)
    }

    /// The `Diff` the cursor currently lives in (`Mode::Diff`'s active
    /// side), or `None` off the Files pane / without a real Files split.
    fn cursor_diff(&self) -> Option<&git::Diff> {
        let DiffView::Files(files) = &self.diff else {
            return None;
        };
        Some(match self.cursor.side {
            DiffSide::Worktree => &files.unstaged,
            DiffSide::Staged => &files.staged,
        })
    }

    /// Scroll the shared Files-split viewport so `cursor.line` stays on
    /// screen, the same "a jump always lands visibly" rule phase 3's `]` /
    /// `[` already follows.
    fn ensure_cursor_visible(&mut self) {
        let viewport = self.right_viewport.max(1);
        if self.cursor.line < self.right_scroll {
            self.right_scroll = self.cursor.line;
        } else if self.cursor.line >= self.right_scroll + viewport {
            self.right_scroll = self.cursor.line + 1 - viewport;
        }
        self.clamp_right_scroll();
    }

    /// `Enter` / `l` on a Files-pane file row (`Mode::Nav`): focus the diff
    /// for staging within it. A no-op off the Files pane, on a directory
    /// row, already in `Mode::Diff`, or when neither side has a selectable
    /// line to put the cursor on (binary, a pure rename, no change at all) —
    /// those stage whole-file only, from `Mode::Nav`.
    fn enter_diff_mode(&mut self) {
        if self.focus != Pane::Files || self.mode == Mode::Diff {
            return;
        }
        let Some(entry) = self.selected_file() else {
            return;
        };
        let DiffView::Files(files) = &self.diff else {
            return;
        };
        // Same direction rule as the file-level toggle: worktree changes
        // lead, so a half-staged file's cursor starts where there's still
        // something to stage.
        let side = if entry.worktree == git::Change::None {
            DiffSide::Staged
        } else {
            DiffSide::Worktree
        };
        let diff = match side {
            DiffSide::Worktree => &files.unstaged,
            DiffSide::Staged => &files.staged,
        };
        let hunks = hunk_lines_for(diff);
        let Some(hunk) = hunks.iter().find(|hl| !hl.selectable.is_empty()) else {
            return;
        };
        let Some(&line) = hunk.selectable.first() else {
            return;
        };

        self.mode = Mode::Diff;
        self.cursor = DiffCursor {
            side,
            line,
            anchor: None,
            hunk_id: hunk_content_id(diff, hunk.hunk_index),
        };
        self.ensure_cursor_visible();
    }

    /// `Esc` / `h` in `Mode::Diff`: back to `Mode::Nav`.
    fn leave_diff_mode(&mut self) {
        self.mode = Mode::Nav;
    }

    /// `j` / `k` in `Mode::Diff`: move the line cursor over selectable
    /// lines only, `docs/PLAN_6_STAGING.md`'s "context lines are
    /// unselectable".
    fn move_diff_cursor(&mut self, dir: isize) {
        let Some(diff) = self.cursor_diff() else {
            return;
        };
        let lines = selectable_lines(diff);
        let Some(pos) = lines.iter().position(|&l| l == self.cursor.line) else {
            return;
        };
        let next = if dir > 0 {
            pos.saturating_add(1).min(lines.len().saturating_sub(1))
        } else {
            pos.saturating_sub(1)
        };
        let Some(&line) = lines.get(next) else {
            return;
        };
        // Crossing into a different hunk: re-tag `hunk_id` right away, or
        // the very next keystroke's `resync_diff_cursor` (which runs on
        // every key, not just a stage) reads the stale id, decides the old
        // hunk "lost" this line, and snaps the cursor straight back to it.
        let hunk_id = hunk_id_at(diff, line);
        self.cursor.line = line;
        if let Some(id) = hunk_id {
            self.cursor.hunk_id = id;
        }
        self.ensure_cursor_visible();
    }

    /// `V` in `Mode::Diff`: start or clear a line V-selection.
    fn toggle_diff_anchor(&mut self) {
        self.cursor.anchor = if self.cursor.anchor.is_some() {
            None
        } else {
            Some(self.cursor.line)
        };
    }

    /// `]` / `[` in `Mode::Diff`: move the cursor to the next / previous
    /// hunk's first selectable line. Unlike the `Mode::Nav` `]` / `[`
    /// (`jump_diff_anchor`), which scrolls a single commit diff and is a
    /// no-op on the Files split, this moves the cursor itself.
    fn jump_diff_cursor_hunk(&mut self, dir: isize) {
        let Some(diff) = self.cursor_diff() else {
            return;
        };
        let starts: Vec<usize> = hunk_lines_for(diff)
            .iter()
            .filter_map(|hl| hl.selectable.first().copied())
            .collect();
        let cur = self.cursor.line;
        let target = if dir > 0 {
            starts.iter().find(|&&l| l > cur).copied()
        } else {
            starts.iter().rev().find(|&&l| l < cur).copied()
        };
        if let Some(line) = target {
            let hunk_id = hunk_id_at(diff, line);
            self.cursor.line = line;
            if let Some(id) = hunk_id {
                self.cursor.hunk_id = id;
            }
            self.ensure_cursor_visible();
        }
    }

    /// What `<space>` / `d` would act on right now: the V-selected lines
    /// when there is a selection, else the whole hunk under the cursor
    /// (`docs/PLAN_6_STAGING.md` "Granule resolution", S1's simplified
    /// rule). `None` when the cursor's hunk cannot be found (should not
    /// happen while `Mode::Diff` is up) or a V-selection covers no `+`/`-`
    /// line (only context was under it — a no-op, not an empty patch).
    fn current_granule(&self) -> Option<Granule> {
        let diff = self.cursor_diff()?;
        let file = diff.files.first()?;
        let hunks = hunk_lines_for(diff);
        let hl = hunks
            .iter()
            .find(|hl| hl.lines.contains(&self.cursor.line))?;
        let hunk = file.hunks.get(hl.hunk_index)?;

        if let Some(anchor) = self.cursor.anchor {
            let lo = anchor.min(self.cursor.line);
            let hi = anchor.max(self.cursor.line);
            let lines: Vec<usize> = hl
                .selectable
                .iter()
                .filter(|&&l| (lo..=hi).contains(&l))
                .map(|&l| l - hl.lines.start)
                .collect();
            if lines.is_empty() {
                return None;
            }
            Some(Granule::Lines {
                file_header: diff.text.get(file.header.clone())?.to_owned(),
                hunk_header: diff.text.get(hunk.header.clone())?.to_owned(),
                hunk_body: diff.text.get(hunk.body.clone())?.to_owned(),
                lines,
            })
        } else {
            let patch = diff.text.get(file.header.start..hunk.body.end)?.to_owned();
            Some(Granule::Hunk { patch })
        }
    }

    /// Run a `Granule` through the matching backend call.
    fn apply_granule(
        &self,
        granule: &Granule,
        dir: ApplyDir,
        target: ApplyTarget,
    ) -> GitResult<()> {
        let Some(repo) = &self.repo else {
            return Ok(());
        };
        match granule {
            Granule::Hunk { patch } => repo.apply_hunk(patch, dir, target),
            Granule::Lines {
                file_header,
                hunk_header,
                hunk_body,
                lines,
            } => repo.apply_lines(file_header, hunk_header, hunk_body, lines, dir, target),
        }
    }

    /// Refresh after a stage / unstage / discard, then surface a failure in
    /// the Status pane. `git apply` is atomic per invocation, so a failure
    /// leaves the repository exactly as it was; the refresh still runs so a
    /// failed attempt (context drift from an external edit) re-reads the
    /// current diff for the retry (`docs/PLAN_6_STAGING.md` "apply fails").
    fn finish_apply(&mut self, result: GitResult<()>) {
        self.refresh();
        if let Err(e) = result {
            self.last_error = Some(e.to_string());
        }
    }

    /// `<space>` on a Files row (`Mode::Nav`): stage or unstage the whole
    /// file, direction inferred from which side has a change
    /// (`docs/PLAN_6_STAGING.md` "Stage vs unstage is one key").
    fn stage_selected_file(&mut self) {
        if self.focus != Pane::Files {
            return;
        }
        let Some(entry) = self.selected_file() else {
            return;
        };
        let dir = if entry.worktree != git::Change::None {
            ApplyDir::Forward
        } else if entry.staged != git::Change::None {
            ApplyDir::Reverse
        } else {
            return;
        };
        let path = entry.path.clone();
        let Some(repo) = &self.repo else {
            return;
        };
        let result = repo.stage_file(&path, dir);
        self.finish_apply(result);
    }

    /// `<space>` in `Mode::Diff`: stage/unstage the hunk under the cursor,
    /// or the V-selection when one is active.
    fn stage_diff_cursor(&mut self) {
        let Some(granule) = self.current_granule() else {
            return;
        };
        let dir = match self.cursor.side {
            DiffSide::Worktree => ApplyDir::Forward,
            DiffSide::Staged => ApplyDir::Reverse,
        };
        let result = self.apply_granule(&granule, dir, ApplyTarget::Index);
        self.cursor.anchor = None;
        self.finish_apply(result);
    }

    /// `a` (Nav, Files focused): stage every changed file if any is
    /// unstaged, else unstage everything — one `git` call either way
    /// (`docs/PLAN_6_STAGING.md` milestone S4).
    fn stage_all_files(&mut self) {
        if self.focus != Pane::Files {
            return;
        }
        let dir = if self.files.iter().any(|f| f.worktree != git::Change::None) {
            ApplyDir::Forward
        } else if self.files.iter().any(|f| f.staged != git::Change::None) {
            ApplyDir::Reverse
        } else {
            return;
        };
        let Some(repo) = &self.repo else {
            return;
        };
        let result = repo.stage_all(dir);
        self.finish_apply(result);
    }

    /// `d`: ask before discarding a worktree change, at the file granularity
    /// from `Mode::Nav` (Files focused) or at the hunk / line granularity
    /// under the cursor from `Mode::Diff`. Discard only ever touches the
    /// worktree (`docs/PLAN_6_STAGING.md`'s own scope), so it is a no-op on
    /// the Staged side and on a file with no worktree change of its own.
    fn discard_prompt(&mut self) {
        match self.mode {
            Mode::Nav if self.focus == Pane::Files => {
                let Some(entry) = self.selected_file() else {
                    return;
                };
                if entry.worktree == git::Change::None {
                    return;
                }
                self.pending_confirm = Some(ConfirmPrompt {
                    message: format!("discard all changes in {}?", entry.path.display()),
                    action: ConfirmAction::DiscardFile(entry.path.clone()),
                });
            },
            Mode::Diff if self.cursor.side == DiffSide::Worktree => {
                let Some(granule) = self.current_granule() else {
                    return;
                };
                let Some(entry) = self.selected_file() else {
                    return;
                };
                let what = match &granule {
                    Granule::Hunk { .. } => "this hunk".to_owned(),
                    Granule::Lines { lines, .. } => {
                        format!(
                            "{} line{}",
                            lines.len(),
                            if lines.len() == 1 { "" } else { "s" }
                        )
                    },
                };
                self.pending_confirm = Some(ConfirmPrompt {
                    message: format!("discard {what} in {}?", entry.path.display()),
                    action: ConfirmAction::DiscardGranule(granule),
                });
            },
            Mode::Nav | Mode::Diff => {},
        }
    }

    /// Refresh after a branch mutation (checkout / create / delete / fast-
    /// forward), then surface a failure in the Status pane. Same shape as
    /// `finish_apply`.
    fn finish_branch_action(&mut self, result: GitResult<()>) {
        self.refresh();
        if let Err(e) = result {
            self.last_error = Some(e.to_string());
        }
    }

    /// `<space>` on the Branches pane (`Mode::Nav`): checkout the selected
    /// branch. `refresh()` picks up the new `HEAD`, branches, and files (a
    /// checkout changes the working tree too). No-op while drilled into a
    /// branch's log, where the selected row is a commit, not a branch.
    fn checkout_selected_branch(&mut self) {
        if self.focus != Pane::Branches || self.branch_drill.is_some() {
            return;
        }
        let Some(entry) = self.branches.get(self.selected(Pane::Branches)) else {
            return;
        };
        let name = entry.name.clone();
        let Some(repo) = &self.repo else { return };
        let result = repo.checkout(&name);
        self.finish_branch_action(result);
    }

    /// `n` (Nav, Branches focused): open the new-branch popup, named from
    /// the current `HEAD` once submitted.
    fn open_new_branch_popup(&mut self) {
        if self.focus != Pane::Branches || self.popup.is_some() || self.branch_drill.is_some() {
            return;
        }
        self.popup = Some(Popup::NewBranch(TextBuffer::default()));
    }

    /// `Enter` in the new-branch popup: `git checkout -b <name>` from
    /// `HEAD`. Success closes the popup and refreshes; failure (a bad
    /// name, or one already taken) keeps the popup open with the typed
    /// text so the user can fix it and retry — the message surfaces in
    /// the Status pane rather than a second popup layered on this one.
    fn do_create_branch(&mut self) {
        let Some(Popup::NewBranch(buf)) = &self.popup else {
            return;
        };
        let name = buf.text();
        let Some(repo) = &self.repo else { return };
        match repo.create_branch(&name) {
            Ok(()) => {
                self.popup = None;
                self.refresh();
            },
            Err(e) => self.last_error = Some(e.to_string()),
        }
    }

    /// `d` (Nav, Branches focused): ask before deleting the selected
    /// branch. The currently checked-out branch skips the confirm
    /// entirely — `git` refuses to delete it either way, so its own
    /// message goes straight to `last_error`, the same "explain, do
    /// nothing" path an invalid discard already takes, rather than
    /// opening a confirm for an outcome that is already certain.
    fn delete_branch_prompt(&mut self) {
        if self.focus != Pane::Branches || self.branch_drill.is_some() {
            return;
        }
        let Some(entry) = self.branches.get(self.selected(Pane::Branches)) else {
            return;
        };
        let name = entry.name.clone();
        if entry.is_head {
            let Some(repo) = &self.repo else { return };
            let result = repo.delete_branch(&name, false);
            self.finish_branch_action(result);
            return;
        }
        self.pending_confirm = Some(ConfirmPrompt {
            message: format!("delete branch {name}?"),
            action: ConfirmAction::DeleteBranch { name, force: false },
        });
    }

    /// `u` (Nav, Branches focused): fast-forward the selected branch to
    /// its upstream, checked out or not (`Repo::fast_forward` picks the
    /// mechanism). No confirm: exactly as reversible as any other git
    /// command, the reflog has your back the same way it does from a
    /// shell.
    fn fast_forward_selected_branch(&mut self) {
        if self.focus != Pane::Branches || self.branch_drill.is_some() {
            return;
        }
        let Some(entry) = self.branches.get(self.selected(Pane::Branches)) else {
            return;
        };
        let name = entry.name.clone();
        let Some(repo) = &self.repo else { return };
        let result = repo.fast_forward(&name);
        self.finish_branch_action(result);
    }

    /// Every changed path currently reported as conflicted (staged or
    /// worktree side), for the merge-conflict note's message.
    fn conflicted_paths(&self) -> Vec<String> {
        self.files
            .iter()
            .filter(|f| {
                f.staged == git::Change::Conflicted || f.worktree == git::Change::Conflicted
            })
            .map(|f| f.path.display().to_string())
            .collect()
    }

    /// `M` (Nav, Branches focused): merge the selected branch into the
    /// current one. `refresh()` always runs, even on a conflict — the
    /// Files pane already renders `Change::Conflicted`, so the conflicted
    /// paths are visible without a dedicated flow.
    fn merge_selected_branch(&mut self) {
        if self.focus != Pane::Branches || self.branch_drill.is_some() {
            return;
        }
        let Some(entry) = self.branches.get(self.selected(Pane::Branches)) else {
            return;
        };
        let name = entry.name.clone();
        let Some(repo) = &self.repo else { return };
        let result = repo.merge_branch(&name);
        self.refresh();
        match result {
            Ok(git::MergeOutcome::Merged) => {},
            Ok(git::MergeOutcome::Conflicted) => {
                let files = self.conflicted_paths().join(", ");
                self.popup = Some(Popup::Note(format!(
                    "merge conflict in {files}. Resolve and commit, or `git merge --abort` \
                     from the shell — conflict resolution UI is phase 11."
                )));
            },
            Err(e) => self.last_error = Some(e.to_string()),
        }
    }

    /// `y` while a confirm prompt is up: run its action. A branch delete
    /// refused for being unmerged (`"is not fully merged"`, the same
    /// stable-substring technique `commit.rs`'s `NothingStaged` already
    /// uses) re-opens the confirm one more time asking to force it,
    /// rather than reporting the refusal and stopping — `git branch -d`
    /// is offering a choice, not failing outright.
    fn run_confirm(&mut self) {
        let Some(prompt) = self.pending_confirm.take() else {
            return;
        };
        match prompt.action {
            ConfirmAction::DiscardFile(path) => {
                let untracked = self
                    .files
                    .iter()
                    .find(|f| f.path == path)
                    .is_some_and(|f| f.worktree == git::Change::Untracked);
                let result = match &self.repo {
                    Some(repo) => repo.discard_file(&path, untracked),
                    None => return,
                };
                self.cursor.anchor = None;
                self.finish_apply(result);
            },
            ConfirmAction::DiscardGranule(granule) => {
                let result = self.apply_granule(&granule, ApplyDir::Reverse, ApplyTarget::Worktree);
                self.cursor.anchor = None;
                self.finish_apply(result);
            },
            ConfirmAction::DeleteBranch { name, force } => {
                let Some(repo) = &self.repo else { return };
                match repo.delete_branch(&name, force) {
                    Ok(()) => self.refresh(),
                    Err(git::GitError::BranchFailed(msg))
                        if !force && msg.contains("is not fully merged") =>
                    {
                        self.pending_confirm = Some(ConfirmPrompt {
                            message: format!(
                                "'{name}' is not fully merged. Force delete? This may lose \
                                 commits with no other reference to them."
                            ),
                            action: ConfirmAction::DeleteBranch { name, force: true },
                        });
                    },
                    Err(e) => self.last_error = Some(e.to_string()),
                }
            },
        }
    }

    /// The pending discard / branch-delete confirmation message, for the
    /// keybar prompt (`ui::draw_keybar`), or `None` when nothing is
    /// pending.
    pub fn confirm_message(&self) -> Option<&str> {
        self.pending_confirm.as_ref().map(|p| p.message.as_str())
    }

    /// The commit popup's render data (`ui::draw_commit_popup`), or `None`
    /// when it is not up.
    pub fn commit_popup(&self) -> Option<CommitPopupView<'_>> {
        let Some(Popup::Commit(draft)) = &self.popup else {
            return None;
        };
        Some(CommitPopupView {
            title: draft.kind.title(),
            lines: &draft.text.lines,
            cursor: (draft.text.row, draft.text.col),
            toggles: Some((draft.sign_off, draft.no_verify)),
            hints: "Commit: Ctrl-S | Sign-off: Ctrl-O | No-verify: Ctrl-N | Cancel: Esc",
        })
    }

    /// The new-branch popup's render data, reusing `ui::draw_commit_popup`'s
    /// shape (`docs/PLAN_8_BRANCHES.md`), or `None` when it is not up.
    pub fn new_branch_popup(&self) -> Option<CommitPopupView<'_>> {
        let Some(Popup::NewBranch(buf)) = &self.popup else {
            return None;
        };
        Some(CommitPopupView {
            title: "New branch",
            lines: &buf.lines,
            cursor: (buf.row, buf.col),
            toggles: None,
            hints: "Create: Enter | Cancel: Esc",
        })
    }

    /// A dismissible note's message (`ui::draw_note_popup`), or `None` when
    /// none is up.
    pub fn note_popup(&self) -> Option<&str> {
        match &self.popup {
            Some(Popup::Note(msg)) => Some(msg),
            _ => None,
        }
    }

    /// The remote-pick popup's remotes and highlighted index
    /// (`docs/PLAN_9_REMOTE.md`'s "No upstream" flow, 2+ remotes), or
    /// `None` when it is not up.
    pub fn remote_pick(&self) -> Option<(&[git::RemoteEntry], usize)> {
        match &self.popup {
            Some(Popup::RemotePick(pick)) => Some((&pick.remotes, pick.selected)),
            _ => None,
        }
    }

    /// Cursor state for the right-pane render: `(side, cursor line, V-select
    /// range)` while `Mode::Diff` is up, else `None`. The range is
    /// inclusive-exclusive (`a..b`) over `side`'s own `Diff::text` lines.
    pub fn diff_cursor(&self) -> Option<(DiffSide, usize, Option<Range<usize>>)> {
        if self.mode != Mode::Diff {
            return None;
        }
        let range = self
            .cursor
            .anchor
            .map(|a| a.min(self.cursor.line)..a.max(self.cursor.line) + 1);
        Some((self.cursor.side, self.cursor.line, range))
    }

    /// Right-pane title suffix while `Mode::Diff` is up: `hunk 1/3` or
    /// `lines 41-42`/`line 41`, so it is obvious what `<space>` will hit.
    pub fn diff_granule_hint(&self) -> Option<String> {
        if self.mode != Mode::Diff {
            return None;
        }
        let diff = self.cursor_diff()?;
        let hunks = hunk_lines_for(diff);
        let total = hunks.len();
        let current = hunks
            .iter()
            .position(|hl| hl.lines.contains(&self.cursor.line))?;
        Some(match self.cursor.anchor {
            Some(anchor) => {
                let lo = anchor.min(self.cursor.line) + 1;
                let hi = anchor.max(self.cursor.line) + 1;
                if lo == hi {
                    format!("line {lo}")
                } else {
                    format!("lines {lo}-{hi}")
                }
            },
            None => format!("hunk {}/{total}", current + 1),
        })
    }

    /// Keys meaningful only in `Mode::Diff`. Returns whether `key` was one
    /// of them, so `on_key` falls through to the ordinary right-pane scroll
    /// keys (`J`/`K`/`PageUp`/`PageDown`/`Ctrl-d`/`u`/`<`/`>`) otherwise —
    /// those still just scroll the shared viewport, unchanged from phase 3.
    fn on_diff_key(&mut self, key: KeyEvent) -> bool {
        if self.mode != Mode::Diff {
            return false;
        }
        match key.code {
            KeyCode::Esc | KeyCode::Char('h') => self.leave_diff_mode(),
            KeyCode::Char('j') | KeyCode::Down => self.move_diff_cursor(1),
            KeyCode::Char('k') | KeyCode::Up => self.move_diff_cursor(-1),
            KeyCode::Char(']') => self.jump_diff_cursor_hunk(1),
            KeyCode::Char('[') => self.jump_diff_cursor_hunk(-1),
            KeyCode::Char('V') => self.toggle_diff_anchor(),
            KeyCode::Char(' ') => self.stage_diff_cursor(),
            KeyCode::Char('d') => self.discard_prompt(),
            _ => return false,
        }
        true
    }

    /// `c` / `A` / `w`: open the commit popup. Amend / Reword pre-fill
    /// `HEAD`'s current message; a plain commit reuses `commit_draft` if an
    /// earlier `Esc` left one behind (lazygit's "draft survives a cancel").
    /// A no-op with a popup already up, without a repo, with nothing staged
    /// (`c`), or with no commit yet to amend/reword.
    fn open_commit(&mut self, kind: git::CommitKind) {
        if self.popup.is_some() {
            return;
        }
        let Some(repo) = &self.repo else { return };
        match &kind {
            git::CommitKind::Normal
                if !self.files.iter().any(|f| f.staged != git::Change::None) =>
            {
                self.last_error = Some("nothing staged to commit".to_owned());
                return;
            },
            git::CommitKind::Amend | git::CommitKind::Reword if self.commits.is_empty() => {
                self.last_error = Some("no commit yet to amend".to_owned());
                return;
            },
            _ => {},
        }

        let prefill = match &kind {
            git::CommitKind::Amend | git::CommitKind::Reword => repo.head_message().ok().flatten(),
            _ => self.commit_draft.take(),
        };
        let text = prefill.map_or_else(TextBuffer::default, |s| TextBuffer::from_text(&s));
        self.popup = Some(Popup::Commit(CommitDraft {
            text,
            kind,
            sign_off: false,
            no_verify: false,
        }));
    }

    /// Every key while `self.popup` is `Some`: printable/editing keys go to
    /// the draft's `TextBuffer`, `Ctrl-S` commits, `Ctrl-O` / `Ctrl-N` flip
    /// the sign-off / no-verify toggles, `Esc` cancels (keeping the draft
    /// for a commit popup, dropping it outright for a new-branch one — a
    /// few retyped characters cost nothing) or dismisses a note. `Enter`
    /// *submits* the new-branch popup rather than inserting a newline, the
    /// one behavioural difference from reusing `TextBuffer` as-is.
    fn popup_key(&mut self, key: KeyEvent) {
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        let mut dismiss = false;
        let mut cancel = false;
        let mut commit_now = false;
        let mut create_branch_now = false;
        let mut pick_remote_now = false;

        match &mut self.popup {
            None => return,
            Some(Popup::Note(_)) => {
                if matches!(key.code, KeyCode::Esc | KeyCode::Enter) {
                    dismiss = true;
                }
            },
            Some(Popup::Commit(draft)) => match key.code {
                KeyCode::Char('s') if ctrl => commit_now = true,
                KeyCode::Char('o') if ctrl => draft.sign_off = !draft.sign_off,
                KeyCode::Char('n') if ctrl => draft.no_verify = !draft.no_verify,
                KeyCode::Esc => cancel = true,
                KeyCode::Enter => draft.text.insert_newline(),
                KeyCode::Backspace => draft.text.backspace(),
                KeyCode::Left => draft.text.move_left(),
                KeyCode::Right => draft.text.move_right(),
                KeyCode::Up => draft.text.move_up(),
                KeyCode::Down => draft.text.move_down(),
                KeyCode::Char(c) if !ctrl => draft.text.insert_char(c),
                _ => {},
            },
            Some(Popup::NewBranch(buf)) => match key.code {
                KeyCode::Esc => dismiss = true,
                KeyCode::Enter => create_branch_now = true,
                KeyCode::Backspace => buf.backspace(),
                KeyCode::Left => buf.move_left(),
                KeyCode::Right => buf.move_right(),
                KeyCode::Char(c) if !ctrl => buf.insert_char(c),
                _ => {},
            },
            Some(Popup::RemotePick(pick)) => match key.code {
                KeyCode::Esc => dismiss = true,
                KeyCode::Enter => pick_remote_now = true,
                KeyCode::Char('j') | KeyCode::Down => {
                    pick.selected = (pick.selected + 1).min(pick.remotes.len().saturating_sub(1));
                },
                KeyCode::Char('k') | KeyCode::Up => pick.selected = pick.selected.saturating_sub(1),
                _ => {},
            },
        }

        if dismiss {
            self.popup = None;
        }
        if cancel {
            if let Some(Popup::Commit(draft)) = &self.popup {
                self.commit_draft = Some(draft.text.text());
            }
            self.popup = None;
        }
        if commit_now {
            self.do_commit();
        }
        if create_branch_now {
            self.do_create_branch();
        }
        if pick_remote_now {
            if let Some(Popup::RemotePick(pick)) = &self.popup {
                let name = pick.remotes.get(pick.selected).map(|r| r.name.clone());
                self.popup = None;
                if let Some(name) = name {
                    self.push_with_upstream(name);
                }
            }
        }
    }

    /// `Ctrl-S` in the commit popup: run `Repo::commit`, then either close
    /// the popup and refresh (phase 2's `refresh()` picks up the new
    /// `HEAD`, phase 3's `update_right_pane` sees the now-empty staged diff)
    /// or swap the popup for a dismissible `Note` on failure, keeping the
    /// draft either way except on success.
    fn do_commit(&mut self) {
        let Some(Popup::Commit(draft)) = &self.popup else {
            return;
        };
        if !matches!(draft.kind, git::CommitKind::Fixup { .. }) && draft.text.is_blank() {
            self.popup = Some(Popup::Note("empty commit message".to_owned()));
            return;
        }
        let message = draft.text.text();
        let opts = git::CommitOpts {
            sign_off: draft.sign_off,
            no_verify: draft.no_verify,
        };
        let kind = draft.kind.clone();
        let Some(repo) = &self.repo else { return };
        let result = repo.commit(&kind, &message, opts);

        match result {
            Ok(_hash) => {
                self.commit_draft = None;
                self.popup = None;
                self.refresh();
            },
            Err(git::GitError::NothingStaged) => {
                self.popup = Some(Popup::Note("nothing staged to commit".to_owned()));
            },
            Err(e) => {
                self.popup = Some(Popup::Note(e.to_string()));
            },
        }
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
        // Not `mock_files().len() - 1`: the mock fixture spans several
        // directories, so the Files pane is a tree (root + dir headers +
        // files), more rows than files.
        let last = app.row_count(Pane::Files) - 1;
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
        // Row index in the tree, not a flat index into `mock_files()`: the
        // fixture spans several directories, so a directory header row can
        // sit ahead of the file this test is after.
        let png = (0..app.row_count(Pane::Files))
            .find(|&i| {
                Path::new(&app.file_display(i))
                    .extension()
                    .is_some_and(|e| e == "png")
            })
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
    }
}
