//! The key hint bar and the help screen, generated from the `Keymap` so they
//! show the keys the user actually has. See `docs/PLAN_12_POLISH.md` P3.

use unicode_width::UnicodeWidthStr;

use super::Pane;
use super::keymap::{Action, Context, KeyBinding, Keymap};

impl Action {
    /// The short word for a key hint (`Stage: <space>`).
    pub fn label(self) -> &'static str {
        match self {
            Self::StageFile => "Stage",
            Self::StageAll => "All",
            Self::Discard => "Discard",
            Self::Commit => "Commit",
            Self::Amend => "Amend",
            Self::RewordHead | Self::RewordCommit => "Reword",
            Self::Fetch => "Fetch",
            Self::Pull => "Pull",
            Self::Push => "Push",
            Self::Help => "Help",
            Self::Quit => "Quit",
            Self::Checkout => "Checkout",
            Self::NewBranch => "New",
            Self::DeleteBranch => "Delete",
            Self::DropCommit | Self::DropStash => "Drop",
            Self::FastForward => "Fast-forward",
            Self::Merge => "Merge",
            Self::ApplyStash => "Apply",
            Self::PopStash => "Pop",
            Self::Squash => "Squash",
            Self::Fixup => "Fixup",
            Self::EditCommit => "Edit",
            Self::NewFixup => "New fixup!",
            Self::Autosquash => "Autosquash",
            Self::OperationMenu => "Continue / skip / abort",
            _ => "",
        }
    }

    /// What the action does, for the help screen.
    pub fn description(self) -> &'static str {
        match self {
            Self::Quit => "quit",
            Self::Help => "toggle this help",
            Self::CommandLog => "open the command log",
            Self::OperationMenu => "continue / skip / abort a stopped operation",
            Self::ContextMenu => "more actions for this row (or right-click)",
            Self::Back => "back out of the diff or a drill",
            Self::Enter => "drill in, open a folder, or focus the diff",
            Self::EnterDiff => "focus the diff, to stage within it",
            Self::Refresh => "refresh",
            Self::Fetch => "fetch",
            Self::Pull => "pull",
            Self::Push => "push (asks first after a rewrite)",
            Self::Commit => "commit; nothing staged asks to stage all",
            Self::Amend => "amend HEAD, message pre-filled",
            Self::RewordHead => "reword HEAD's message only",
            Self::Focus(Pane::Status) => "focus Status",
            Self::Focus(Pane::Files) => "focus Files",
            Self::Focus(Pane::Branches) => "focus Branches",
            Self::Focus(Pane::Commits) => "focus Commits",
            Self::Focus(Pane::Stash) => "focus Stash",
            Self::NextPane => "focus the next pane",
            Self::PrevPane => "focus the previous pane",
            Self::ToggleBranchesTab => "Branches: switch Local / Remotes",
            Self::SelectDown => "move the selection down",
            Self::SelectUp => "move the selection up",
            Self::ScrollLineDown => "scroll the diff down a line",
            Self::ScrollLineUp => "scroll the diff up a line",
            Self::ScrollPageDown => "scroll the diff down a page",
            Self::ScrollPageUp => "scroll the diff up a page",
            Self::ScrollHalfDown => "scroll the diff down half a page",
            Self::ScrollHalfUp => "scroll the diff up half a page",
            Self::ScrollTop => "diff to the top",
            Self::ScrollBottom => "diff to the bottom",
            Self::NextHunk => "next hunk or file",
            Self::PrevHunk => "previous hunk or file",
            Self::StageFile => "stage / unstage the file",
            Self::StageAll => "stage / unstage every changed file",
            Self::Discard => "discard the change under the cursor (asks first)",
            Self::StashPush => "stash every change, with a message",
            Self::LeaveDiff => "back to the file list",
            Self::CursorDown => "cursor down",
            Self::CursorUp => "cursor up",
            Self::CursorNextHunk => "cursor to the next hunk",
            Self::CursorPrevHunk => "cursor to the previous hunk",
            Self::ToggleSelection => "start / clear a line selection",
            Self::StageCursor => "stage / unstage the hunk or lines",
            Self::Checkout => "check out the selected branch",
            Self::NewBranch => "new branch from HEAD, named in a popup",
            Self::FastForward => "fast-forward the selected branch to its upstream",
            Self::Merge => "merge the selected branch into the current one",
            Self::DeleteBranch => "delete the selected branch (asks first)",
            Self::RewordCommit => "reword the selected commit",
            Self::DropCommit => "drop the selected commit (asks first)",
            Self::Squash => "squash into the commit below, keeping messages",
            Self::Fixup => "fold into the commit below, dropping its message",
            Self::EditCommit => "stop the rebase at this commit to edit it",
            Self::NewFixup => "commit what is staged as a fixup! of this commit",
            Self::Autosquash => "fold fixup! / squash! commits above this one",
            Self::ApplyStash => "apply the stash, keeping it",
            Self::PopStash => "pop the stash",
            Self::DropStash => "drop the stash (asks first)",
        }
    }
}

/// Which hint bar to draw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bar {
    Default,
    Branches,
    Stash,
    Commits,
    /// A merge, rebase, cherry-pick or revert is stopped: `m` is the way out.
    Operation,
}

/// One hint segment: the actions whose keys it shows, each in its home context.
type Segment = &'static [(Context, Action)];

const FETCH_PULL_PUSH: Segment = &[
    (Context::Global, Action::Fetch),
    (Context::Global, Action::Pull),
    (Context::Global, Action::Push),
];
/// Always last, and the last to be dropped.
const PINNED: [Segment; 2] = [
    &[(Context::Global, Action::Help)],
    &[(Context::Global, Action::Quit)],
];

/// The segments of `bar` in order of priority; when the terminal is too narrow
/// the last ones go first.
fn body(bar: Bar) -> &'static [Segment] {
    match bar {
        Bar::Default => &[
            &[(Context::Files, Action::StageFile)],
            &[(Context::Files, Action::StageAll)],
            &[(Context::Files, Action::Discard)],
            &[(Context::Global, Action::Commit)],
            &[(Context::Global, Action::Amend)],
            &[(Context::Global, Action::RewordHead)],
            FETCH_PULL_PUSH,
        ],
        Bar::Branches => &[
            &[(Context::Branches, Action::Checkout)],
            &[(Context::Branches, Action::NewBranch)],
            &[(Context::Branches, Action::DeleteBranch)],
            &[(Context::Branches, Action::FastForward)],
            &[(Context::Branches, Action::Merge)],
            FETCH_PULL_PUSH,
        ],
        Bar::Stash => &[
            &[(Context::Stash, Action::ApplyStash)],
            &[(Context::Stash, Action::PopStash)],
            &[(Context::Stash, Action::DropStash)],
            FETCH_PULL_PUSH,
        ],
        Bar::Commits => &[
            &[(Context::Commits, Action::RewordCommit)],
            &[(Context::Commits, Action::DropCommit)],
            &[(Context::Commits, Action::Squash)],
            &[(Context::Commits, Action::Fixup)],
            &[(Context::Commits, Action::EditCommit)],
            &[(Context::Commits, Action::NewFixup)],
            &[(Context::Commits, Action::Autosquash)],
            FETCH_PULL_PUSH,
        ],
        Bar::Operation => &[
            &[(Context::Global, Action::OperationMenu)],
            &[(Context::Files, Action::StageFile)],
            &[(Context::Files, Action::StageAll)],
            &[(Context::Global, Action::Commit)],
        ],
    }
}

/// `<space>` for the space bar, otherwise the key as `KeyBinding` writes it.
fn hint_key(binding: KeyBinding) -> String {
    if binding.code == ratatui::crossterm::event::KeyCode::Char(' ')
        && !binding.ctrl
        && !binding.alt
    {
        "<space>".to_owned()
    } else {
        binding.to_string()
    }
}

/// `Label: key`, or `Fetch/Pull/Push: f/p/P` for a group. `None` when none of
/// its actions has a key (the user unbound them).
fn segment_text(keymap: &Keymap, segment: Segment) -> Option<String> {
    let parts: Vec<(&str, String)> = segment
        .iter()
        .filter_map(|&(context, action)| {
            keymap
                .keys(context, action)
                .first()
                .map(|&key| (action.label(), hint_key(key)))
        })
        .collect();
    if parts.is_empty() {
        return None;
    }
    let labels: Vec<&str> = parts.iter().map(|(label, _)| *label).collect();
    let keys: Vec<&str> = parts.iter().map(|(_, key)| key.as_str()).collect();
    Some(format!("{}: {}", labels.join("/"), keys.join("/")))
}

/// The hint line for `bar`, `Label: key | Label: key`, no wider than `width`
/// cells: segments are dropped from the end of the body until it fits, then
/// the pinned `Help` and `Quit` (last one first).
pub fn keybar_text(keymap: &Keymap, bar: Bar, width: usize) -> String {
    let mut segments: Vec<String> = body(bar)
        .iter()
        .filter_map(|&segment| segment_text(keymap, segment))
        .collect();
    let mut pinned: Vec<String> = PINNED
        .iter()
        .filter_map(|&segment| segment_text(keymap, segment))
        .collect();
    let joined = |body: &[String], pinned: &[String]| -> String {
        body.iter()
            .chain(pinned.iter())
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(" | ")
    };
    while UnicodeWidthStr::width(joined(&segments, &pinned).as_str()) > width
        && !segments.is_empty()
    {
        segments.pop();
    }
    while UnicodeWidthStr::width(joined(&segments, &pinned).as_str()) > width && !pinned.is_empty()
    {
        pinned.pop();
    }
    joined(&segments, &pinned)
}

/// One line of the help screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HelpLine {
    Heading(String),
    /// `keys` is left-aligned in a column, `text` beside it.
    Entry {
        keys: String,
        text: String,
    },
    Blank,
}

/// The help screen for a focused pane: its own context (and the diff cursor's
/// while it is up), then the global one, then the keys that are not
/// remappable. Actions with no key (unbound by the user) are left out.
pub fn help_lines(keymap: &Keymap, contexts: &[Context]) -> Vec<HelpLine> {
    let mut lines = Vec::new();
    for &context in contexts {
        let mut entries = Vec::new();
        for action in Keymap::actions_of(context) {
            let keys = keymap.keys(context, action);
            if keys.is_empty() {
                continue;
            }
            entries.push(HelpLine::Entry {
                keys: keys
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", "),
                text: action.description().to_owned(),
            });
        }
        if entries.is_empty() {
            continue;
        }
        if !lines.is_empty() {
            lines.push(HelpLine::Blank);
        }
        lines.push(HelpLine::Heading(format!("{} keys", context.title())));
        lines.extend(entries);
    }
    lines.push(HelpLine::Blank);
    lines.push(HelpLine::Heading("Fixed keys (not remappable)".to_owned()));
    for (keys, text) in FIXED_KEYS {
        lines.push(HelpLine::Entry {
            keys: (*keys).to_owned(),
            text: (*text).to_owned(),
        });
    }
    lines
}

/// Keys that belong to a popup, a confirm or the terminal, not the keymap.
const FIXED_KEYS: &[(&str, &str)] = &[
    ("ctrl-c", "quit, from anywhere"),
    ("y / n", "answer a confirm"),
    ("enter / esc", "in a popup: confirm / cancel"),
    ("tab", "commit popup: switch summary / description"),
    (
        "ctrl-o / ctrl-n",
        "commit popup: toggle sign-off / no-verify",
    ),
    (
        "ctrl-s / meta-enter",
        "commit popup: commit from either field",
    ),
    (
        "j / k, enter, a letter",
        "in a menu: move, run, run that row",
    ),
    (
        "wheel / click",
        "scroll the pane under it / focus a row there",
    ),
];
