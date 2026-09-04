//! Plain owned data the left panes render. No `git2` type, no `ratatui` type.
//!
//! Phase 1's lazygit re-skin builds these from `mock`; phase 2 milestones
//! G3..G5 fill the same structs from real git reads (`refs.rs`, `log.rs`,
//! `stash.rs`). The UI never learns which source it got.

/// One row of the Commits pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitEntry {
    /// Abbreviated hash, 7 or 8 hex chars.
    pub short_hash: String,
    /// Full author name, e.g. `Max Wells`.
    pub author: String,
    /// First line of the commit message.
    pub summary: String,
    /// Commit time, seconds since the epoch. Raw until a date crate lands.
    pub time: i64,
}

impl CommitEntry {
    /// Up to two uppercase initials from the author name, for the lazygit-style
    /// `MW` column. `"Max Wells"` -> `"MW"`, `"cottreau"` -> `"C"`.
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

/// One row of the Local Branches pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchEntry {
    pub name: String,
    /// The currently checked-out branch (`*` in lazygit).
    pub is_head: bool,
    /// Upstream ref, e.g. `origin/main`.
    pub upstream: Option<String>,
    pub ahead: usize,
    pub behind: usize,
}

/// One row of the Stash pane.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StashEntry {
    /// Stack position: `stash@{index}`.
    pub index: usize,
    pub message: String,
}
