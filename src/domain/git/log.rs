//! Recent commits on HEAD or on an arbitrary branch tip, newest first,
//! bounded to a max count.
//!
//! All types here are plain owned values. No `git2` type escapes this module.

use std::collections::{BTreeMap, HashSet};

use git2::{BranchType, Oid, Repository, Revwalk, Sort};

use crate::domain::git::error::{GitError, GitResult};
use crate::domain::git::model::{CommitEntry, CommitRef, CommitRefKind, PushState};

/// Walk HEAD's history, newest first, up to `max` entries. An unborn branch
/// (fresh repo, no commits) comes back as an empty list, not an error.
pub(super) fn commits(repo: &Repository, max: usize) -> GitResult<Vec<CommitEntry>> {
    let mut revwalk = repo.revwalk().map_err(GitError::Read)?;
    if revwalk.push_head().is_err() {
        return Ok(Vec::new());
    }
    let mut entries = walk(repo, revwalk, max)?;
    decorate(repo, &mut entries);
    mark_push_state(repo, &mut entries);
    Ok(entries)
}

/// Walk one local branch's history, newest first, up to `max` entries. A
/// branch that no longer exists, or one with no commits, comes back as an
/// empty list, not an error — the caller (`App`) treats that as "drop the
/// scope" rather than surfacing it.
pub(super) fn commits_for(
    repo: &Repository,
    branch: &str,
    max: usize,
) -> GitResult<Vec<CommitEntry>> {
    let Ok(branch_ref) = repo.find_branch(branch, BranchType::Local) else {
        return Ok(Vec::new());
    };
    let Some(oid) = branch_ref.get().target() else {
        return Ok(Vec::new());
    };
    let mut revwalk = repo.revwalk().map_err(GitError::Read)?;
    revwalk.push(oid).map_err(GitError::Read)?;
    let mut entries = walk(repo, revwalk, max)?;
    decorate(repo, &mut entries);
    Ok(entries)
}

/// How many commits are read walking a branch's ancestry to place the listed ones:
/// the walk stops early once every listed commit is found.
const ANCESTRY_BUDGET: usize = 20_000;

/// The names pointing at each listed commit, in `git log --decorate` order: `HEAD ->` and
/// the branches, then tags, then remote branches. Reads every ref once.
fn decorate(repo: &Repository, entries: &mut [CommitEntry]) {
    let head = repo.head().ok();
    let head_branch = head
        .as_ref()
        .filter(|h| h.is_branch())
        .and_then(|h| h.shorthand().ok().map(str::to_owned));
    let head_oid = head.as_ref().and_then(git2::Reference::target);

    // (kind order, label) per commit, sorted afterwards.
    let mut by_commit: BTreeMap<Oid, Vec<(u8, String)>> = BTreeMap::new();
    if let Ok(refs) = repo.references() {
        for reference in refs.flatten() {
            let (Ok(name), Ok(short)) = (reference.name(), reference.shorthand()) else {
                continue;
            };
            let Ok(commit) = reference.peel_to_commit() else {
                continue;
            };
            let entry = if name.starts_with("refs/heads/") {
                if head_branch.as_deref() == Some(short) {
                    (0, format!("HEAD -> {short}"))
                } else {
                    (1, short.to_owned())
                }
            } else if name.starts_with("refs/tags/") {
                (2, format!("tag: {short}"))
            } else if name.starts_with("refs/remotes/") {
                (3, short.to_owned())
            } else {
                continue;
            };
            by_commit.entry(commit.id()).or_default().push(entry);
        }
    }
    // A detached HEAD is its own label, on the commit it points at.
    if head_branch.is_none()
        && let Some(oid) = head_oid
    {
        by_commit
            .entry(oid)
            .or_default()
            .push((0, "HEAD".to_owned()));
    }
    for entry in entries {
        let Ok(oid) = Oid::from_str(&entry.full_hash) else {
            continue;
        };
        let Some(mut labels) = by_commit.remove(&oid) else {
            continue;
        };
        labels.sort();
        entry.refs = labels
            .into_iter()
            .map(|(order, label)| CommitRef {
                kind: match order {
                    0 => CommitRefKind::Head,
                    1 => CommitRefKind::Branch,
                    2 => CommitRefKind::Tag,
                    _ => CommitRefKind::Remote,
                },
                label,
            })
            .collect();
    }
}

/// lazygit's hash colours: a commit reachable from the remote's main branch is merged, else
/// one reachable from the current branch's upstream is pushed, else it is not pushed yet.
fn mark_push_state(repo: &Repository, entries: &mut [CommitEntry]) {
    let listed: HashSet<Oid> = entries
        .iter()
        .filter_map(|e| Oid::from_str(&e.full_hash).ok())
        .collect();
    let upstream = repo
        .head()
        .ok()
        .filter(git2::Reference::is_branch)
        .and_then(|head| git2::Branch::wrap(head).upstream().ok())
        .and_then(|branch| branch.get().target());
    let main = ["origin/main", "origin/master"].iter().find_map(|name| {
        repo.find_reference(&format!("refs/remotes/{name}"))
            .ok()
            .and_then(|r| r.target())
    });
    let pushed = upstream.map_or_else(HashSet::new, |tip| reachable(repo, tip, &listed));
    let merged = main.map_or_else(HashSet::new, |tip| reachable(repo, tip, &listed));
    for entry in entries {
        let Ok(oid) = Oid::from_str(&entry.full_hash) else {
            continue;
        };
        entry.push_state = if merged.contains(&oid) {
            PushState::Merged
        } else if pushed.contains(&oid) {
            PushState::Pushed
        } else {
            PushState::Unpushed
        };
    }
}

/// The `wanted` commits reachable from `tip`.
fn reachable(repo: &Repository, tip: Oid, wanted: &HashSet<Oid>) -> HashSet<Oid> {
    let mut found = HashSet::new();
    let Ok(mut revwalk) = repo.revwalk() else {
        return found;
    };
    if revwalk.push(tip).is_err() {
        return found;
    }
    for oid in revwalk.flatten().take(ANCESTRY_BUDGET) {
        if wanted.contains(&oid) {
            found.insert(oid);
            if found.len() == wanted.len() {
                break;
            }
        }
    }
    found
}

/// Shared revwalk drain: TOPOLOGICAL sorting breaks ties between commits made
/// in the same second (which TIME alone leaves in an arbitrary order) by
/// parent-before-child.
fn walk(repo: &Repository, mut revwalk: Revwalk<'_>, max: usize) -> GitResult<Vec<CommitEntry>> {
    revwalk
        .set_sorting(Sort::TIME | Sort::TOPOLOGICAL)
        .map_err(GitError::Read)?;

    revwalk
        .take(max)
        .map(|oid| {
            let oid = oid.map_err(GitError::Read)?;
            let commit = repo.find_commit(oid).map_err(GitError::Read)?;
            let full_hash = oid.to_string();
            Ok(CommitEntry {
                short_hash: full_hash.chars().take(7).collect(),
                full_hash,
                author: commit.author().name().unwrap_or("unknown").to_owned(),
                summary: commit.summary().ok().flatten().unwrap_or("").to_owned(),
                time: commit.time().seconds(),
                refs: Vec::new(),
                push_state: PushState::default(),
            })
        })
        .collect()
}
