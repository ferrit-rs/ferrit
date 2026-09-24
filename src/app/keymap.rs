//! Which key does what: `Action`s, the `Context`s that group them, and the
//! `Keymap` that maps `(Context, KeyBinding)` to an `Action`. See
//! `docs/PLAN_12_POLISH.md` P2.
//!
//! The default keymap is exactly the bindings `on_key` had before it existed
//! (`tests/keymap.rs` lists them). One deliberate difference: modifiers now
//! have to match. `on_key` used to ignore them for character keys, so
//! `Ctrl-d` outside a diff fell through to `d` and opened the discard
//! prompt; a binding now fires only for the exact modifiers it names.
//!
//! Not part of the keymap, on purpose: `Ctrl-c` (always quits), typing inside a
//! popup, `Esc` / `Enter` inside a popup, the `y` / `n` confirm answers. A user
//! who rebinds `y` must not be able to make a destructive confirm
//! unanswerable.

use std::collections::HashMap;
use std::collections::hash_map::Entry as MapEntry;
use std::fmt;

use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use super::Pane;
use super::config::KeyOverrides;

/// Where a binding applies. `resolve` tries the most specific context first,
/// then `Global`, which is how `d` means discard on Files, delete on Branches
/// and drop on Commits without a special case.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Context {
    Global,
    Files,
    /// `Mode::Diff`: the cursor is inside the diff, staging by hunk or line.
    Diff,
    Branches,
    Commits,
    Stash,
}

impl Context {
    /// A heading for the help screen.
    pub fn title(self) -> &'static str {
        match self {
            Self::Global => "Global",
            Self::Files => "Files",
            Self::Diff => "Diff cursor",
            Self::Branches => "Branches",
            Self::Commits => "Commits",
            Self::Stash => "Stash",
        }
    }

    /// The `[keys.<name>]` table name.
    pub fn name(self) -> &'static str {
        match self {
            Self::Global => "global",
            Self::Files => "files",
            Self::Diff => "diff",
            Self::Branches => "branches",
            Self::Commits => "commits",
            Self::Stash => "stash",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        [
            Self::Global,
            Self::Files,
            Self::Diff,
            Self::Branches,
            Self::Commits,
            Self::Stash,
        ]
        .into_iter()
        .find(|context| context.name() == name)
    }

    /// The context of a focused pane, if it has bindings of its own.
    pub fn for_pane(pane: Pane) -> Option<Self> {
        match pane {
            Pane::Status => None,
            Pane::Files => Some(Self::Files),
            Pane::Branches => Some(Self::Branches),
            Pane::Commits => Some(Self::Commits),
            Pane::Stash => Some(Self::Stash),
        }
    }
}

/// Everything a key can do outside a popup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    // Global.
    Quit,
    Help,
    CommandLog,
    OperationMenu,
    /// Leave a drill or the right pane.
    Back,
    /// Drill into the selection, toggle a directory, or focus the diff.
    Enter,
    EnterDiff,
    Refresh,
    Fetch,
    Pull,
    Push,
    Commit,
    Amend,
    /// Reword `HEAD`'s message only.
    RewordHead,
    Focus(Pane),
    NextPane,
    PrevPane,
    ToggleBranchesTab,
    SelectDown,
    SelectUp,
    // Scrolling the right pane when it shows a diff.
    ScrollLineDown,
    ScrollLineUp,
    ScrollPageDown,
    ScrollPageUp,
    ScrollHalfDown,
    ScrollHalfUp,
    ScrollTop,
    ScrollBottom,
    NextHunk,
    PrevHunk,
    // Files.
    StageFile,
    StageAll,
    Discard,
    StashPush,
    // Diff mode (cursor inside the diff).
    LeaveDiff,
    CursorDown,
    CursorUp,
    CursorNextHunk,
    CursorPrevHunk,
    ToggleSelection,
    StageCursor,
    // Branches.
    Checkout,
    NewBranch,
    FastForward,
    Merge,
    DeleteBranch,
    // Commits.
    /// Reword the selected commit (`HEAD` amends, an older one rebases).
    RewordCommit,
    DropCommit,
    Squash,
    Fixup,
    EditCommit,
    NewFixup,
    Autosquash,
    // Stash.
    ApplyStash,
    PopStash,
    DropStash,
}

impl Action {
    /// The name used in `[keys.<context>]`: `stage_file`, `focus_files`, ...
    pub fn name(self) -> &'static str {
        match self {
            Self::Quit => "quit",
            Self::Help => "help",
            Self::CommandLog => "command_log",
            Self::OperationMenu => "operation_menu",
            Self::Back => "back",
            Self::Enter => "enter",
            Self::EnterDiff => "enter_diff",
            Self::Refresh => "refresh",
            Self::Fetch => "fetch",
            Self::Pull => "pull",
            Self::Push => "push",
            Self::Commit => "commit",
            Self::Amend => "amend",
            Self::RewordHead => "reword_head",
            Self::Focus(Pane::Status) => "focus_status",
            Self::Focus(Pane::Files) => "focus_files",
            Self::Focus(Pane::Branches) => "focus_branches",
            Self::Focus(Pane::Commits) => "focus_commits",
            Self::Focus(Pane::Stash) => "focus_stash",
            Self::NextPane => "next_pane",
            Self::PrevPane => "prev_pane",
            Self::ToggleBranchesTab => "toggle_branches_tab",
            Self::SelectDown => "select_down",
            Self::SelectUp => "select_up",
            Self::ScrollLineDown => "scroll_line_down",
            Self::ScrollLineUp => "scroll_line_up",
            Self::ScrollPageDown => "scroll_page_down",
            Self::ScrollPageUp => "scroll_page_up",
            Self::ScrollHalfDown => "scroll_half_down",
            Self::ScrollHalfUp => "scroll_half_up",
            Self::ScrollTop => "scroll_top",
            Self::ScrollBottom => "scroll_bottom",
            Self::NextHunk => "next_hunk",
            Self::PrevHunk => "prev_hunk",
            Self::StageFile => "stage_file",
            Self::StageAll => "stage_all",
            Self::Discard => "discard",
            Self::StashPush => "stash_push",
            Self::LeaveDiff => "leave_diff",
            Self::CursorDown => "cursor_down",
            Self::CursorUp => "cursor_up",
            Self::CursorNextHunk => "cursor_next_hunk",
            Self::CursorPrevHunk => "cursor_prev_hunk",
            Self::ToggleSelection => "toggle_selection",
            Self::StageCursor => "stage_cursor",
            Self::Checkout => "checkout",
            Self::NewBranch => "new_branch",
            Self::FastForward => "fast_forward",
            Self::Merge => "merge",
            Self::DeleteBranch => "delete_branch",
            Self::RewordCommit => "reword_commit",
            Self::DropCommit => "drop_commit",
            Self::Squash => "squash",
            Self::Fixup => "fixup",
            Self::EditCommit => "edit_commit",
            Self::NewFixup => "new_fixup",
            Self::Autosquash => "autosquash",
            Self::ApplyStash => "apply_stash",
            Self::PopStash => "pop_stash",
            Self::DropStash => "drop_stash",
        }
    }

    /// The action called `name`. Every action has a default binding, so the
    /// defaults list is the list of actions.
    pub fn from_name(name: &str) -> Option<Self> {
        DEFAULTS
            .iter()
            .map(|&(_, _, action)| action)
            .find(|action| action.name() == name)
    }
}

/// A key and the modifiers that matter. `Shift` is not tracked: a character's
/// case already says it, and `BackTab` is its own key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyBinding {
    pub code: KeyCode,
    pub ctrl: bool,
    pub alt: bool,
}

impl KeyBinding {
    pub fn of(code: KeyCode) -> Self {
        Self::new(code, false, false)
    }

    /// With `ctrl` or `alt`, a letter's case means nothing (a terminal sends
    /// `Ctrl-D` as `d` with the control modifier, or as `D` with shift too),
    /// so it is folded to lower case; a bare character keeps its case.
    fn new(code: KeyCode, ctrl: bool, alt: bool) -> Self {
        let code = match code {
            KeyCode::Char(c) if ctrl || alt => KeyCode::Char(c.to_ascii_lowercase()),
            other => other,
        };
        Self { code, ctrl, alt }
    }

    /// The binding a key event triggers.
    pub fn from_event(event: KeyEvent) -> Self {
        Self::new(
            event.code,
            event.modifiers.contains(KeyModifiers::CONTROL),
            event.modifiers.contains(KeyModifiers::ALT),
        )
    }

    /// `q`, `J`, `ctrl-d`, `alt-x`, `enter`, `esc`, `space`, `tab`, `backtab`,
    /// `up`, `down`, `left`, `right`, `pgup`, `pgdn`, `home`, `end`,
    /// `backspace`, `delete`, `insert` and `f1` to `f12`. Case-insensitive
    /// except for a single character, which is taken as typed. `None` for
    /// anything else, including a modifier on a named key that has none in
    /// terminals (only `ctrl` and `alt` exist).
    pub fn parse(text: &str) -> Option<Self> {
        let mut rest = text;
        let (mut ctrl, mut alt) = (false, false);
        loop {
            let lower = rest.to_ascii_lowercase();
            if let Some(tail) = lower.strip_prefix("ctrl-") {
                ctrl = true;
                rest = rest.get(rest.len() - tail.len()..)?;
            } else if let Some(tail) = lower.strip_prefix("alt-") {
                alt = true;
                rest = rest.get(rest.len() - tail.len()..)?;
            } else {
                break;
            }
        }
        let mut chars = rest.chars();
        let code = match (chars.next(), chars.next()) {
            (Some(c), None) => KeyCode::Char(c),
            _ => named_key(&rest.to_ascii_lowercase())?,
        };
        Some(Self::new(code, ctrl, alt))
    }
}

fn named_key(name: &str) -> Option<KeyCode> {
    Some(match name {
        "enter" => KeyCode::Enter,
        "esc" | "escape" => KeyCode::Esc,
        "space" => KeyCode::Char(' '),
        "tab" => KeyCode::Tab,
        "backtab" | "shift-tab" => KeyCode::BackTab,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "pgup" | "pageup" => KeyCode::PageUp,
        "pgdn" | "pagedown" => KeyCode::PageDown,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "backspace" => KeyCode::Backspace,
        "delete" => KeyCode::Delete,
        "insert" => KeyCode::Insert,
        other => {
            let number: u8 = other.strip_prefix('f')?.parse().ok()?;
            if !(1..=12).contains(&number) {
                return None;
            }
            KeyCode::F(number)
        },
    })
}

impl fmt::Display for KeyBinding {
    /// The form `parse` reads back: `ctrl-d`, `space`, `pgdn`, `J`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.ctrl {
            f.write_str("ctrl-")?;
        }
        if self.alt {
            f.write_str("alt-")?;
        }
        match self.code {
            KeyCode::Char(' ') => f.write_str("space"),
            KeyCode::Char(c) => write!(f, "{c}"),
            KeyCode::Enter => f.write_str("enter"),
            KeyCode::Esc => f.write_str("esc"),
            KeyCode::Tab => f.write_str("tab"),
            KeyCode::BackTab => f.write_str("backtab"),
            KeyCode::Up => f.write_str("up"),
            KeyCode::Down => f.write_str("down"),
            KeyCode::Left => f.write_str("left"),
            KeyCode::Right => f.write_str("right"),
            KeyCode::PageUp => f.write_str("pgup"),
            KeyCode::PageDown => f.write_str("pgdn"),
            KeyCode::Home => f.write_str("home"),
            KeyCode::End => f.write_str("end"),
            KeyCode::Backspace => f.write_str("backspace"),
            KeyCode::Delete => f.write_str("delete"),
            KeyCode::Insert => f.write_str("insert"),
            KeyCode::F(n) => write!(f, "f{n}"),
            _ => f.write_str("?"),
        }
    }
}

/// `(context, key, action)` for every default binding.
const DEFAULTS: &[(Context, &str, Action)] = &[
    (Context::Global, "q", Action::Quit),
    (Context::Global, "?", Action::Help),
    (Context::Global, "@", Action::CommandLog),
    (Context::Global, "m", Action::OperationMenu),
    (Context::Global, "esc", Action::Back),
    (Context::Global, "enter", Action::Enter),
    (Context::Global, "l", Action::EnterDiff),
    (Context::Global, "r", Action::Refresh),
    (Context::Global, "f", Action::Fetch),
    (Context::Global, "p", Action::Pull),
    (Context::Global, "P", Action::Push),
    (Context::Global, "c", Action::Commit),
    (Context::Global, "A", Action::Amend),
    (Context::Global, "w", Action::RewordHead),
    (Context::Global, "1", Action::Focus(Pane::Status)),
    (Context::Global, "2", Action::Focus(Pane::Files)),
    (Context::Global, "3", Action::Focus(Pane::Branches)),
    (Context::Global, "4", Action::Focus(Pane::Commits)),
    (Context::Global, "5", Action::Focus(Pane::Stash)),
    (Context::Global, "tab", Action::NextPane),
    (Context::Global, "right", Action::NextPane),
    (Context::Global, "backtab", Action::PrevPane),
    (Context::Global, "left", Action::PrevPane),
    (Context::Global, "ctrl-right", Action::ToggleBranchesTab),
    (Context::Global, "ctrl-left", Action::ToggleBranchesTab),
    (Context::Global, "j", Action::SelectDown),
    (Context::Global, "down", Action::SelectDown),
    (Context::Global, "k", Action::SelectUp),
    (Context::Global, "up", Action::SelectUp),
    (Context::Global, "J", Action::ScrollLineDown),
    (Context::Global, "K", Action::ScrollLineUp),
    (Context::Global, "pgdn", Action::ScrollPageDown),
    (Context::Global, "pgup", Action::ScrollPageUp),
    (Context::Global, "ctrl-d", Action::ScrollHalfDown),
    (Context::Global, "ctrl-u", Action::ScrollHalfUp),
    (Context::Global, "<", Action::ScrollTop),
    (Context::Global, ">", Action::ScrollBottom),
    (Context::Global, "]", Action::NextHunk),
    (Context::Global, "[", Action::PrevHunk),
    (Context::Files, "space", Action::StageFile),
    (Context::Files, "a", Action::StageAll),
    (Context::Files, "d", Action::Discard),
    (Context::Files, "s", Action::StashPush),
    (Context::Diff, "esc", Action::LeaveDiff),
    (Context::Diff, "h", Action::LeaveDiff),
    (Context::Diff, "j", Action::CursorDown),
    (Context::Diff, "down", Action::CursorDown),
    (Context::Diff, "k", Action::CursorUp),
    (Context::Diff, "up", Action::CursorUp),
    (Context::Diff, "]", Action::CursorNextHunk),
    (Context::Diff, "[", Action::CursorPrevHunk),
    (Context::Diff, "V", Action::ToggleSelection),
    (Context::Diff, "space", Action::StageCursor),
    (Context::Diff, "d", Action::Discard),
    (Context::Branches, "space", Action::Checkout),
    (Context::Branches, "n", Action::NewBranch),
    (Context::Branches, "u", Action::FastForward),
    (Context::Branches, "M", Action::Merge),
    (Context::Branches, "d", Action::DeleteBranch),
    (Context::Commits, "w", Action::RewordCommit),
    (Context::Commits, "d", Action::DropCommit),
    (Context::Commits, "s", Action::Squash),
    (Context::Commits, "S", Action::Fixup),
    (Context::Commits, "e", Action::EditCommit),
    (Context::Commits, "F", Action::NewFixup),
    (Context::Commits, "a", Action::Autosquash),
    (Context::Stash, "space", Action::ApplyStash),
    (Context::Stash, "g", Action::PopStash),
    (Context::Stash, "d", Action::DropStash),
];

/// One `[keys]` entry that passed the first checks.
struct Override {
    context: Context,
    action: Action,
    keys: Vec<KeyBinding>,
    label: String,
}

/// `Ctrl-c` quits from anywhere and is not in the keymap, so it cannot be
/// rebound to anything else either.
const RESERVED_QUIT: KeyBinding = KeyBinding {
    code: KeyCode::Char('c'),
    ctrl: true,
    alt: false,
};

/// `(context, key) -> action`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keymap {
    bindings: HashMap<(Context, KeyBinding), Action>,
}

impl Default for Keymap {
    fn default() -> Self {
        let bindings = DEFAULTS
            .iter()
            .filter_map(|&(context, key, action)| {
                KeyBinding::parse(key).map(|binding| ((context, binding), action))
            })
            .collect();
        Self { bindings }
    }
}

impl Keymap {
    /// The action `key` triggers, trying each context in order (most specific
    /// first). `None`: unbound.
    pub fn resolve(&self, contexts: &[Context], key: KeyBinding) -> Option<Action> {
        contexts
            .iter()
            .find_map(|&context| self.bindings.get(&(context, key)).copied())
    }

    /// The default keymap with the user's `[keys]` applied, and one issue per
    /// entry that could not be. An entry is `[keys.<context>] <action> = "<key>"`
    /// (or a list of keys): it replaces that action's default keys in that
    /// context.
    ///
    /// An entry that is wrong keeps its action's defaults and is reported: an
    /// unknown context or action, an action used outside the contexts it lives
    /// in, a key that does not parse, `ctrl-c` (which always quits), or a key
    /// already taken by another action in that context. The rest still apply.
    /// Every overridden action's defaults are removed before any new key is
    /// added, so two actions can swap keys. Entries apply in name order, so a
    /// clash between two entries is decided the same way every run.
    pub fn from_overrides(overrides: &KeyOverrides) -> (Self, Vec<String>) {
        let mut issues = Vec::new();
        let mut entries: Vec<Override> = Vec::new();
        for (context_name, actions) in overrides {
            let Some(context) = Context::from_name(context_name) else {
                issues.push(format!("unknown key context `keys.{context_name}` ignored"));
                continue;
            };
            for (action_name, list) in actions {
                let label = format!("keys.{context_name}.{action_name}");
                let Some(action) = Action::from_name(action_name) else {
                    issues.push(format!("`{label}`: unknown action, ignored"));
                    continue;
                };
                if !DEFAULTS
                    .iter()
                    .any(|&(c, _, a)| c == context && a == action)
                {
                    issues.push(format!(
                        "`{label}`: `{action_name}` is not an action of the `{context_name}` context, ignored"
                    ));
                    continue;
                }
                let mut keys = Vec::new();
                let mut valid = true;
                for text in list.texts() {
                    match KeyBinding::parse(text) {
                        None => {
                            issues.push(format!(
                                "`{label}`: `{text}` is not a key, keeping the default"
                            ));
                            valid = false;
                        },
                        Some(binding) if binding == RESERVED_QUIT => {
                            issues.push(format!("`{label}`: ctrl-c always quits and cannot be bound, keeping the default"));
                            valid = false;
                        },
                        Some(binding) if !keys.contains(&binding) => keys.push(binding),
                        Some(_) => {},
                    }
                }
                if valid {
                    entries.push(Override {
                        context,
                        action,
                        keys,
                        label,
                    });
                }
            }
        }

        let mut map = Self::default().bindings;
        map.retain(|&(context, _), action| {
            !entries
                .iter()
                .any(|e| e.context == context && e.action == *action)
        });
        let mut rejected: Vec<&Override> = Vec::new();
        for entry in &entries {
            let clash = entry.keys.iter().find_map(|&key| {
                map.get(&(entry.context, key))
                    .filter(|&&other| other != entry.action)
                    .map(|&other| (key, other))
            });
            match clash {
                Some((key, other)) => {
                    issues.push(format!(
                        "`{}`: `{key}` is already bound to `{}` there, keeping the default",
                        entry.label,
                        other.name()
                    ));
                    rejected.push(entry);
                },
                None => {
                    for &key in &entry.keys {
                        map.insert((entry.context, key), entry.action);
                    }
                },
            }
        }
        for entry in rejected {
            for &(context, text, action) in DEFAULTS {
                if context != entry.context || action != entry.action {
                    continue;
                }
                let Some(key) = KeyBinding::parse(text) else {
                    continue;
                };
                match map.entry((context, key)) {
                    MapEntry::Occupied(_) => issues.push(format!(
                        "`{}`: its default `{key}` is taken too, so `{}` is unbound",
                        entry.label,
                        action.name()
                    )),
                    MapEntry::Vacant(slot) => {
                        slot.insert(action);
                    },
                }
            }
        }
        (Self { bindings: map }, issues)
    }

    /// The keys `action` has in `context`, in the order the defaults list them
    /// and then by name, so the order never depends on hashing.
    pub fn keys(&self, context: Context, action: Action) -> Vec<KeyBinding> {
        let mut keys: Vec<KeyBinding> = self
            .bindings
            .iter()
            .filter(|&(&(c, _), &a)| c == context && a == action)
            .map(|(&(_, key), _)| key)
            .collect();
        let rank = |key: KeyBinding| {
            DEFAULTS
                .iter()
                .position(|&(c, text, a)| {
                    c == context && a == action && KeyBinding::parse(text) == Some(key)
                })
                .unwrap_or(usize::MAX)
        };
        keys.sort_by_key(|&key| (rank(key), key.to_string()));
        keys
    }

    /// The actions that live in `context`, in the order the defaults list them.
    pub fn actions_of(context: Context) -> Vec<Action> {
        let mut actions: Vec<Action> = Vec::new();
        for &(c, _, action) in DEFAULTS {
            if c == context && !actions.contains(&action) {
                actions.push(action);
            }
        }
        actions
    }

    /// Every binding, for help and tests.
    pub fn bindings(&self) -> impl Iterator<Item = (Context, KeyBinding, Action)> + '_ {
        self.bindings
            .iter()
            .map(|(&(context, key), &action)| (context, key, action))
    }
}
