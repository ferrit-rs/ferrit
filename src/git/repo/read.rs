//! Repo status: the header line data and the working-tree file list.
//!
//! All types here are plain owned values. No `git2` type escapes this module.
//! Recent commits on HEAD or on an arbitrary branch tip, newest first,
//! bounded to a max count.
//! Local branches: which one is HEAD, its upstream, ahead/behind.

use crate::git::diff::Rev;
use crate::git::diff::{Diff, DiffOpts, DiffSide};
use crate::git::error::{GitError, GitResult};
use crate::git::model::{
    BranchEntry, Change, CommitEntry, CommitRef, CommitRefKind, FileEntry, PushState, StatusHeader,
};
use crate::git::repo::exec;
use crate::git::repo::read_error;
use git2::{BranchType, ErrorCode, Oid, Repository, Revwalk, Sort, Status, StatusOptions};
use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Output;

// --- status ---
/// Read the header: branch, upstream, ahead/behind, conflict count.
pub(crate) fn header(repo: &Repository) -> GitResult<StatusHeader> {
    let mut out = StatusHeader::default();

    match repo.head() {
        Ok(head) => {
            out.detached = repo.head_detached().unwrap_or(false);
            let local_oid = head.target();
            out.branch = if out.detached {
                local_oid.map_or_else(
                    || "HEAD".to_owned(),
                    |oid| crate::git::repo::short_hash(&oid),
                )
            } else {
                head.shorthand().unwrap_or("HEAD").to_owned()
            };

            if !out.detached
                && let Ok(upstream) = git2::Branch::wrap(head).upstream()
            {
                out.upstream = upstream.name().ok().flatten().map(str::to_owned);
                if let (Some(local_oid), Some(up_oid)) = (local_oid, upstream.get().target())
                    && let Ok((ahead, behind)) = repo.graph_ahead_behind(local_oid, up_oid)
                {
                    out.ahead = ahead;
                    out.behind = behind;
                }
            }
        },
        Err(e) if e.code() == ErrorCode::UnbornBranch => {
            // Fresh repo, no commits yet.
            out.branch = repo
                .find_reference("HEAD")
                .ok()
                .and_then(|r| r.symbolic_target().ok().flatten().map(str::to_owned))
                .map_or_else(
                    || "main".to_owned(),
                    |t| t.trim_start_matches("refs/heads/").to_owned(),
                );
        },
        Err(e) => return Err(read_error(e)),
    }

    let index = repo.index().map_err(read_error)?;
    out.conflicts = if index.has_conflicts() {
        index.conflicts().map_or(0, Iterator::count)
    } else {
        0
    };

    Ok(out)
}

/// Read the working-tree entries, sorted by path. Untracked files included,
/// ignored files excluded.
pub(crate) fn files(repo: &Repository) -> GitResult<Vec<FileEntry>> {
    let mut opts = StatusOptions::new();
    opts.include_untracked(true)
        .recurse_untracked_dirs(true)
        .renames_head_to_index(true)
        .renames_index_to_workdir(true)
        .exclude_submodules(true);

    let statuses = repo.statuses(Some(&mut opts)).map_err(read_error)?;

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

// --- log ---
/// Walk HEAD's history, newest first, up to `max` entries. An unborn branch
/// (fresh repo, no commits) comes back as an empty list, not an error.
pub(crate) fn commits(repo: &Repository, max: usize) -> GitResult<Vec<CommitEntry>> {
    let mut revwalk = repo.revwalk().map_err(read_error)?;
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
pub(crate) fn commits_for(
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
    let mut revwalk = repo.revwalk().map_err(read_error)?;
    revwalk.push(oid).map_err(read_error)?;
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
    let head = repo.head().ok().filter(git2::Reference::is_branch);
    // A branch with no upstream has nothing "unpushed" (lazygit computes that against
    // `@{upstream}..HEAD`), so its commits are pushed unless merged.
    let no_upstream = head
        .as_ref()
        .is_some_and(|h| git2::Branch::wrap(h.clone()).upstream().is_err());
    let upstream = head
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
        } else if no_upstream || pushed.contains(&oid) {
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
        .map_err(read_error)?;

    revwalk
        .take(max)
        .map(|oid| {
            let oid = oid.map_err(read_error)?;
            let commit = repo.find_commit(oid).map_err(read_error)?;
            let full_hash = oid.to_string();
            Ok(CommitEntry {
                short_hash: full_hash.chars().take(7).collect(),
                full_hash,
                author: commit.author().name().unwrap_or("unknown").to_owned(),
                author_email: commit.author().email().unwrap_or("").to_owned(),
                summary: commit.summary().ok().flatten().unwrap_or("").to_owned(),
                body: commit.body().ok().flatten().unwrap_or("").trim().to_owned(),
                time: commit.time().seconds(),
                refs: Vec::new(),
                push_state: PushState::default(),
            })
        })
        .collect()
}

// --- diff ---
/// One file's worktree-or-staged diff.
pub(crate) fn file_diff(
    repo: &Repository,
    path: &Path,
    side: DiffSide,
    opts: DiffOpts,
) -> GitResult<Diff> {
    let workdir = workdir(repo)?;

    let mut cmd = DiffCmd::base("diff", opts);
    if side == DiffSide::Staged {
        cmd = cmd.arg("--cached");
    }
    let out = cmd
        .arg("--")
        .arg(path.to_string_lossy().into_owned())
        .run(workdir)?;
    if !out.status.success() {
        return Err(GitError::DiffFailed(stderr(&out)));
    }
    let text = String::from_utf8_lossy(&out.stdout).into_owned();

    // Untracked file: plain `git diff` prints nothing. Re-ask with --no-index
    // so it renders as all-additions, the way lazygit does. A tracked file
    // that simply has no worktree change also prints nothing: leave that one
    // empty, do not fake an all-additions diff for it.
    if text.trim().is_empty() && side == DiffSide::Worktree && is_untracked(repo, path) {
        let ni = DiffCmd::base("diff", opts)
            .arg("--no-index")
            .arg("--")
            .arg("/dev/null")
            .arg(path.to_string_lossy().into_owned())
            .run(workdir)?;
        // --no-index exits 1 when the files differ, which is the normal case.
        match ni.status.code() {
            Some(0 | 1) => {},
            _ => return Err(GitError::DiffFailed(stderr(&ni))),
        }
        return Ok(Diff::new(String::from_utf8_lossy(&ni.stdout).into_owned()));
    }

    Ok(Diff::new(text))
}

/// A commit against its first parent (`git show`). Empty-tree diff for the root
/// commit; first-parent diff for a merge (`-m --first-parent`). `hash` is a
/// `CommitEntry::full_hash`.
pub(crate) fn commit_diff(repo: &Repository, hash: &str, opts: DiffOpts) -> GitResult<Diff> {
    let workdir = workdir(repo)?;

    // `--decorate` puts `(HEAD -> main, tag: v1, origin/main)` on the commit line and
    // `--stat` the per-file summary between the message and the patch, as lazygit's
    // Patch shows them.
    let out = DiffCmd::base("show", opts)
        .arg("-m")
        .arg("--first-parent")
        .arg("--decorate=short")
        .arg("--stat")
        .arg("-p")
        .arg(hash.to_owned())
        .run(workdir)?;

    if !out.status.success() {
        let err = stderr(&out);
        if err.contains("bad object")
            || err.contains("unknown revision")
            || err.contains("ambiguous argument")
        {
            return Err(GitError::NoSuchCommit(hash.to_owned()));
        }
        return Err(GitError::DiffFailed(err));
    }
    Ok(Diff::new(String::from_utf8_lossy(&out.stdout).into_owned()))
}

/// A stash entry's patch (`git stash show -p --stat`), untracked files included,
/// under lazygit's header: `header` (`stash@{0}: On main: msg`), a blank line,
/// the stat, a blank line, the patch. `oid` is a `StashEntry::oid`; git accepts a
/// stash-like commit directly, so a shifted `stash@{n}` cannot make this stale.
pub(crate) fn stash_diff(
    repo: &Repository,
    oid: &str,
    header: &str,
    opts: DiffOpts,
) -> GitResult<Diff> {
    // `git stash show` wants its own verb before the diff flags.
    let out = DiffCmd::base("stash", opts)
        .after_subcommand("show")
        .arg("--stat")
        .arg("-p")
        .arg("--include-untracked")
        .arg(oid.to_owned())
        .run(workdir(repo)?)?;
    if !out.status.success() {
        return Err(GitError::DiffFailed(stderr(&out)));
    }
    Ok(Diff::new(format!(
        "{header}\n\n{}",
        String::from_utf8_lossy(&out.stdout)
    )))
}

/// Is `path` untracked (worktree-new) in `repo`? Decides whether an empty
/// `git diff` means "nothing changed" or "needs the `--no-index` fallback".
fn is_untracked(repo: &Repository, path: &Path) -> bool {
    repo.status_file(path)
        .is_ok_and(|s| s.contains(Status::WT_NEW))
}

/// Shared with `apply.rs`, which runs `git apply` / `add` / `restore` /
/// `clean` against the same worktree.
pub(crate) fn workdir(repo: &Repository) -> GitResult<&Path> {
    repo.workdir()
        .ok_or_else(|| GitError::DiffFailed("bare repository has no working tree".to_owned()))
}

/// Shared with `apply.rs`.
pub(crate) fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).trim().to_owned()
}

/// Shared argv builder. The flag set lives here once so an `--ext-diff` /
/// `-c diff.external=` addition later is a one-line change.
struct DiffCmd {
    args: Vec<String>,
}

impl DiffCmd {
    pub(crate) fn base(sub: &str, opts: DiffOpts) -> Self {
        let mut args = vec![
            sub.to_owned(),
            "--no-ext-diff".to_owned(),
            "--color=never".to_owned(),
            format!("--unified={}", opts.context),
            format!("--find-renames={}%", opts.rename_threshold),
            "--submodule".to_owned(),
        ];
        if opts.ignore_whitespace {
            args.push("--ignore-all-space".to_owned());
        }
        Self { args }
    }

    /// Insert a verb right after the subcommand (`stash` `show` ...).
    fn after_subcommand(mut self, verb: &str) -> Self {
        self.args.insert(1, verb.to_owned());
        self
    }

    fn arg(mut self, a: impl Into<String>) -> Self {
        self.args.push(a.into());
        self
    }

    fn run(self, workdir: &Path) -> GitResult<Output> {
        exec::output(exec::git(workdir).args(&self.args))
            .map_err(|e| GitError::DiffFailed(format!("cannot run git: {e}")))
    }
}

// --- blob ---
fn read_err(path: &Path, msg: impl std::fmt::Display) -> GitError {
    read_error(git2::Error::from_str(&format!("{}: {msg}", path.display())))
}

/// Read `path` at `rev`. Missing files, bare repos and non-blob entries all
/// come back as `GitError::Read`, never a panic.
pub(crate) fn blob_bytes(repo: &Repository, path: &Path, rev: Rev) -> GitResult<Vec<u8>> {
    match rev {
        Rev::Workdir => {
            let root = repo
                .workdir()
                .ok_or_else(|| read_err(path, "bare repository has no working directory"))?;
            std::fs::read(root.join(path)).map_err(|e| read_err(path, e))
        },
        Rev::Head => {
            let tree = repo
                .head()
                .and_then(|h| h.peel_to_tree())
                .map_err(read_error)?;
            let entry = tree.get_path(path).map_err(read_error)?;
            let object = entry.to_object(repo).map_err(read_error)?;
            let blob = object
                .as_blob()
                .ok_or_else(|| read_err(path, "not a blob at HEAD"))?;
            Ok(blob.content().to_vec())
        },
    }
}

// --- refs ---
/// Read the local branches, HEAD first, then alphabetical by name.
pub(crate) fn branches(repo: &Repository) -> GitResult<Vec<BranchEntry>> {
    let mut out: Vec<BranchEntry> = repo
        .branches(Some(BranchType::Local))
        .map_err(read_error)?
        .map(|res| {
            let (branch, _) = res.map_err(read_error)?;
            let is_head = branch.is_head();
            let name = branch
                .name()
                .map_err(read_error)?
                .unwrap_or("(invalid utf-8)")
                .to_owned();

            let (upstream, ahead, behind) = match branch.upstream() {
                Ok(up) => {
                    let up_name = up.name().ok().flatten().map(str::to_owned);
                    let ahead_behind = match (branch.get().target(), up.get().target()) {
                        (Some(local), Some(remote)) => {
                            repo.graph_ahead_behind(local, remote).unwrap_or((0, 0))
                        },
                        _ => (0, 0),
                    };
                    (up_name, ahead_behind.0, ahead_behind.1)
                },
                Err(_) => (None, 0, 0),
            };

            let tip_time = branch
                .get()
                .target()
                .and_then(|oid| repo.find_commit(oid).ok())
                .map_or(0, |commit| commit.time().seconds());

            Ok(BranchEntry {
                name,
                is_head,
                upstream,
                ahead,
                behind,
                tip_time,
            })
        })
        .collect::<GitResult<Vec<_>>>()?;

    out.sort_by(|a, b| match (a.is_head, b.is_head) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        // The checked-out branch first, then the most recently committed to, as
        // lazygit orders them; a tie falls back to the name.
        _ => b
            .tip_time
            .cmp(&a.tip_time)
            .then_with(|| a.name.cmp(&b.name)),
    });
    Ok(out)
}
