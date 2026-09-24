//! Application state and the draw / event loop.
//!
//! Phase 2 wired every left pane (Status, Files, Branches, Commits, Stash) to
//! a real read-only `git::Repo`. `App` owns the repo handle, the cached
//! snapshot, which left pane is focused, and one selection cursor per pane.
//! `App::mock()` is the repo-free path the render tests use.

pub mod events;
pub mod mock;
pub mod screens;
pub mod terminal;
pub mod theme;
pub(super) mod theme_config;

use std::collections::HashSet;
use std::fmt::{self, Display, Write as _};
use std::ops::Range;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, mpsc};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::components::tui_overlay::state::OverlayState;
use crate::components::ui::mouse_pointer::MousePointer;
use crate::components::ui::toast::Toast;
use crate::domain::profile::Profile;
use crate::domain::profile::settings::Settings;
use color_eyre::Result;
use enum_map::{Enum, EnumMap};
use ratatui::crossterm::event::{
    Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::layout::{Position, Rect};
use ratatui::text::{Line, Text};
use ratatui_image::picker::Picker;

use crate::app::events::{AppEvent, Events};
use crate::app::screens as ui;
use crate::app::terminal::Tui;
use crate::components::ui::text_input::{TextInput, TextInputMode};
use crate::domain::git;
use crate::domain::git::apply::{ApplyDir, ApplyTarget};
use crate::domain::git::diff::{DiffOpts, DiffSide};
use crate::domain::git::error::GitResult;
use crate::domain::image::detect;
use crate::domain::image::preview::{self, Preview};

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
    Commit(git::model::CommitEntry, git::diff::Diff),
    /// Stash pane: the selected entry's `git stash show -p`, scrolled and
    /// rendered like a commit diff (`docs/PLAN_10_STASH.md`).
    Stash(git::model::StashEntry, git::diff::Diff),
    /// Branches pane, not drilled in: the selected branch's own log, shown
    /// passively (no Enter needed), lazygit's live branch -> log preview.
    BranchLog(BranchLog),
}

/// A Files-pane selection's two sides at once, lazygit's own Unstaged
/// Changes / Staged Changes split: a file half-staged shows real content in
/// both, a file entirely on one side shows an empty diff on the other.
#[derive(Debug, Clone)]
pub struct FilesDiff {
    pub unstaged: git::diff::Diff,
    pub staged: git::diff::Diff,
}

/// Read-only view of an editor popup for `ui::draw_commit_popup`
/// (`docs/PLAN_7_COMMIT.md`), reused as-is for the new-branch popup
/// (`docs/PLAN_8_BRANCHES.md`) — same shape, different title/footer.
/// Borrows the draft's lines, so it is cheap to build fresh every frame
/// rather than cached.
pub struct CommitPopupView<'a> {
    pub title: &'static str,
    /// Commit subject, or the whole single-field input for another popup.
    pub input: &'a TextInput,
    /// Commit body editor; absent for the new-branch input.
    pub description: Option<&'a TextInput>,
    pub summary_focused: bool,
    pub overlay_state: Option<&'a mut OverlayState>,
    /// Compatibility view of the component's text lines.
    pub lines: &'a [String],
    /// `(row, character column)` cursor position.
    pub cursor: (usize, usize),
    /// `Some((sign_off, no_verify))` for the commit popup's toggle line;
    /// `None` for the new-branch popup, which has nothing to toggle.
    pub toggles: Option<(bool, bool)>,
    /// Footer key hints, e.g. `"Commit: Ctrl-S | ... | Cancel: Esc"`.
    pub hints: &'static str,
}

/// The one active popup, ready for rendering. Explicit variants keep popup
/// precedence in one `match` instead of chaining `if let` checks.
pub enum PopupView<'a> {
    Commit(CommitPopupView<'a>),
    CommitAllConfirm(&'a mut OverlayState),
    NewBranch(CommitPopupView<'a>),
    Stash(CommitPopupView<'a>),
    CommandLog(CommandLogView),
    Upstream(CommitPopupView<'a>),
    Note(&'a str),
}

/// The `@` viewer's render data: every recorded command and how far the view
/// is scrolled up from the newest.
#[derive(Debug)]
pub struct CommandLogView {
    pub records: Vec<git::command_log::CommandRecord>,
    pub from_bottom: usize,
}

/// A branch's own commit log for the passive `DiffView::BranchLog` preview.
#[derive(Debug, Clone)]
pub struct BranchLog {
    pub branch: String,
    pub commits: Vec<git::model::CommitEntry>,
}

/// Snapshot plus any active drill-down data loaded in the same worker.
#[doc(hidden)]
#[derive(Debug)]
pub struct RefreshCompletion {
    pub(crate) snapshot: Result<git::Snapshot, String>,
    pub(crate) profile: Option<Profile>,
    pub(crate) branch_log: Option<(String, Result<Vec<git::model::CommitEntry>, String>)>,
    pub(crate) commit_files: Option<(String, Result<Vec<git::model::FileEntry>, String>)>,
}

#[derive(Default)]
struct RefreshQueryState {
    in_flight: bool,
    pending: bool,
}

#[derive(Default)]
struct ImageQueryState {
    path: Option<PathBuf>,
    generation: u64,
    in_flight: bool,
    pending: Option<(PathBuf, u64)>,
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
    commits: Vec<git::model::CommitEntry>,
    /// The branch-list cursor to restore when `Esc` backs out.
    return_index: usize,
}

/// State for the Commits pane's Enter-to-drill-down: the pane swaps its
/// commit list for that commit's own changed-file tree, in place, the same
/// shape `BranchDrill` gives the Branches pane one level up. Read only, no
/// staging; `Esc` backs out.
struct CommitDrill {
    hash: String,
    /// `"<short_hash> <summary>"`, for `App::commits_title`.
    title: String,
    /// One synthetic `FileEntry` per file the commit's diff touched, same
    /// index order as the underlying `git::diff::Diff::files`/`file_lines()` so a
    /// selected row's scroll target is a plain index lookup.
    files: Vec<git::model::FileEntry>,
    /// The commit-list cursor to restore when `Esc` backs out.
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
    /// Apply an existing Git identity to future Ferrit commits for this run.
    SelectAuthor(Option<crate::domain::profile::settings::Identity>),
    /// The whole file's worktree change (`d` in `Mode::Nav`, Files focused).
    DiscardFile(PathBuf),
    /// A hunk or a line selection (`d` in `Mode::Diff`, worktree side).
    DiscardGranule(Granule),
    /// `d` in `Mode::Nav`, Branches focused: `git branch -d` / `-D`. `force`
    /// is `false` on the first confirm, `true` on the second one offered
    /// after an unmerged-branch refusal (`App::run_confirm`).
    DeleteBranch { name: String, force: bool },
    /// `d` on the Stash pane: `git stash drop`, resolved by oid.
    DropStash { oid: String },
    /// Push a branch known to be behind its upstream, using a lease guard.
    ForcePush,
}

/// Body-line ranges (global `diff.text` line indices) for every hunk of a
/// single-file `Diff`, plus which of those lines are selectable.
fn hunk_lines_for(diff: &git::diff::Diff) -> Vec<HunkLines> {
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
fn selectable_lines(diff: &git::diff::Diff) -> Vec<usize> {
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
fn hunk_id_at(diff: &git::diff::Diff, line: usize) -> Option<u64> {
    let hl = hunk_lines_for(diff)
        .into_iter()
        .find(|hl| hl.lines.contains(&line))?;
    Some(hunk_content_id(diff, hl.hunk_index))
}

/// Stable id for hunk `hunk_index` of `diff`: a hash of its header + body
/// text, so a background refresh can re-find the same hunk even once
/// staging moved a *different* hunk out from under it (gitu's `Item.id`).
fn hunk_content_id(diff: &git::diff::Diff, hunk_index: usize) -> u64 {
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

/// Modal state that owns all input while it is up, the same idea as
/// `show_help` today but richer (`docs/PLAN_7_COMMIT.md`).
enum Popup {
    Commit(commit::CommitDraft),
    CommitAllConfirm,
    /// New-branch name input (`docs/PLAN_8_BRANCHES.md`). `Enter` *submits*
    /// here, unlike the commit popup, where `Enter` inserts a newline —
    /// the only behavioural difference from reusing `TextInput` outright.
    NewBranch(TextInput),
    /// Stash message input, `s` on Files (`docs/PLAN_10_STASH.md`). `Enter`
    /// submits; an empty message lets git write its own.
    Stash(TextInput),
    /// `P` with no upstream: edit `<remote> <branch>` before first push.
    Upstream(TextInput),
    /// `@`: every recorded `git` command, newest last (`docs/PLAN_12_POLISH.md`
    /// P0). `from_bottom` is how many rows the view is scrolled up from the
    /// newest entry; the renderer clamps it to what fits.
    CommandLog {
        from_bottom: usize,
    },
    /// A dismissible message: a commit failure, "empty commit message", a
    /// branch-op failure, or a merge conflict.
    Note(String),
}

/// Mouse-wheel step for the right pane, in lines. Matches gitu's default
/// `mouse_scroll_lines`.
const WHEEL_LINES: isize = 3;

/// `(discriminant, diff text)` for cheap "did the right pane actually change"
/// checks: `String` equality on a few KB, no hashing.
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

/// Which of the Branches pane's own two real tabs is showing (the third,
/// Tags, is still an inert label — `Pane::title`). `Remotes` has no
/// selection cursor of its own; it is `Repo::remotes()` rendered plainly,
/// same as the Local tab's list was for the entirety of phase 2 before
/// phase 8 made it actionable. `docs/PLAN_9_REMOTE.md`.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
enum BranchesTab {
    #[default]
    Local,
    Remotes,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SelectionKey {
    File(PathBuf),
    Directory(PathBuf),
    Branch(String),
    Commit(String),
    Stash(String),
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
    /// Git author name from the repository's effective config.
    git_user_name: Option<String>,
    /// Git settings and activity shown in the profile drawer.
    profile: Profile,
    /// Optional per-commit author chosen from identities already in Git config.
    selected_author: Option<crate::domain::profile::settings::Identity>,
    /// First visible profile drawer line.
    profile_scroll: usize,
    theme_config: theme_config::ThemeConfig,
    theme_rgb_channel: usize,
    theme_mode: theme_config::ThemeMode,
    theme_palette_selected: usize,
    theme_picker_display: crate::components::ui::color_picker::ColorPickerDisplay,
    theme_saved_config: theme_config::ThemeConfig,
    profile_hit_areas: screens::profile::ProfileHitAreas,
    header: git::model::StatusHeader,
    files: Vec<git::model::FileEntry>,
    /// Directories collapsed in the Files pane's tree view (`FileRow`,
    /// `files_tree_rows`). Empty means "everything expanded", lazygit's own
    /// default; paths persist across `refresh()`, only `Enter` on a
    /// directory row changes this.
    collapsed_dirs: HashSet<PathBuf>,
    branches: Vec<git::model::BranchEntry>,
    /// `Some` while the Branches pane is drilled into one branch's own log
    /// (Enter on a branch, `Esc` to back out); `None` shows the branch list.
    branch_drill: Option<BranchDrill>,
    /// Configured remotes, feeding the Branches pane's Remotes tab.
    /// `docs/PLAN_9_REMOTE.md`.
    remotes: Vec<git::model::RemoteEntry>,
    /// Which of the Branches pane's own two tabs is showing.
    /// `Ctrl-Right`/`Ctrl-Left` switch it, Branches focused.
    branches_tab: BranchesTab,
    commits: Vec<git::model::CommitEntry>,
    /// `Some` while the Commits pane is drilled into one commit's own
    /// changed-file tree (Enter on a commit, `Esc` to back out); `None`
    /// shows the commit list.
    commit_drill: Option<CommitDrill>,
    stashes: Vec<git::model::StashEntry>,
    /// A merge, rebase, cherry-pick or revert stopped mid-way (`Snapshot`).
    operation: Option<git::model::Operation>,
    /// Last `refresh()` failure, shown in the Status pane. Never a panic.
    last_error: Option<String>,
    /// Optional worktree watcher failure; polling remains active as fallback.
    watch_error: Option<String>,

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
    /// Click target for the configured Git author in the bottom info panel.
    author_click_area: Rect,
    /// Whether the mouse is currently over that clickable author name.
    mouse_pointer: MousePointer,
    /// Animated side sheet opened by clicking that author.
    pub(crate) author_overlay: OverlayState,
    /// Backdrop state for the commit editor modal.
    pub(crate) commit_overlay: OverlayState,
    /// Persistent bottom-right error notification, dismissed by clicking `x`.
    pub(crate) toast: Option<Toast>,
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
    confirm_overlay: OverlayState,
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
    /// Start time for the inline branch-row spinner.
    remote_busy_started: Option<Instant>,
    remote_cancel: Arc<AtomicBool>,
    remote_worker: Option<JoinHandle<()>>,
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
    /// One snapshot worker at a time. Bursty filesystem events collapse into
    /// one follow-up snapshot instead of queuing stale concurrent reads.
    refresh_query: RefreshQueryState,
    /// Remote failure must outlive snapshot completion that was requested
    /// immediately after the remote command.
    remote_refresh_error: Option<String>,
    /// One selected-diff worker; only latest requested key waits behind it.
    diff_query: DiffQueryState,
    image_query: ImageQueryState,
}

mod tree;

mod branch_actions;
mod commit;
pub mod diff_query;
mod drill_nav;
mod error;
pub mod image_query;
mod input;
mod popups;
mod remote;
mod staging;
mod stash_actions;

pub(crate) use error::AppError;

#[cfg(test)]
mod tests;

use diff_query::{DiffQueryState, RightKey};
use tree::{FileRow, commit_drill_files, tree_rows};

impl App {
    fn base(repo: Option<git::Repo>, theme_config: theme_config::ThemeConfig) -> Self {
        let repo_name = repo
            .as_ref()
            .map_or_else(|| "ferrit".to_owned(), git::Repo::name);
        let git_user_name = repo.as_ref().and_then(git::Repo::user_name);
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
                git::Repo::identity_settings,
            );
        let activity_commits = repo
            .as_ref()
            .and_then(|repo| repo.activity().ok())
            .unwrap_or_default();
        let profile = Profile::new(
            Settings {
                global_identities,
                repository_identity,
                effective_identity,
                identity_source,
            },
            &activity_commits,
        );
        let theme_palette_selected = crate::components::ui::color_picker::nearest_index(
            theme_config.color(),
            crate::components::ui::color_picker::ColorPickerDisplay::default(),
        );
        let theme_saved_config = theme_config.clone();
        Self {
            focus: Pane::default(),
            selection: EnumMap::default(),
            show_help: false,
            should_quit: false,
            repo,
            repo_name,
            git_user_name,
            profile,
            selected_author: None,
            profile_scroll: 0,
            theme_config,
            theme_rgb_channel: theme_config::RGB_RED_CHANNEL,
            theme_mode: theme_config::ThemeMode::Idle,
            theme_palette_selected,
            theme_picker_display: crate::components::ui::color_picker::ColorPickerDisplay::default(
            ),
            theme_saved_config,
            profile_hit_areas: screens::profile::ProfileHitAreas::default(),
            header: git::model::StatusHeader::default(),
            files: Vec::new(),
            collapsed_dirs: HashSet::new(),
            branches: Vec::new(),
            branch_drill: None,
            remotes: Vec::new(),
            branches_tab: BranchesTab::default(),
            commits: Vec::new(),
            commit_drill: None,
            stashes: Vec::new(),
            operation: None,
            last_error: None,
            watch_error: None,
            picker: Picker::halfblocks(),
            preview: Preview::None,
            diff: DiffView::None,
            right_key: None,
            rendered_diff: None,
            right_scroll: 0,
            right_viewport: 0,
            right_area: Rect::ZERO,
            author_click_area: Rect::ZERO,
            mouse_pointer: MousePointer::default(),
            author_overlay: OverlayState::new().with_duration(Duration::from_millis(200)),
            commit_overlay: OverlayState::new(),
            toast: None,
            left_areas: EnumMap::default(),
            list_offset: EnumMap::default(),
            right_focused: false,
            mode: Mode::default(),
            cursor: DiffCursor::default(),
            pending_confirm: None,
            confirm_overlay: OverlayState::new(),
            popup: None,
            commit_draft: None,
            remote_busy: None,
            remote_busy_started: None,
            remote_cancel: Arc::new(AtomicBool::new(false)),
            remote_worker: None,
            status_note: None,
            event_sender: None,
            refresh_query: RefreshQueryState::default(),
            remote_refresh_error: None,
            diff_query: DiffQueryState::default(),
            image_query: ImageQueryState::default(),
        }
    }

    /// Open the repo at or above `path`, then take one snapshot.
    pub fn open(path: &Path) -> GitResult<Self> {
        let mut app = Self::base(
            Some(git::Repo::open(path)?),
            theme_config::ThemeConfig::load(),
        );
        app.refresh();
        Ok(app)
    }

    /// Repo-free instance backed by `mock` data, for the render tests.
    pub fn mock() -> Self {
        let mut app = Self::base(None, theme_config::ThemeConfig::default());
        app.header = mock::mock_header();
        app.files = mock::mock_files();
        app.branches = mock::mock_branches();
        app.remotes = mock::mock_remotes();
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
                self.report_notice(line);
            }
            self.picker = found.picker;
            self.invalidate_image_query();
            self.update_right_pane();
        }
    }

    /// Re-read the wired panes. On error keep the old snapshot and stash the
    /// message; never propagate, never panic. No-op without a repo.
    pub fn refresh(&mut self) {
        let branch = self.branch_drill.as_ref().map(|drill| drill.branch.clone());
        let commit = self.commit_drill.as_ref().map(|drill| drill.hash.clone());
        let Some(repo) = &mut self.repo else { return };
        let completion = Self::load_refresh(repo, branch, commit);
        self.apply_refresh_result(completion);
    }

    /// Request refresh without blocking the TUI. Outside `run()` (tests and
    /// startup helpers), retain synchronous behavior.
    pub(super) fn request_refresh(&mut self) {
        let Some(sender) = self.event_sender.clone() else {
            self.refresh();
            return;
        };
        if self.refresh_query.in_flight {
            self.refresh_query.pending = true;
            return;
        }
        let Some(path) = self
            .repo
            .as_ref()
            .map(|repo| repo.reopen_path().to_path_buf())
        else {
            return;
        };
        let branch = self.branch_drill.as_ref().map(|drill| drill.branch.clone());
        let commit = self.commit_drill.as_ref().map(|drill| drill.hash.clone());
        self.refresh_query.in_flight = true;
        thread::spawn(move || {
            let completion = run_worker(WorkerKind::Refresh, || match git::Repo::open(&path) {
                Ok(mut repo) => Self::load_refresh(&mut repo, branch, commit),
                Err(error) => {
                    let message = error.to_string();
                    RefreshCompletion {
                        snapshot: Err(message.clone()),
                        profile: None,
                        branch_log: branch.map(|name| (name, Err(message.clone()))),
                        commit_files: commit.map(|hash| (hash, Err(message))),
                    }
                },
            })
            .unwrap_or_else(|error| RefreshCompletion {
                snapshot: Err(error.to_string()),
                profile: None,
                branch_log: None,
                commit_files: None,
            });
            let _ = sender.send(AppEvent::RefreshDone(Box::new(completion)));
        });
    }

    fn load_refresh(
        repo: &mut git::Repo,
        branch: Option<String>,
        commit: Option<String>,
    ) -> RefreshCompletion {
        let snapshot = repo.snapshot().map_err(|error| error.to_string());
        let branch_log = branch.map(|name| {
            let result = repo.branch_log(&name).map_err(|error| error.to_string());
            (name, result)
        });
        let commit_files = commit.map(|hash| {
            let result = repo
                .commit_diff(&hash, DiffOpts::default())
                .map(|diff| commit_drill_files(&diff))
                .map_err(|error| error.to_string());
            (hash, result)
        });
        RefreshCompletion {
            snapshot,
            profile: repo.activity().ok().map(|commits| {
                let (global_identities, repository_identity, effective_identity, identity_source) =
                    repo.identity_settings();
                Profile::new(
                    Settings {
                        global_identities,
                        repository_identity,
                        effective_identity,
                        identity_source,
                    },
                    &commits,
                )
            }),
            branch_log,
            commit_files,
        }
    }

    fn apply_refresh_result(&mut self, completion: RefreshCompletion) {
        if let Some(profile) = completion.profile {
            self.profile = profile;
        }
        let old_selection: [(Pane, usize, Option<SelectionKey>); 5] =
            PANES.map(|pane| (pane, self.selection[pane], self.selection_key(pane)));
        match completion.snapshot {
            Ok(snap) => {
                self.header = snap.header;
                self.files = snap.files;
                self.branches = snap.branches;
                self.remotes = snap.remotes;
                self.commits = snap.commits;
                self.stashes = snap.stashes;
                self.operation = snap.operation;
                self.last_error = None;
            },
            Err(error) => self.report_error(AppError::Refresh(error)),
        }

        // A drilled branch log (Enter on Branches) stays live across a
        // background refresh instead of going stale; a branch that vanished
        // (deleted, renamed) backs out of the drill-down instead of erroring
        // the whole refresh.
        if let Some((branch, result)) = completion.branch_log
            && self
                .branch_drill
                .as_ref()
                .is_some_and(|drill| drill.branch == branch)
        {
            match result {
                Ok(commits) => {
                    if let Some(drill) = &mut self.branch_drill {
                        drill.commits = commits;
                    }
                },
                Err(_) => self.branch_drill = None,
            }
        }

        // Same treatment for a drilled commit's file tree (Enter on
        // Commits): re-read the file list so it reflects the diff as of
        // this refresh; a commit that vanished (e.g. a reword/rebase that
        // changed its hash) backs out rather than erroring the refresh.
        if let Some((hash, result)) = completion.commit_files
            && self
                .commit_drill
                .as_ref()
                .is_some_and(|drill| drill.hash == hash)
        {
            match result {
                Ok(files) => {
                    if let Some(drill) = &mut self.commit_drill {
                        drill.files = files;
                    }
                },
                Err(_) => self.commit_drill = None,
            }
        }

        for (pane, old_index, key) in old_selection {
            let last = self.row_count(pane).saturating_sub(1);
            let new_index = key
                .as_ref()
                .and_then(|key| self.find_selection_key(pane, key))
                .unwrap_or(old_index);
            self.selection[pane] = new_index.min(last);
        }
        self.diff_query.refresh_requested = true;
        self.invalidate_image_query();
        self.update_right_pane();
    }

    fn on_refresh_done(&mut self, completion: RefreshCompletion) {
        self.refresh_query.in_flight = false;
        let rerun = std::mem::take(&mut self.refresh_query.pending);
        self.apply_refresh_result(completion);
        if rerun {
            self.request_refresh();
        } else if let Some(error) = self.remote_refresh_error.take() {
            self.report_error(AppError::Refresh(error));
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
        if self.focus != Pane::Commits {
            return;
        }
        let Some(drill) = &self.commit_drill else {
            return;
        };
        let rows = tree_rows(&drill.files, &self.collapsed_dirs);
        let Some(FileRow::File { index, .. }) = rows.get(self.selected(Pane::Commits)) else {
            return;
        };
        let DiffView::Commit(_, diff) = &self.diff else {
            return;
        };
        if let Some(&line) = diff.file_lines().get(*index) {
            self.right_scroll = line;
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
        tree_rows(&self.files, &self.collapsed_dirs)
    }

    /// Same tree shape as `files_tree_rows`, over a drilled commit's own
    /// changed files instead of the worktree's. Empty while not drilled.
    fn commit_tree_rows(&self) -> Vec<FileRow> {
        match &self.commit_drill {
            Some(drill) => tree_rows(&drill.files, &self.collapsed_dirs),
            None => Vec::new(),
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
            DiffView::Commit(_, d) | DiffView::Stash(_, d) => d.text.lines().count(),
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
        let anchors = match &self.diff {
            DiffView::Commit(_, d) | DiffView::Stash(_, d) => d.file_lines(),
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

    /// Configured Git author name, shown in the Info panel header when set.
    pub fn git_user_name(&self) -> Option<&str> {
        self.git_user_name.as_deref()
    }

    /// All author identities and activity for the open repository.
    pub(crate) fn profile(&self) -> &Profile {
        &self.profile
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
    ) -> Option<(Text<'static>, usize, git::diff::DiffStat)> {
        let key = &self.right_key;
        let cache = &mut self.rendered_diff;
        match &self.diff {
            DiffView::Commit(_, diff) | DiffView::Stash(_, diff) => {
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

    /// Store the configured Git author's clickable cells for mouse routing.
    pub fn set_author_click_area(&mut self, area: Rect) {
        self.author_click_area = area;
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
            Pane::Branches if self.branches_tab == BranchesTab::Remotes => 0,
            Pane::Branches => self
                .branch_drill
                .as_ref()
                .map_or(self.branches.len(), |drill| drill.commits.len()),
            Pane::Commits => match &self.commit_drill {
                Some(_) => self.commit_tree_rows().len(),
                None => self.commits.len(),
            },
            Pane::Stash => self.stashes.len(),
        }
    }

    fn selection_key(&self, pane: Pane) -> Option<SelectionKey> {
        match pane {
            Pane::Status => None,
            Pane::Files => selection_key_for_file_rows(
                &self.files_tree_rows(),
                &self.files,
                self.selected(pane),
            ),
            Pane::Branches if self.branches_tab == BranchesTab::Remotes => None,
            Pane::Branches => self.branch_drill.as_ref().map_or_else(
                || {
                    self.branches
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
            Pane::Commits => self.commit_drill.as_ref().map_or_else(
                || {
                    self.commits
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
                .stashes
                .get(self.selected(pane))
                .map(|entry| SelectionKey::Stash(entry.oid.clone())),
        }
    }

    fn find_selection_key(&self, pane: Pane, key: &SelectionKey) -> Option<usize> {
        match (pane, key) {
            (Pane::Files, SelectionKey::File(_) | SelectionKey::Directory(_)) => {
                find_file_row_key(&self.files_tree_rows(), &self.files, key)
            },
            (Pane::Branches, SelectionKey::Branch(name)) if self.branch_drill.is_none() => {
                self.branches.iter().position(|entry| entry.name == *name)
            },
            (Pane::Branches, SelectionKey::Commit(hash)) => self
                .branch_drill
                .as_ref()?
                .commits
                .iter()
                .position(|entry| entry.full_hash == *hash),
            (Pane::Commits, SelectionKey::Commit(hash)) if self.commit_drill.is_none() => self
                .commits
                .iter()
                .position(|entry| entry.full_hash == *hash),
            (Pane::Commits, SelectionKey::File(_) | SelectionKey::Directory(_)) => {
                let drill = self.commit_drill.as_ref()?;
                find_file_row_key(&self.commit_tree_rows(), &drill.files, key)
            },
            (Pane::Stash, SelectionKey::Stash(oid)) => {
                self.stashes.iter().position(|entry| entry.oid == *oid)
            },
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
            vec![theme::error_line(&format!("error: {err}"))]
        } else {
            let h = &self.header;
            let mut line = format!("{} \u{2192} {}", self.repo_name, h.branch);
            if h.ahead > 0 {
                let _ = write!(line, " \u{2191}{}", h.ahead);
            }
            if h.behind > 0 {
                let _ = write!(line, " \u{2193}{}", h.behind);
            }
            let mut lines = vec![theme::status_line(&line)];
            if h.conflicts > 0 {
                lines.push(theme::error_line(&format!(
                    "\u{2717} {} merge conflict(s)",
                    h.conflicts
                )));
            }
            lines
        };
        if let Some(operation) = self.operation {
            // Right under the first line, error or header, so it is the
            // first thing read while git waits on the user.
            out.insert(out.len().min(1), theme::operation_line(&operation.label()));
        }
        if let Some(label) = self.remote_busy_label() {
            out.push(theme::busy_line(label));
        } else if self.last_error.is_none()
            && let Some(note) = &self.status_note
        {
            out.push(theme::status_line(note));
        }
        out
    }

    /// Keep persistent Status text while moving typed error into transient toast.
    pub(super) fn report_error(&mut self, error: impl Into<AppError>) {
        let error = error.into();
        self.last_error = Some(error.to_string());
        self.toast = Some(Toast::error(error));
    }

    pub(super) fn report_notice(&mut self, message: impl Into<String>) {
        self.last_error = Some(message.into());
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
        if self.branches_tab == BranchesTab::Remotes {
            if self.remotes.is_empty() {
                return vec![Line::raw("no remotes configured")];
            }
            return self.remotes.iter().map(theme::remote_line).collect();
        }
        if self.branches.is_empty() {
            return vec![Line::raw("no local branches")];
        }
        self.branches
            .iter()
            .map(|branch| {
                let operation = branch
                    .is_head
                    .then(|| self.remote_branch_status())
                    .flatten();
                theme::branch_line_with_status(branch, operation.as_deref())
            })
            .collect()
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

    /// Commits pane rows: the commit list, or one commit's own changed-file
    /// tree while drilled in (`commit_drill`, `enter_commit_files`), same
    /// shape `branch_lines` gives the Branches pane.
    pub fn commit_lines(&self) -> Vec<Line<'static>> {
        if let Some(drill) = &self.commit_drill {
            return self
                .commit_tree_rows()
                .iter()
                .filter_map(|row| match row {
                    FileRow::Dir {
                        name,
                        depth,
                        expanded,
                        ..
                    } => Some(theme::dir_line(name, *depth, *expanded)),
                    FileRow::File { index, depth } => drill
                        .files
                        .get(*index)
                        .map(|entry| theme::file_line(entry, *depth)),
                })
                .collect();
        }
        if self.commits.is_empty() {
            return vec![Line::raw("no commits yet")];
        }
        self.commits.iter().map(theme::commit_line).collect()
    }

    /// `[4] Commits - Reflog`, or `[4] Diff files (<hash> <summary>)` while
    /// drilled into a commit's own changed-file tree (Enter on a commit,
    /// `Esc` to back out; see `enter_commit_files`).
    pub fn commits_title(&self) -> String {
        match &self.commit_drill {
            Some(drill) => format!("[4] Diff files ({})", drill.title),
            None => Pane::Commits.title().to_owned(),
        }
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
                .map(git::model::FileEntry::display)
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

    /// Draw, then block for the next event batch, until `should_quit`. Events
    /// come from terminal input, a recursive worktree watch, and a 10s poll.
    /// Bounded batches avoid repainting for every auto-repeat key while still
    /// guaranteeing regular redraws during sustained input.
    pub fn run(&mut self, terminal: &mut Tui) -> Result<()> {
        let events = Events::new(self.watch_root().as_deref())?;
        self.watch_error = events.watch_error().map(|error| {
            format!("filesystem watcher unavailable; polling fallback active: {error}")
        });
        self.last_error = self.watch_error.clone();
        // A background fetch/pull/push (`start_remote_op`) needs its own
        // way back onto this channel; only `run()` has an `Events` to ask
        // for one, so it hands `on_key` this clone rather than `on_key`
        // taking `&Events` directly (it is also called from `feed_key`,
        // which has none).
        self.event_sender = Some(events.sender());
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
            terminal.draw(|frame| ui::draw(frame, self))?;

            let was_animating = self.author_overlay.is_animating();
            let toast_animating = self.toast.as_ref().is_some_and(Toast::is_animating);
            let remote_animating = self.remote_busy.is_some();
            let timeout = (was_animating || toast_animating || remote_animating)
                .then_some(Duration::from_millis(16));
            let batch = if let Some(timeout) = timeout {
                match events.next_batch_timeout(timeout) {
                    Err(error) => return Err(error),
                    Ok(Some(batch)) => batch,
                    Ok(None) => {
                        let elapsed = overlay_tick.elapsed();
                        self.author_overlay.tick(elapsed);
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
                        let toast_consumed =
                            self.toast.as_mut().is_some_and(|toast| toast.on_mouse(m));
                        if toast_consumed {
                            self.mouse_pointer.request(false);
                        } else {
                            self.on_mouse(m);
                        }
                        self.mouse_pointer.sync()?;
                    },
                    AppEvent::Input(_) => {},
                    AppEvent::Refresh => self.request_refresh(),
                    AppEvent::RefreshDone(completion) => self.on_refresh_done(*completion),
                    AppEvent::DiffDone(completion) => self.on_diff_done(completion),
                    AppEvent::ImageDone(completion) => self.on_image_done(completion),
                    AppEvent::RemoteDone { op, message } => self.on_remote_done(op, message),
                }
                if self.should_quit {
                    break;
                }
            }
            if self.author_overlay.is_animating() && was_animating {
                self.author_overlay.tick(overlay_tick.elapsed());
            }
            self.tick_toast(overlay_tick.elapsed());
            overlay_tick = Instant::now();
        }
        if self.remote_worker.is_some() {
            self.remote_cancel
                .store(true, std::sync::atomic::Ordering::Release);
            if let Some(worker) = self.remote_worker.take() {
                let _ = worker.join();
            }
            self.remote_busy = None;
        }
        Ok(())
    }

    fn tick_toast(&mut self, elapsed: Duration) {
        if let Some(toast) = &mut self.toast {
            toast.tick(elapsed);
            if toast.is_closed() {
                self.toast = None;
            }
        }
    }
}

fn selection_key_for_file_rows(
    rows: &[FileRow],
    files: &[git::model::FileEntry],
    selected: usize,
) -> Option<SelectionKey> {
    match rows.get(selected)? {
        FileRow::Dir { path, .. } => Some(SelectionKey::Directory(path.clone())),
        FileRow::File { index, .. } => files
            .get(*index)
            .map(|entry| SelectionKey::File(entry.path.clone())),
    }
}

fn find_file_row_key(
    rows: &[FileRow],
    files: &[git::model::FileEntry],
    key: &SelectionKey,
) -> Option<usize> {
    rows.iter().position(|row| match (row, key) {
        (FileRow::Dir { path, .. }, SelectionKey::Directory(wanted)) => path == wanted,
        (FileRow::File { index, .. }, SelectionKey::File(wanted)) => {
            files.get(*index).is_some_and(|entry| entry.path == *wanted)
        },
        _ => false,
    })
}

#[derive(Debug, Clone, Copy)]
pub(super) enum WorkerKind {
    Refresh,
    Diff,
    ImagePreview,
    RemoteOperation,
}

impl Display for WorkerKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = match self {
            Self::Refresh => "refresh",
            Self::Diff => "diff",
            Self::ImagePreview => "image preview",
            Self::RemoteOperation => "remote operation",
        };
        f.write_str(label)
    }
}

#[derive(Debug)]
pub(super) struct WorkerError {
    worker: WorkerKind,
    detail: String,
}

impl Display for WorkerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} worker panicked: {}", self.worker, self.detail)
    }
}

/// Run worker logic behind a panic boundary so completion events can release
/// single-flight state even when a repository operation unexpectedly panics.
pub(super) fn run_worker<T>(
    worker: WorkerKind,
    work: impl FnOnce() -> T,
) -> std::result::Result<T, WorkerError> {
    catch_unwind(AssertUnwindSafe(work)).map_err(|payload| {
        let detail = payload
            .downcast_ref::<String>()
            .map(String::as_str)
            .or_else(|| payload.downcast_ref::<&'static str>().copied())
            .unwrap_or("non-string panic payload")
            .to_owned();
        WorkerError { worker, detail }
    })
}
