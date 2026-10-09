//! Repository statistics for the dashboard (`docs/PLAN_13_DASHBOARD.md`,
//! "Backend"): one headless function, `Repo::stats`, returning owned values.
//! Commits come from a `git2` revwalk from the tip of the main branch only (the
//! commits on `main`, not those only on other branches; when there is no main
//! branch, `HEAD`'s own); the churn columns (D1) come from `git log --numstat`
//! through `exec`, over the same commits. Nothing here imports `ratatui`.
//!
//! Counting rules: `totals.commits`, the series and the authors count every
//! commit in the window, merges included; `kinds` leaves merges out. Times are
//! committer times (what `git log --since` uses), days and ISO weeks are UTC.
//!
//! Authors: commits are grouped by `.mailmap`-resolved email, then the groups
//! that share a name (case-insensitive, trimmed) are merged into one author.
//! Two different people with the same name are merged too: a deliberate choice
//! (the user asked for it), since one person with several emails and no
//! `.mailmap` is far more common. `.mailmap` still wins when present, as it
//! renames before the grouping.
//!
//! This file holds the types and the pure functions. The code that reads with
//! `git2` or runs `git` is `crate::infra::git::stats`.

pub(crate) mod authors;

pub mod branches;

pub mod kind;

pub mod series;

pub mod share;

use self::branches::{BranchHealth, TagSince};
use self::kind::Kind;
use self::series::{Bucket, Granularity};
use self::share::Share;

pub(crate) const DAY: i64 = 86_400;

/// The heat map always shows the last 26 weeks, whatever the window.
pub const HEAT_DAYS: i64 = 26 * 7;

/// Commits walked at most; past it `sampled` is set and the newest are kept.
pub const WALK_CAP: usize = 20_000;

/// Commits `git log --numstat` reads at most (same rule).
pub const NUMSTAT_CAP: usize = 5_000;

/// How far back the statistics look.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Window {
    /// The last 7 days, drawn day by day.
    Days7,
    /// The last 30 days, drawn day by day.
    Days30,
    /// The last 90 days, drawn week by week.
    Days90,
    /// The last 365 days, drawn month by month.
    Year,
    /// The whole history.
    All,
}

impl Window {
    /// Length in days, `None` for the whole history.
    pub const fn days(self) -> Option<i64> {
        match self {
            Self::Days7 => Some(7),
            Self::Days30 => Some(30),
            Self::Days90 => Some(90),
            Self::Year => Some(365),
            Self::All => None,
        }
    }
}

/// Knobs `Repo::stats` fixes for the app and tests override.
#[derive(Debug, Clone, Copy)]
pub struct StatsOptions {
    /// "Now" in unix seconds: the end of the window and of the series.
    pub now: i64,
    /// Most commits the walk reads before it stops and reports `sampled`.
    pub walk_cap: usize,
    /// Most commits `git log --numstat` reads before it stops and reports `sampled`.
    pub numstat_cap: usize,
    /// Read `git log --numstat` (lines and hot files). The app first asks for
    /// `false`, which is quick, paints it, then asks again with `true`.
    pub churn: bool,
}

impl Default for StatsOptions {
    fn default() -> Self {
        Self {
            now: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX)),
            walk_cap: WALK_CAP,
            numstat_cap: NUMSTAT_CAP,
            churn: true,
        }
    }
}

/// Lines added and removed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Lines {
    /// Lines added.
    pub added: u64,
    /// Lines removed.
    pub removed: u64,
}

/// The headline numbers of the dashboard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Totals {
    /// Commits in the window.
    pub commits: usize,
    /// Distinct authors in the window, after `.mailmap` and after the merge of
    /// the emails that share a name.
    pub authors: usize,
    /// Local branches.
    pub local_branches: usize,
    /// Remote-tracking branches, `HEAD` aliases left out.
    pub remote_branches: usize,
    /// Configured remotes: the screen hides remote counts at 0.
    pub remotes: usize,
    /// Tags.
    pub tags: usize,
    /// Stash entries.
    pub stashes: usize,
    /// Oldest and newest commit walked, whatever the window (`None` when empty).
    pub first_commit: Option<i64>,
    /// Time of the newest commit walked, in unix seconds.
    pub last_commit: Option<i64>,
    /// Lines in the window, `None` when `git log` failed.
    pub lines: Option<Lines>,
}

/// One contributor, after `.mailmap` and after the emails that share a name are merged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorStat {
    /// Name and `email` are those of the most frequent email.
    pub name: String,
    /// The most frequent email of this author.
    pub email: String,
    /// Every distinct email of this author (same name), most commits first
    /// (ties alphabetical); at least one, `email` is the first.
    pub emails: Vec<String>,
    /// Summed over every email.
    pub commits: usize,
    /// Summed over every email; `None` when `git log` failed.
    pub added: Option<u64>,
    /// Lines removed, summed over every email; `None` when `git log` failed.
    pub removed: Option<u64>,
    /// The newest commit of any email.
    pub last_commit: i64,
}

/// The commits of one kind (`feat`, `fix`, `docs`, ...) in the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KindStat {
    /// Which kind.
    pub kind: Kind,
    /// Non-merge commits of this kind.
    pub commits: usize,
}

/// One hot file: how many commits of the window touched it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileStat {
    /// The path, relative to the worktree root.
    pub path: String,
    /// `share.count` is the number of commits touching the file; the whole is
    /// the window's commits, so shares need not add to 100.
    pub share: Share,
    /// Lines added to the file in the window.
    pub added: u64,
    /// Lines removed from the file in the window.
    pub removed: u64,
}

/// The files that changed most in the window, and what was left out.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotFiles {
    /// Top 10, most commits first (ties by path), lockfiles and changelogs left
    /// out, only files that exist in the tree of HEAD. The log is read with
    /// `--no-renames`: a renamed file's new path counts only its post-rename
    /// commits, and the old path is `gone`.
    pub files: Vec<FileStat>,
    /// The left-out files that were touched in the window, most touched first.
    pub hidden: Vec<String>,
    /// Distinct touched paths left out because HEAD's tree has no such file
    /// (deleted, moved). Ignored paths count in `hidden` only. 0 when HEAD has
    /// no tree (unborn).
    pub gone: usize,
    /// Non-merge commits read, the whole the shares are taken of.
    pub commits: usize,
}

/// Working tree and upstream, from the same reads the panes use. A file can
/// count in several of `changed`, `staged` and `untracked`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkState {
    /// Files with a change in the worktree (not untracked, not conflicted).
    pub changed: usize,
    /// Files with a change in the index.
    pub staged: usize,
    /// Files git does not track yet.
    pub untracked: usize,
    /// Paths with an unresolved conflict.
    pub conflicted: usize,
    /// Stash entries.
    pub stashes: usize,
    /// The current branch's upstream, as `origin/main`.
    pub upstream: Option<String>,
    /// Commits the branch has that its upstream does not.
    pub ahead: usize,
    /// Commits the upstream has that the branch does not.
    pub behind: usize,
}

/// Everything the dashboard shows, read in one pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoStats {
    /// The window these figures are for.
    pub window: Window,
    /// The headline numbers.
    pub totals: Totals,
    /// Oldest first, from the start of the window clipped to the repository's
    /// age up to now; empty when the window has no commits.
    pub series: Vec<Bucket>,
    /// How `series` groups its buckets: days, weeks or months.
    pub granularity: Granularity,
    /// Commits per day over the last 26 weeks ending now, empty days included,
    /// whatever the window (the heat map draws this).
    pub daily: Vec<Bucket>,
    /// Most commits first (ties by name).
    pub authors: Vec<AuthorStat>,
    /// Non-merge commits by kind, most first, empty kinds left out.
    pub kinds: Vec<KindStat>,
    /// `None` when `git log` failed or was not asked for (`StatsOptions::churn`).
    pub hot_files: Option<HotFiles>,
    /// See `branches::health` for the order.
    pub branches: Vec<BranchHealth>,
    /// Name of the main branch; `None` when HEAD's own branch is all there is.
    pub main_branch: Option<String>,
    /// The working tree and upstream right now.
    pub work: WorkState,
    /// Commits since the newest tag reachable from `HEAD`, if there is one.
    pub since_tag: Option<TagSince>,
    /// The repository is a shallow clone: history before `first_commit` is missing.
    pub shallow: bool,
    /// A cap cut the walk or the numstat short.
    pub sampled: bool,
}
