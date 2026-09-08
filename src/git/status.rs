//! Repo status: the header line data and the working-tree file list.
//!
//! All types here are plain owned values. No `git2` type escapes this module.

use std::path::PathBuf;

use git2::{ErrorCode, Repository, Status, StatusOptions};

use crate::git::error::{GitError, GitResult};

/// One-glance summary of where the repo stands.
#[derive(Debug, Clone, Default)]
pub struct StatusHeader {
    /// Branch short name, or the short hash when detached.
    pub branch: String,
    pub detached: bool,
    /// Upstream ref name, e.g. `origin/main`.
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
            Change::None => ' ',
            Change::Modified => 'M',
            Change::Added => 'A',
            Change::Deleted => 'D',
            Change::Renamed => 'R',
            Change::Typechange => 'T',
            Change::Untracked => '?',
            Change::Conflicted => 'U',
        }
    }
}

/// One entry of the working tree that differs from a clean state.
#[derive(Debug, Clone)]
pub struct FileEntry {
    pub path: PathBuf,
    /// Index vs HEAD (the staged change).
    pub staged: Change,
    /// Worktree vs index (the unstaged change).
    pub worktree: Change,
    pub binary: bool,
}

impl FileEntry {
    /// `XY path`, the way porcelain lays it out: X = staged, Y = worktree.
    pub fn display(&self) -> String {
        format!(
            "{}{} {}",
            self.staged.code(),
            self.worktree.code(),
            self.path.display()
        )
    }
}

/// Read the header: branch, upstream, ahead/behind, conflict count.
pub fn header(repo: &Repository) -> GitResult<StatusHeader> {
    let mut out = StatusHeader::default();

    match repo.head() {
        Ok(head) => {
            out.detached = repo.head_detached().unwrap_or(false);
            let local_oid = head.target();
            out.branch = if out.detached {
                local_oid
                    .map(|oid| crate::git::short_hash(&oid))
                    .unwrap_or_else(|| "HEAD".to_string())
            } else {
                head.shorthand().unwrap_or("HEAD").to_string()
            };

            if !out.detached
                && let Ok(upstream) = git2::Branch::wrap(head).upstream()
            {
                out.upstream = upstream.name().ok().flatten().map(str::to_string);
                if let (Some(local_oid), Some(up_oid)) = (local_oid, upstream.get().target())
                    && let Ok((ahead, behind)) = repo.graph_ahead_behind(local_oid, up_oid)
                {
                    out.ahead = ahead;
                    out.behind = behind;
                }
            }
        }
        Err(e) if e.code() == ErrorCode::UnbornBranch => {
            // Fresh repo, no commits yet.
            out.branch = repo
                .find_reference("HEAD")
                .ok()
                .and_then(|r| r.symbolic_target().ok().flatten().map(str::to_string))
                .map(|t| t.trim_start_matches("refs/heads/").to_string())
                .unwrap_or_else(|| "main".to_string());
        }
        Err(e) => return Err(GitError::Read(e)),
    }

    let index = repo.index().map_err(GitError::Read)?;
    out.conflicts = if index.has_conflicts() {
        index.conflicts().map(|c| c.count()).unwrap_or(0)
    } else {
        0
    };

    Ok(out)
}

/// Read the working-tree entries, sorted by path. Untracked files included,
/// ignored files excluded.
pub fn files(repo: &Repository) -> GitResult<Vec<FileEntry>> {
    let mut opts = StatusOptions::new();
    opts.include_untracked(true)
        .recurse_untracked_dirs(true)
        .renames_head_to_index(true)
        .renames_index_to_workdir(true)
        .exclude_submodules(true);

    let statuses = repo.statuses(Some(&mut opts)).map_err(GitError::Read)?;

    let mut out: Vec<FileEntry> = statuses
        .iter()
        .filter_map(|entry| {
            let s = entry.status();
            if s.contains(Status::IGNORED) {
                return None;
            }
            let path = PathBuf::from(entry.path().ok()?);
            Some(FileEntry {
                staged: staged_change(s),
                worktree: worktree_change(s),
                binary: false, // filled in a later milestone
                path,
            })
        })
        .collect();

    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

/// First flag the status carries wins; table order is the priority.
/// `CONFLICTED` leads both tables so a conflicted path never reads as a plain
/// modification.
fn first_change(s: Status, table: &[(Status, Change)]) -> Change {
    table
        .iter()
        .find(|(flag, _)| s.contains(*flag))
        .map_or(Change::None, |&(_, change)| change)
}

fn staged_change(s: Status) -> Change {
    first_change(
        s,
        &[
            (Status::CONFLICTED, Change::Conflicted),
            (Status::INDEX_NEW, Change::Added),
            (Status::INDEX_MODIFIED, Change::Modified),
            (Status::INDEX_DELETED, Change::Deleted),
            (Status::INDEX_RENAMED, Change::Renamed),
            (Status::INDEX_TYPECHANGE, Change::Typechange),
        ],
    )
}

fn worktree_change(s: Status) -> Change {
    first_change(
        s,
        &[
            (Status::CONFLICTED, Change::Conflicted),
            (Status::WT_NEW, Change::Untracked),
            (Status::WT_MODIFIED, Change::Modified),
            (Status::WT_DELETED, Change::Deleted),
            (Status::WT_RENAMED, Change::Renamed),
            (Status::WT_TYPECHANGE, Change::Typechange),
        ],
    )
}
