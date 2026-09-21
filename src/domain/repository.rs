//! Owned repository concepts shared by Git access, app state, and rendering.
//! These types contain no `git2` or terminal types; `crate::domain::git` adapts to them.

use std::path::PathBuf;

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

/// One commit row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitEntry {
    pub full_hash: String,
    pub short_hash: String,
    pub author: String,
    pub summary: String,
    pub time: i64,
}

impl CommitEntry {
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
