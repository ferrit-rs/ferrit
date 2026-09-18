//! Headless git backend. Nothing under `git::` imports `ratatui`.
//!
//! See `docs/PLAN_2_GIT_BACKEND.md`: Status, Files, Branches, Commits, Stash
//! and blob reads are all wired to real `git2` reads (G0..G6). Since
//! `docs/PLAN_6_STAGING.md` and `docs/PLAN_7_COMMIT.md`, `apply` and
//! `commit` also write the index, worktree and `HEAD` (stage / unstage /
//! discard, commit / amend / reword / fixup / squash); those are the two
//! submodules that are not read-only, and both shell out to `git` rather
//! than writing objects directly.

mod apply;
mod blob;
mod commit;
mod diff;
mod error;
mod log;
mod model;
mod refs;
mod stash;
mod status;

use std::path::Path;

use git2::Repository;

pub use apply::{ApplyDir, ApplyTarget, transform_body};
pub use blob::Rev;
pub use commit::{CommitKind, CommitOpts};
pub use diff::{Diff, DiffOpts, DiffSide, DiffStat, FileMeta, FileStatus, HunkMeta, parse_diff};
pub use error::{GitError, GitResult};
pub use model::{BranchEntry, CommitEntry, StashEntry};
pub use status::{Change, FileEntry, StatusHeader};

/// How many commits `Repo::snapshot()` reads for the Commits pane. Plain
/// constant until the pane grows real scrolling/paging.
const COMMITS_LIMIT: usize = 200;

/// An open repository. Wraps `git2::Repository` and hands out owned snapshots.
pub struct Repo {
    inner: Repository,
}

/// Everything the wired panes need from one refresh.
#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub header: StatusHeader,
    pub files: Vec<FileEntry>,
    pub branches: Vec<BranchEntry>,
    pub commits: Vec<CommitEntry>,
    pub stashes: Vec<StashEntry>,
}

/// Abbreviated hash, the 7 hex chars `git` shows by default. Shared by
/// `status` (upstream not needed there, but commits/refs both want it).
pub(crate) fn short_hash(oid: &git2::Oid) -> String {
    oid.to_string().chars().take(7).collect()
}

impl Repo {
    /// Open the repository at or above `path`. Walks up like `git` does.
    pub fn open(path: &Path) -> GitResult<Self> {
        let inner = Repository::discover(path).map_err(|e| {
            if e.code() == git2::ErrorCode::NotFound {
                GitError::NotARepository(path.to_path_buf())
            } else {
                GitError::Open(e)
            }
        })?;
        Ok(Self { inner })
    }

    /// The repository's directory name, e.g. `ferrit`. Used in the status
    /// header (`ferrit -> main`). Falls back to `"repo"` for odd layouts.
    pub fn name(&self) -> String {
        self.inner
            .workdir()
            .and_then(|w| w.file_name())
            .or_else(|| self.inner.path().parent().and_then(|p| p.file_name()))
            .map_or_else(|| "repo".to_owned(), |n| n.to_string_lossy().into_owned())
    }

    /// Re-read every wired pane in one go. Partial failure fails the whole call.
    ///
    /// `&mut self`: reading the stash list needs `&mut git2::Repository`.
    pub fn snapshot(&mut self) -> GitResult<Snapshot> {
        Ok(Snapshot {
            header: status::header(&self.inner)?,
            files: status::files(&self.inner)?,
            branches: refs::branches(&self.inner)?,
            commits: log::commits(&self.inner, COMMITS_LIMIT)?,
            stashes: stash::stashes(&mut self.inner)?,
        })
    }

    /// The worktree root, or `None` for a bare repo. The filesystem watcher
    /// recurses from here; `.git` lives under it in the normal layout.
    pub fn workdir(&self) -> Option<&Path> {
        self.inner.workdir()
    }

    /// Raw bytes of `path` at `rev`. Feeds the right-pane image preview.
    pub fn blob_bytes(&self, path: &Path, rev: Rev) -> GitResult<Vec<u8>> {
        blob::blob_bytes(&self.inner, path, rev)
    }

    /// One file's diff (`git diff [--cached] -- <path>`), parsed. Untracked
    /// files come back via `--no-index` as all-additions.
    pub fn file_diff(&self, path: &Path, side: DiffSide, opts: DiffOpts) -> GitResult<Diff> {
        diff::file_diff(&self.inner, path, side, opts)
    }

    /// One commit's diff against its first parent (`git show <hash>`), parsed.
    pub fn commit_diff(&self, hash: &str, opts: DiffOpts) -> GitResult<Diff> {
        diff::commit_diff(&self.inner, hash, opts)
    }

    /// One local branch's own commit history, newest first, bounded by
    /// `COMMITS_LIMIT`. Feeds the Branches pane's Enter-to-drill-down log
    /// (`docs/PLAN_2_GIT_BACKEND.md`, G7).
    pub fn branch_log(&self, branch: &str) -> GitResult<Vec<CommitEntry>> {
        log::commits_for(&self.inner, branch, COMMITS_LIMIT)
    }

    /// Stage or unstage a whole file. No patch: `git add` / `git restore
    /// --staged`. See `docs/PLAN_6_STAGING.md`.
    pub fn stage_file(&self, path: &Path, dir: ApplyDir) -> GitResult<()> {
        apply::stage_file(&self.inner, path, dir)
    }

    /// Stage or unstage every changed file (`a`): `git add -A` / `git
    /// restore --staged .`.
    pub fn stage_all(&self, dir: ApplyDir) -> GitResult<()> {
        apply::stage_all(&self.inner, dir)
    }

    /// Discard a whole file's worktree change, never the index. `untracked`
    /// picks `git clean` (nothing to restore *to*) over `git restore
    /// --worktree`.
    pub fn discard_file(&self, path: &Path, untracked: bool) -> GitResult<()> {
        apply::discard_file(&self.inner, path, untracked)
    }

    /// Stage / unstage / discard one hunk. `patch` is `file.header.start
    /// .. hunk.body.end` over a `Diff::text`; the caller slices it so this
    /// module never re-runs the diff.
    pub fn apply_hunk(&self, patch: &str, dir: ApplyDir, target: ApplyTarget) -> GitResult<()> {
        apply::apply_hunk(&self.inner, patch, dir, target)
    }

    /// Stage / unstage / discard a set of body lines within one hunk.
    /// `lines` are 0-based indices into `hunk_body`'s own lines.
    pub fn apply_lines(
        &self,
        file_header: &str,
        hunk_header: &str,
        hunk_body: &str,
        lines: &[usize],
        dir: ApplyDir,
        target: ApplyTarget,
    ) -> GitResult<()> {
        apply::apply_lines(
            &self.inner,
            file_header,
            hunk_header,
            hunk_body,
            lines,
            dir,
            target,
        )
    }

    /// Run `git commit`. See `docs/PLAN_7_COMMIT.md`.
    pub fn commit(&self, kind: &CommitKind, message: &str, opts: CommitOpts) -> GitResult<String> {
        commit::commit(&self.inner, kind, message, opts)
    }

    /// `HEAD`'s current message, for pre-filling the Amend / Reword popup.
    pub fn head_message(&self) -> GitResult<Option<String>> {
        commit::head_message(&self.inner)
    }

    /// Count of paths staged relative to `HEAD`; the commit popup's
    /// precondition.
    pub fn staged_count(&self) -> GitResult<usize> {
        commit::staged_count(&self.inner)
    }
}
