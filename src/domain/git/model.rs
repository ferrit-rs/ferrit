//! Owned repository concepts shared by Git access, app state, and rendering.
//! These types contain no `git2` or terminal types; `crate::domain::git` adapts to them.

use std::path::PathBuf;

/// A multi-step operation git has stopped in the middle of, waiting for the
/// user. Read from the repository state, so it is also true for one started
/// from another shell. See `docs/PLAN_11_REBASE.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    /// A merge that stopped, usually on a conflict.
    Merge,
    /// `step` of `total` commits; both `0` when git's progress files are
    /// missing or unreadable.
    Rebase {
        /// The commit being applied, counting from 1.
        step: usize,
        /// How many commits the rebase applies.
        total: usize,
    },
    /// A `git cherry-pick` that stopped.
    CherryPick,
    /// A `git revert` that stopped.
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
    /// The current branch's name, or `HEAD` when detached.
    pub branch: String,
    /// `HEAD` points at a commit rather than a branch.
    pub detached: bool,
    /// The branch's upstream, as `origin/main`, if it has one.
    pub upstream: Option<String>,
    /// Commits on the branch that its upstream does not have.
    pub ahead: usize,
    /// Commits on the upstream that the branch does not have.
    pub behind: usize,
    /// Paths with an unresolved conflict.
    pub conflicts: usize,
}

/// How one side (index or worktree) of a path changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    /// Nothing on this side.
    None,
    /// The content changed.
    Modified,
    /// The path is new.
    Added,
    /// The path is gone.
    Deleted,
    /// The path was renamed.
    Renamed,
    /// The type changed (a file became a symlink, say).
    Typechange,
    /// Not tracked by git yet. Only ever a worktree change.
    Untracked,
    /// An unresolved merge conflict.
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
    /// The path relative to the worktree root.
    pub path: PathBuf,
    /// What the index holds against `HEAD`.
    pub staged: Change,
    /// What the worktree holds against the index.
    pub worktree: Change,
    /// Whether git treats the file as binary. Not read yet: always `false`.
    pub binary: bool,
}

impl FileEntry {
    /// Some of the path's changes sit in the index.
    pub fn has_staged(&self) -> bool {
        !matches!(
            self.staged,
            Change::None | Change::Untracked | Change::Conflicted
        )
    }

    /// Staged and nothing left in the worktree (`M `, `A `), lazygit's
    /// "fully staged": a file that is also modified again (`MM`) is not.
    pub fn is_fully_staged(&self) -> bool {
        self.has_staged() && self.worktree == Change::None
    }

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
    /// The remote's name, usually `origin`.
    pub name: String,
    /// The URL fetches use.
    pub fetch_url: String,
    /// The URL pushes use, which can differ from the fetch URL.
    pub push_url: String,
}

/// One local branch row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BranchEntry {
    /// The branch's name, without `refs/heads/`.
    pub name: String,
    /// It is the checked-out branch.
    pub is_head: bool,
    /// The upstream, as `origin/main`, if there is one.
    pub upstream: Option<String>,
    /// Commits the branch has that its upstream does not.
    pub ahead: usize,
    /// Commits the upstream has that the branch does not.
    pub behind: usize,
    /// Unix time of the tip commit, in seconds. Branches are listed newest first.
    pub tip_time: i64,
}

/// What a name that points at a commit is, for the `(HEAD -> main, tag: v1, origin/main)`
/// decoration and for a tag shown in the commit list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommitRefKind {
    /// `HEAD -> main`, or a bare `HEAD` when detached.
    Head,
    /// A local branch.
    Branch,
    /// A tag.
    Tag,
    /// A remote-tracking branch.
    Remote,
}

/// One name pointing at a commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitRef {
    /// As `git log --decorate` writes it: `HEAD -> main`, `feat/x`, `tag: v0.6.0`, `origin/main`.
    pub label: String,
    /// What kind of name it is.
    pub kind: CommitRefKind,
}

/// Where a commit stands relative to the remote, lazygit's hash colours: red
/// not pushed yet, yellow pushed, green merged into the remote's main branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PushState {
    /// Only on this machine.
    #[default]
    Unpushed,
    /// On the remote.
    Pushed,
    /// Merged into the remote's main branch.
    Merged,
}

/// One commit row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitEntry {
    /// The full hash, 40 hex characters.
    pub full_hash: String,
    /// The abbreviated hash git shows by default, 7 characters.
    pub short_hash: String,
    /// The author's name.
    pub author: String,
    /// The author's email, `Name <email>` in the branch Log header; empty when unknown.
    pub author_email: String,
    /// The first line of the message.
    pub summary: String,
    /// The message under the subject, trimmed; empty for a subject-only commit.
    pub body: String,
    /// Unix time of the commit, in seconds.
    pub time: i64,
    /// Names pointing at this commit, in `git log --decorate` order: `HEAD ->` and the
    /// branches, then tags, then remote branches. Empty when none, and in a branch's log.
    pub refs: Vec<CommitRef>,
    /// Where the commit stands against the remote.
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
    /// The stash's position in the stack, 0 for the newest. It shifts when an entry is dropped; `oid` does not.
    pub index: usize,
    /// The stash commit's id.
    pub oid: String,
    /// The message git recorded, such as `On main: wip`.
    pub message: String,
}
