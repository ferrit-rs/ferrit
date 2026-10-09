//! The `git2` and subprocess half of `crate::git::stats/branches`: the types are there.

use crate::git::repo::read_error;
use git2::{BranchType, Oid, Repository};

use crate::git::error::GitResult;

use crate::git::stats::branch_health::{BranchHealth, STALE_SECONDS, TagSince, VsMain};

/// The main branch: `origin/HEAD` (any remote) if set, else
/// `init.defaultBranch`, else the first of `main`, `master`. Named and
/// resolved to its tip (the local branch when it exists, else the remote
/// one). `None` means HEAD's own branch is all there is.
pub(super) fn main_branch(repo: &Repository) -> Option<(String, Oid)> {
    let tip = |name: &str, remote: Option<&str>| {
        repo.find_branch(name, BranchType::Local)
            .ok()
            .and_then(|b| b.get().target())
            .or_else(|| {
                let full = format!("refs/remotes/{}/{name}", remote?);
                repo.find_reference(&full).ok()?.target()
            })
    };
    let remotes = repo.remotes().ok()?;
    for remote in remotes.iter().flatten().flatten() {
        let Ok(head) = repo.find_reference(&format!("refs/remotes/{remote}/HEAD")) else {
            continue;
        };
        let target = head.symbolic_target().ok().flatten().map(str::to_owned);
        let prefix = format!("refs/remotes/{remote}/");
        if let Some(name) = target.as_deref().and_then(|t| t.strip_prefix(&prefix))
            && let Some(oid) = tip(name, Some(remote))
        {
            return Some((name.to_owned(), oid));
        }
    }
    let configured = repo
        .config()
        .ok()
        .and_then(|c| c.get_string("init.defaultBranch").ok());
    configured
        .iter()
        .map(String::as_str)
        .chain(["main", "master"])
        .find_map(|name| tip(name, None).map(|oid| (name.to_owned(), oid)))
}

/// Every local branch with its health, and the main branch's name. Order:
/// the current branch, then branches with work not in the main branch, then
/// the rest (merged ones, the main branch itself), each group by most recent
/// tip and then name.
pub(super) fn health(
    repo: &Repository,
    now: i64,
) -> GitResult<(Option<String>, Vec<BranchHealth>)> {
    let main = main_branch(repo);
    let mut out = Vec::new();
    for entry in repo.branches(Some(BranchType::Local)).map_err(read_error)? {
        let (branch, _) = entry.map_err(read_error)?;
        let Some(name) = branch.name().ok().flatten().map(str::to_owned) else {
            continue;
        };
        let Some(tip) = branch.get().target() else {
            continue;
        };
        let current = branch.is_head();
        let tip_time = repo
            .find_commit(tip)
            .map_or(0, |commit| commit.time().seconds());
        let vs_main = main.as_ref().and_then(|(main_name, main_tip)| {
            let (ahead, behind) = repo.graph_ahead_behind(tip, *main_tip).ok()?;
            let merged = name != *main_name
                && (tip == *main_tip || repo.graph_descendant_of(*main_tip, tip).unwrap_or(false));
            Some(VsMain {
                ahead,
                behind,
                merged,
            })
        });
        out.push(BranchHealth {
            stale: vs_main.is_some() && !current && now - tip_time > STALE_SECONDS,
            name,
            current,
            tip_time,
            vs_main,
        });
    }
    let group = |b: &BranchHealth| {
        if b.current {
            0
        } else if b.vs_main.is_some_and(|v| v.ahead > 0) {
            1
        } else {
            2
        }
    };
    out.sort_by(|a, b| {
        group(a)
            .cmp(&group(b))
            .then_with(|| b.tip_time.cmp(&a.tip_time))
            .then_with(|| a.name.cmp(&b.name))
    });
    Ok((main.map(|(name, _)| name), out))
}

/// How many references live under `prefix`, remote `HEAD` aliases excluded.
pub(super) fn count_refs(repo: &Repository, prefix: &str) -> usize {
    repo.references_glob(&format!("{prefix}*"))
        .map_or(0, |refs| {
            refs.flatten()
                .filter(|r| r.name().is_ok_and(|n| !n.ends_with("/HEAD")))
                .count()
        })
}

/// The tag whose commit is the newest among those reachable from `HEAD`
/// (annotated and lightweight alike), with the commits since it.
pub(super) fn since_tag(repo: &Repository) -> Option<TagSince> {
    let head = repo.head().ok()?.target()?;
    let (_, name, tag_oid) = repo
        .references_glob("refs/tags/*")
        .ok()?
        .flatten()
        .filter_map(|reference| {
            let commit = reference.peel_to_commit().ok()?;
            let name = reference.shorthand().ok()?.to_owned();
            Some((commit.time().seconds(), name, commit.id()))
        })
        .filter(|(_, _, oid)| *oid == head || repo.graph_descendant_of(head, *oid).unwrap_or(false))
        .max_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)))?;
    let (commits, _) = repo.graph_ahead_behind(head, tag_oid).ok()?;
    Some(TagSince { name, commits })
}
