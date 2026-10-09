//! The `git2` and subprocess half of `crate::git::stats`: the types are there.

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};

use git2::{Repository, Sort};

use crate::git::error::{GitError, GitResult};
use crate::git::model::Change;
use crate::git::repo::read_error;
use crate::git::stats::authors::{self, AuthorAcc};
use crate::git::stats::kind::{self, Kind};
use crate::git::stats::series::{self, Granularity};
use crate::git::stats::{
    DAY, FileStat, HEAT_DAYS, HotFiles, KindStat, Lines, RepoStats, StatsOptions, Totals, Window,
    WorkState,
};

use super::read;

mod branch_health;
mod churn;

pub(super) fn repo_stats(
    repo: &Repository,
    window: Window,
    opts: &StatsOptions,
    cancel: &AtomicBool,
) -> GitResult<RepoStats> {
    let cutoff = window.days().map(|days| opts.now - days * DAY);
    let mailmap = repo.mailmap().ok();

    // Only the commits on the main branch: its tip is the one start. With no main
    // branch to name, HEAD's own history is all there is (an unborn HEAD has none).
    let main = branch_health::main_branch(repo);
    let mut walk = repo.revwalk().map_err(read_error)?;
    match &main {
        Some((_, tip)) => walk.push(*tip).map_err(read_error)?,
        None => {
            let _ = walk.push_head();
        },
    }
    walk.set_sorting(Sort::TIME).map_err(read_error)?;

    let mut sampled = false;
    let (mut first, mut last) = (None::<i64>, None::<i64>);
    let mut times = Vec::new();
    let mut recent = Vec::new();
    let mut authors: BTreeMap<String, AuthorAcc> = BTreeMap::new();
    let mut kinds: BTreeMap<Kind, usize> = BTreeMap::new();
    for (walked, oid) in walk.enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return Err(GitError::Cancelled);
        }
        if walked >= opts.walk_cap {
            sampled = true;
            break;
        }
        let commit = repo
            .find_commit(oid.map_err(read_error)?)
            .map_err(read_error)?;
        let time = commit.time().seconds();
        first = Some(first.map_or(time, |f| f.min(time)));
        last = Some(last.map_or(time, |l| l.max(time)));
        if time > opts.now - HEAT_DAYS * DAY {
            recent.push(time);
        }
        if cutoff.is_some_and(|c| time < c) {
            continue;
        }
        times.push(time);
        let signature = mailmap
            .as_ref()
            .and_then(|m| commit.author_with_mailmap(m).ok())
            .unwrap_or_else(|| commit.author().to_owned());
        let email = signature.email().unwrap_or("").to_owned();
        let name = signature.name().unwrap_or("unknown").to_owned();
        let key = if email.is_empty() {
            name.clone()
        } else {
            email.to_lowercase()
        };
        let acc = authors.entry(key).or_default();
        if acc.commits == 0 {
            acc.name = name;
            acc.email = email;
        }
        acc.commits += 1;
        acc.last = acc.last.max(time);
        if commit.parent_count() <= 1 {
            let subject = commit.summary().ok().flatten().unwrap_or("");
            *kinds.entry(kind::parse_kind(subject)).or_default() += 1;
        }
    }

    let (main_branch, branch_health) = branch_health::health(repo, opts.now)?;
    let start = first.map_or(opts.now, |f| cutoff.map_or(f, |c| c.max(f)));
    let granularity = Granularity::for_span_days((opts.now - start).max(0) / DAY);
    let series = if times.is_empty() {
        Vec::new()
    } else {
        series::series(times.iter().copied(), start, opts.now, granularity)
    };

    let daily = series::series(
        recent.iter().copied(),
        opts.now - (HEAT_DAYS - 1) * DAY,
        opts.now,
        Granularity::Day,
    );

    let mut author_stats = authors::merge(authors);
    author_stats.sort_by(|a, b| b.commits.cmp(&a.commits).then_with(|| a.name.cmp(&b.name)));
    let mut kind_stats: Vec<KindStat> = kinds
        .into_iter()
        .map(|(kind, commits)| KindStat { kind, commits })
        .collect();
    kind_stats.sort_by_key(|k| std::cmp::Reverse(k.commits));

    if cancel.load(Ordering::Relaxed) {
        return Err(GitError::Cancelled);
    }
    let churn = if opts.churn {
        let rev = main
            .as_ref()
            .map_or_else(|| "HEAD".to_owned(), |(_, tip)| tip.to_string());
        churn::read(repo, &rev, cutoff, opts.numstat_cap)
    } else {
        None
    };
    if let Some(churn) = &churn {
        sampled |= churn.sampled;
        for author in &mut author_stats {
            let (mut added, mut removed) = (0, 0);
            for email in &author.emails {
                if let Some(l) = churn.authors.get(&email.to_lowercase()) {
                    added += l.added;
                    removed += l.removed;
                }
            }
            author.added = Some(added);
            author.removed = Some(removed);
        }
    }
    if cancel.load(Ordering::Relaxed) {
        return Err(GitError::Cancelled);
    }

    // Unborn or tree-less HEAD: nothing to check against, keep every path.
    let head_tree = repo.head().ok().and_then(|h| h.peel_to_tree().ok());
    let head_tree_has = |path: &str| {
        head_tree
            .as_ref()
            .is_none_or(|tree| tree.get_path(std::path::Path::new(path)).is_ok())
    };
    let work = work_state(repo);
    Ok(RepoStats {
        window,
        totals: Totals {
            commits: times.len(),
            authors: author_stats.len(),
            local_branches: branch_health.len(),
            remote_branches: branch_health::count_refs(repo, "refs/remotes/"),
            remotes: repo.remotes().map_or(0, |r| r.len()),
            tags: branch_health::count_refs(repo, "refs/tags/"),
            stashes: work.stashes,
            first_commit: first,
            last_commit: last,
            lines: churn.as_ref().map(|c| c.lines),
        },
        series,
        granularity,
        daily,
        authors: author_stats,
        kinds: kind_stats,
        hot_files: churn.as_ref().map(|c| c.hot_files(&head_tree_has)),
        branches: branch_health,
        main_branch,
        work,
        since_tag: branch_health::since_tag(repo),
        shallow: repo.is_shallow(),
        sampled,
    })
}

/// `status::files` and `status::header`, counted; a bare repository (no
/// worktree to read) gives the default.
fn work_state(repo: &Repository) -> WorkState {
    let stashes = repo.reflog("refs/stash").map_or(0, |log| log.len());
    let (Ok(files), Ok(header)) = (read::files(repo), read::header(repo)) else {
        return WorkState {
            stashes,
            ..WorkState::default()
        };
    };
    WorkState {
        changed: files
            .iter()
            .filter(|f| {
                !matches!(
                    f.worktree,
                    Change::None | Change::Untracked | Change::Conflicted
                )
            })
            .count(),
        staged: files.iter().filter(|f| f.has_staged()).count(),
        untracked: files
            .iter()
            .filter(|f| f.worktree == Change::Untracked)
            .count(),
        conflicted: header.conflicts,
        stashes,
        upstream: header.upstream,
        ahead: header.ahead,
        behind: header.behind,
    }
}
