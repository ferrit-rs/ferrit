//! Owned request/result types and Git reads for selected right-pane previews.

use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;

use super::{App, AppEvent, BranchLog, DiffView, FileRow, FilesDiff, Mode, Pane, run_worker};

use crate::git;
use crate::git::diff::{DiffOpts, DiffSide};

#[derive(Default)]
pub(super) struct DiffQueryState {
    pub(super) in_flight: bool,
    pub(super) pending: Option<(RightKey, u64)>,
    pub(super) generation: u64,
    pub(super) refresh_requested: bool,
}

/// Identity of selected right-pane content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RightKey {
    File { path: PathBuf },
    Commit { full_hash: String },
    BranchLog { branch: String },
}

#[derive(Debug)]
pub(crate) enum DiffQueryResult {
    File {
        unstaged: git::diff::Diff,
        staged: git::diff::Diff,
    },
    Commit(git::diff::Diff),
    BranchLog(Vec<git::model::CommitEntry>),
}

/// Hidden event payload for a selected-diff worker completion.
#[doc(hidden)]
#[derive(Debug)]
pub struct DiffCompletion {
    pub(crate) key: RightKey,
    pub(crate) generation: u64,
    pub(crate) result: Result<DiffQueryResult, String>,
}

pub(crate) fn load(repo: &git::Repo, key: &RightKey) -> Result<DiffQueryResult, String> {
    let opts = DiffOpts::default();
    match key {
        RightKey::File { path } => Ok(DiffQueryResult::File {
            unstaged: repo
                .file_diff(path, DiffSide::Worktree, opts)
                .map_err(|error| error.to_string())?,
            staged: repo
                .file_diff(path, DiffSide::Staged, opts)
                .map_err(|error| error.to_string())?,
        }),
        RightKey::Commit { full_hash } => repo
            .commit_diff(full_hash, opts)
            .map(DiffQueryResult::Commit)
            .map_err(|error| error.to_string()),
        RightKey::BranchLog { branch } => repo
            .branch_log(branch)
            .map(DiffQueryResult::BranchLog)
            .map_err(|error| error.to_string()),
    }
}

impl App {
    /// Rebuild `diff` from the current focus and selection. An image selection
    /// owns the right pane, so it clears the diff. A `Refresh` of an unchanged
    /// selection rebuilds the text but keeps `right_scroll`; a changed
    /// selection resets the scroll to the top.
    pub(super) fn update_diff(&mut self) {
        if self.image_query.path.is_some() {
            self.diff = DiffView::None;
            self.right_key = None;
            self.diff_query.generation = self.diff_query.generation.saturating_add(1);
            self.diff_query.pending = None;
            self.mode = Mode::Nav;
            return;
        }
        match self.right_key_for() {
            None => {
                self.diff = DiffView::None;
                self.right_key = None;
                self.diff_query.generation = self.diff_query.generation.saturating_add(1);
                self.diff_query.pending = None;
                self.right_scroll = 0;
                self.mode = Mode::Nav;
            },
            Some(key) if self.right_key.as_ref() == Some(&key) => {
                if std::mem::take(&mut self.diff_query.refresh_requested) {
                    self.diff_query.generation = self.diff_query.generation.saturating_add(1);
                    self.diff = DiffView::Note("loading diff...".into());
                    self.queue_diff_query(key, self.diff_query.generation);
                }
                self.clamp_right_scroll();
                self.resync_diff_cursor();
            },
            Some(key) => {
                self.right_scroll = 0;
                self.right_key = Some(key.clone());
                self.diff_query.generation = self.diff_query.generation.saturating_add(1);
                self.diff = DiffView::Note("loading diff...".into());
                self.diff_query.refresh_requested = false;
                self.queue_diff_query(key, self.diff_query.generation);
                self.mode = Mode::Nav;
            },
        }
    }

    fn queue_diff_query(&mut self, key: RightKey, generation: u64) {
        let Some(sender) = self.event_sender.clone() else {
            self.diff = self.build_diff(&key);
            return;
        };
        let Some(path) = self
            .repo
            .as_ref()
            .map(|repo| repo.reopen_path().to_path_buf())
        else {
            self.diff = DiffView::None;
            return;
        };
        if self.diff_query.in_flight {
            self.diff_query.pending = Some((key, generation));
            return;
        }
        self.start_diff_query(sender, path, key, generation);
    }

    fn start_diff_query(
        &mut self,
        sender: mpsc::Sender<AppEvent>,
        path: PathBuf,
        key: RightKey,
        generation: u64,
    ) {
        self.diff_query.in_flight = true;
        thread::spawn(move || {
            let result = run_worker("diff", || {
                git::Repo::open(&path)
                    .map_err(|error| error.to_string())
                    .and_then(|repo| load(&repo, &key))
            })
            .and_then(|result| result);
            let _ = sender.send(AppEvent::DiffDone(DiffCompletion {
                key,
                generation,
                result,
            }));
        });
    }

    pub(super) fn on_diff_done(&mut self, completion: DiffCompletion) {
        let DiffCompletion {
            key,
            generation,
            result,
        } = completion;
        self.diff_query.in_flight = false;
        if generation == self.diff_query.generation && self.right_key.as_ref() == Some(&key) {
            self.diff = match result {
                Ok(result) => self.diff_view_from_query(&key, result),
                Err(error) => DiffView::Note(error),
            };
            self.clamp_right_scroll();
            self.resync_diff_cursor();
        }
        if let Some((next_key, next_generation)) = self.diff_query.pending.take()
            && next_generation == self.diff_query.generation
            && self.right_key.as_ref() == Some(&next_key)
            && let (Some(sender), Some(path)) = (
                self.event_sender.clone(),
                self.repo
                    .as_ref()
                    .map(|repo| repo.reopen_path().to_path_buf()),
            )
        {
            self.start_diff_query(sender, path, next_key, next_generation);
        }
    }

    /// The diff identity for the current focus and selection: a worktree /
    /// staged file for Files, a commit for Commits, nothing elsewhere.
    fn right_key_for(&self) -> Option<RightKey> {
        match self.focus {
            Pane::Files => {
                let rows = self.files_tree_rows();
                let FileRow::File { index, .. } = rows.get(self.selected(Pane::Files))? else {
                    return None;
                };
                let entry = self.files.get(*index)?;
                Some(RightKey::File {
                    path: entry.path.clone(),
                })
            },
            // Drilled: the selection indexes the file tree, not `self.commits`
            // (`commit_tree_rows`), so the diff stays keyed on the drilled
            // commit's own hash regardless of which file row is highlighted.
            Pane::Commits => {
                if let Some(drill) = &self.commit_drill {
                    Some(RightKey::Commit {
                        full_hash: drill.hash.clone(),
                    })
                } else {
                    let entry = self.commits.get(self.selected(Pane::Commits))?;
                    Some(RightKey::Commit {
                        full_hash: entry.full_hash.clone(),
                    })
                }
            },
            // Drilled: the selected row is a commit, same as Commits. Not
            // drilled: no Enter yet, so preview the selected branch's own
            // log passively (lazygit's live branch -> log, no key needed).
            Pane::Branches => {
                if let Some(drill) = &self.branch_drill {
                    let entry = drill.commits.get(self.selected(Pane::Branches))?;
                    Some(RightKey::Commit {
                        full_hash: entry.full_hash.clone(),
                    })
                } else {
                    let entry = self.branches.get(self.selected(Pane::Branches))?;
                    Some(RightKey::BranchLog {
                        branch: entry.name.clone(),
                    })
                }
            },
            _ => None,
        }
    }

    /// Run the diff read for `key`; worker and eventless paths share this
    /// function so Git errors have identical UI treatment.
    fn build_diff(&self, key: &RightKey) -> DiffView {
        let Some(repo) = &self.repo else {
            return DiffView::None;
        };
        match load(repo, key) {
            Ok(result) => self.diff_view_from_query(key, result),
            Err(error) => DiffView::Note(error),
        }
    }

    fn diff_view_from_query(&self, key: &RightKey, result: DiffQueryResult) -> DiffView {
        match (key, result) {
            (RightKey::File { .. }, DiffQueryResult::File { unstaged, staged })
                if unstaged.files.is_empty() && staged.files.is_empty() =>
            {
                DiffView::Note("no changes to show".into())
            },
            (RightKey::File { .. }, DiffQueryResult::File { unstaged, staged }) => {
                DiffView::Files(FilesDiff { unstaged, staged })
            },
            (RightKey::Commit { full_hash }, DiffQueryResult::Commit(diff)) => {
                let drill_commits = self.branch_drill.iter().flat_map(|d| d.commits.iter());
                match self
                    .commits
                    .iter()
                    .chain(drill_commits)
                    .find(|commit| &commit.full_hash == full_hash)
                {
                    Some(entry) => DiffView::Commit(entry.clone(), diff),
                    None => DiffView::Note("commit not in the list".into()),
                }
            },
            (RightKey::BranchLog { branch }, DiffQueryResult::BranchLog(commits)) => {
                DiffView::BranchLog(BranchLog {
                    branch: branch.clone(),
                    commits,
                })
            },
            _ => DiffView::Note("diff result did not match selection".into()),
        }
    }
}
