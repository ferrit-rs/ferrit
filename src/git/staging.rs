//! Deciding what a stage, unstage or discard acts on, from the files the
//! snapshot lists and the repository behind them. No UI: the caller turns the
//! answer into a call on the port and into what the user is told.

use std::path::{Path, PathBuf};

use super::apply::{ApplyDir, ApplyTarget, Granule};
use super::diff::DiffSide;
use super::error::GitResult;
use super::model::{Change, FileEntry};
use super::port::GitPort;

/// Which way `<space>` goes over these files: stage when any has a change in
/// the worktree, else unstage when any has one in the index, else nowhere.
pub fn direction<'a, I>(files: I) -> Option<ApplyDir>
where
    I: IntoIterator<Item = &'a FileEntry>,
{
    let mut staged = false;
    for file in files {
        if file.worktree != Change::None {
            return Some(ApplyDir::Forward);
        }
        staged |= file.staged != Change::None;
    }
    staged.then_some(ApplyDir::Reverse)
}

/// Conflict markers still in `path`. A file that cannot be read counts as
/// "has markers": refusing to stage is the safe side of an unknown.
pub fn has_markers(repo: &dyn GitPort, path: &Path) -> bool {
    repo.has_conflict_markers(path).unwrap_or(true)
}

/// The conflicted files that still hold markers, in the order given.
pub fn unresolved_conflicts<'a, I>(repo: &dyn GitPort, files: I) -> Vec<PathBuf>
where
    I: IntoIterator<Item = &'a FileEntry>,
{
    files
        .into_iter()
        .filter(|file| file.is_conflicted() && has_markers(repo, &file.path))
        .map(|file| file.path.clone())
        .collect()
}

/// Run a `Granule` through the matching call of the port.
pub fn apply_granule(
    repo: &dyn GitPort,
    granule: &Granule,
    dir: ApplyDir,
    target: ApplyTarget,
) -> GitResult<()> {
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

/// One call on the port that a stage, an unstage or a discard comes to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StageAction {
    /// Stage or unstage one file, or a whole directory.
    File {
        /// The path, relative to the worktree root.
        path: PathBuf,
        /// Stage (`Forward`) or unstage (`Reverse`).
        dir: ApplyDir,
    },
    /// Stage or unstage every file.
    All {
        /// Stage (`Forward`) or unstage (`Reverse`).
        dir: ApplyDir,
    },
    /// Stage every file except those still holding conflict markers.
    AllExcept {
        /// The files left out.
        blocked: Vec<PathBuf>,
    },
    /// Apply a hunk or some lines to the index or the worktree.
    Granule {
        /// What to apply.
        granule: Granule,
        /// Forward or reversed.
        dir: ApplyDir,
        /// The index or the worktree.
        target: ApplyTarget,
    },
    /// Throw away a file's worktree changes.
    Discard {
        /// The path, relative to the worktree root.
        path: PathBuf,
        /// The file is not tracked: it is deleted rather than restored.
        untracked: bool,
    },
}

impl StageAction {
    /// The files a bulk stage left out, if it did.
    #[must_use]
    pub fn left_out(&self) -> Option<&[PathBuf]> {
        match self {
            Self::AllExcept { blocked } => Some(blocked),
            _ => None,
        }
    }
}

/// Why a stage was refused before any call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// `git add` marks an unmerged path resolved whatever the file holds, so
    /// it is refused while this one still has conflict markers.
    ConflictMarkers(PathBuf),
}

/// What a key that stages comes to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Plan {
    /// There is nothing to stage or unstage.
    Nothing,
    /// Make this call.
    Do(StageAction),
    /// Do not, and say why.
    Refuse(Refusal),
}

/// `<space>` on a file: stage it, or unstage it when it is fully staged.
pub fn plan_file(repo: &dyn GitPort, entry: &FileEntry) -> Plan {
    let Some(dir) = direction([entry]) else {
        return Plan::Nothing;
    };
    if dir == ApplyDir::Forward && entry.is_conflicted() && has_markers(repo, &entry.path) {
        return Plan::Refuse(Refusal::ConflictMarkers(entry.path.clone()));
    }
    Plan::Do(StageAction::File {
        path: entry.path.clone(),
        dir,
    })
}

/// `<space>` on a directory row: every change under it, or the whole tree for
/// the root row (an empty path). Conflicted files that still hold markers
/// block it.
pub fn plan_directory(repo: &dyn GitPort, files: &[FileEntry], directory: &Path) -> Plan {
    let root = directory.as_os_str().is_empty();
    let under = files
        .iter()
        .filter(|file| root || file.path.starts_with(directory));
    let Some(dir) = direction(under.clone()) else {
        return Plan::Nothing;
    };
    if dir == ApplyDir::Forward
        && let Some(first) = unresolved_conflicts(repo, under).into_iter().next()
    {
        return Plan::Refuse(Refusal::ConflictMarkers(first));
    }
    Plan::Do(if root {
        StageAction::All { dir }
    } else {
        StageAction::File {
            path: directory.to_path_buf(),
            dir,
        }
    })
}

/// `a`: every file, except the conflicted ones that still hold markers.
pub fn plan_all(repo: &dyn GitPort, files: &[FileEntry]) -> Plan {
    let Some(dir) = direction(files) else {
        return Plan::Nothing;
    };
    let blocked = if dir == ApplyDir::Forward {
        unresolved_conflicts(repo, files)
    } else {
        Vec::new()
    };
    Plan::Do(if blocked.is_empty() {
        StageAction::All { dir }
    } else {
        StageAction::AllExcept { blocked }
    })
}

/// `<space>` in the diff: stage the granule from the worktree side, unstage it
/// from the staged one.
#[must_use]
pub fn plan_granule(granule: Granule, side: DiffSide) -> StageAction {
    let dir = match side {
        DiffSide::Worktree => ApplyDir::Forward,
        DiffSide::Staged => ApplyDir::Reverse,
    };
    StageAction::Granule {
        granule,
        dir,
        target: ApplyTarget::Index,
    }
}

/// Throw away a granule's worktree change: the patch applied in reverse.
#[must_use]
pub fn plan_discard_granule(granule: Granule) -> StageAction {
    StageAction::Granule {
        granule,
        dir: ApplyDir::Reverse,
        target: ApplyTarget::Worktree,
    }
}

/// Throw away a file's worktree changes; an untracked one is deleted.
#[must_use]
pub fn plan_discard_file(files: &[FileEntry], path: &Path) -> StageAction {
    let untracked = files
        .iter()
        .find(|file| file.path == path)
        .is_some_and(|file| file.worktree == Change::Untracked);
    StageAction::Discard {
        path: path.to_path_buf(),
        untracked,
    }
}

/// Make the call.
pub fn run(repo: &dyn GitPort, action: &StageAction) -> GitResult<()> {
    match action {
        StageAction::File { path, dir } => repo.stage_file(path, *dir),
        StageAction::All { dir } => repo.stage_all(*dir),
        StageAction::AllExcept { blocked } => repo.stage_all_except(blocked),
        StageAction::Granule {
            granule,
            dir,
            target,
        } => apply_granule(repo, granule, *dir, *target),
        StageAction::Discard { path, untracked } => repo.discard_file(path, *untracked),
    }
}
