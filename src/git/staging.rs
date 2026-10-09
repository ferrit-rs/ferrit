//! Deciding what a stage, unstage or discard acts on, from the files the
//! snapshot lists and the repository behind them. No UI: the caller turns the
//! answer into a call on the port and into what the user is told.

use std::path::{Path, PathBuf};

use super::apply::{ApplyDir, ApplyTarget, Granule};
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
