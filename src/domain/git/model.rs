//! Owned repository concepts shared by Git access, app state, and rendering.
//! These types contain no `git2` or terminal types; `crate::domain::git` adapts to them.

use std::path::PathBuf;

/// A multi-step operation git has stopped in the middle of, waiting for the
/// user. Read from the repository state, so it is also true for one started
/// from another shell. See `docs/PLAN_11_REBASE.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    Merge,
    /// `step` of `total` commits; both `0` when git's progress files are
    /// missing or unreadable.
    Rebase {
        step: usize,
        total: usize,
    },
    CherryPick,
    Revert,
}

impl Operation {
    /// The lower-case name, for sentences ("abort the rebase?").
    pub fn noun(self) -> &'static str {
        match self {
            Self::Merge => "merge",
            Self::Rebase { .. } => "rebase",
            Self::CherryPick => "cherry-pick",
            Self::Revert => "revert",
        }
    }

    /// The badge shown in the Status pane.
    pub fn label(self) -> String {
        match self {
            Self::Merge => "MERGING".to_owned(),
            Self::Rebase { step, total } if total > 0 => format!("REBASING {step}/{total}"),
            Self::Rebase { .. } => "REBASING".to_owned(),
            Self::CherryPick => "CHERRY-PICKING".to_owned(),
            Self::Revert => "REVERTING".to_owned(),
        }
    }
}

/// One-glance summary of repository state.
#[derive(Debug, Clone, Default)]
pub struct StatusHeader {
    pub branch: String,
    pub detached: bool,
    pub upstream: Option<String>,
    pub ahead: usize,
    pub behind: usize,
    pub conflicts: usize,
}

/// How one side (index or worktree) of a path changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    None,
    Modified,
    Added,
    Deleted,
    Renamed,
    Typechange,
    Untracked,
    Conflicted,
}

impl Change {
    /// The single-letter code `git status --porcelain` prints.
    pub fn code(self) -> char {
        match self {
            Self::None => ' ',
            Self::Modified => 'M',
            Self::Added => 'A',
            Self::Deleted => 'D',
            Self::Renamed => 'R',
            Self::Typechange => 'T',
            Self::Untracked => '?',
            Self::Conflicted => 'U',
        }
    }
}

/// One working-tree path and its staged/worktree changes.
#[derive(Debug, Clone)]
pub struct FileEntry {
    pub path: PathBuf,
    pub staged: Change,
    pub worktree: Change,
    pub binary: bool,
}

impl FileEntry {
    /// `XY path`, the way porcelain lays it out.
    pub fn display(&self) -> String {
        format!(
            "{}{} {}",
            self.staged.code(),
            self.worktree.code(),
            self.path.display()
        )
    }
}

/// One configured remote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteEntry {
    pub name: String,
    pub fetch_url: String,
    pub push_url: String,
}

/// One local branch row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchEntry {
    pub name: String,
    pub is_head: bool,
    pub upstream: Option<String>,
    pub ahead: usize,
    pub behind: usize,
    pub tip_time: i64,
}

/// What a name that points at a commit is, for the `(HEAD -> main, tag: v1, origin/main)`
/// decoration and for a tag shown in the commit list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommitRefKind {
    /// `HEAD -> main`, or a bare `HEAD` when detached.
    Head,
    Branch,
    Tag,
    Remote,
}

/// One name pointing at a commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitRef {
    /// As `git log --decorate` writes it: `HEAD -> main`, `feat/x`, `tag: v0.6.0`, `origin/main`.
    pub label: String,
    pub kind: CommitRefKind,
}

/// Where a commit stands relative to the remote, lazygit's hash colours: red
/// not pushed yet, yellow pushed, green merged into the remote's main branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PushState {
    #[default]
    Unpushed,
    Pushed,
    Merged,
}

/// One commit row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitEntry {
    pub full_hash: String,
    pub short_hash: String,
    pub author: String,
    pub summary: String,
    pub time: i64,
    /// Names pointing at this commit, in `git log --decorate` order: `HEAD ->` and the
    /// branches, then tags, then remote branches. Empty when none, and in a branch's log.
    pub refs: Vec<CommitRef>,
    pub push_state: PushState,
}

impl CommitEntry {
    /// `(HEAD -> main, tag: v0.6.0, origin/main)`, `None` when nothing points here.
    pub fn decoration(&self) -> Option<String> {
        if self.refs.is_empty() {
            return None;
        }
        let labels: Vec<&str> = self.refs.iter().map(|r| r.label.as_str()).collect();
        Some(format!("({})", labels.join(", ")))
    }

    /// The tag names on this commit, without the `tag: ` prefix.
    pub fn tags(&self) -> impl Iterator<Item = &str> {
        self.refs
            .iter()
            .filter(|r| r.kind == CommitRefKind::Tag)
            .map(|r| r.label.trim_start_matches("tag: "))
    }

    /// Up to two uppercase initials from the author name.
    pub fn author_initials(&self) -> String {
        let mut initials: String = self
            .author
            .split_whitespace()
            .filter_map(|word| word.chars().next())
            .map(|c| c.to_ascii_uppercase())
            .take(2)
            .collect();
        if initials.is_empty() {
            initials.push('?');
        }
        initials
    }
}

/// One stash row. The OID is stable when stack indices shift.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StashEntry {
    pub index: usize,
    pub oid: String,
    pub message: String,
}
