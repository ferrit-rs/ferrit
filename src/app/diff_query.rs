//! What the keys do in `App` for `diff_query`: the glue between the interface, the git code and the app's state.

use crate::app::App;
use crate::app::error::AppError;
use crate::app::events::AppEvent;
use crate::app::workers::{WorkerKind, run_worker};
use crate::interface::state::diff_cursor::Mode;
use crate::interface::state::diff_query::DiffCompletion;
use crate::interface::state::diff_query::DiffQueryResult;
use crate::interface::state::diff_query::RightKey;
use crate::interface::state::diff_query::load;
use crate::interface::state::pane::Pane;
use crate::interface::state::tree::FileRow;
use crate::interface::state::views::{BranchLog, DiffView, FilesDiff};
use std::sync::mpsc;
use std::thread;

impl App {
    /// Rebuild `diff` from the current focus and selection. An image selection
    /// owns the right pane, so it clears the diff. A `Refresh` of an unchanged
    /// selection rebuilds the text but keeps `right_scroll`; a changed
    /// selection resets the scroll to the top.
    pub(crate) fn update_diff(&mut self) {
        if self.workers.image.path.is_some() {
            self.right.diff = DiffView::None;
            self.right.key = None;
            self.workers.diff.generation = self.workers.diff.generation.saturating_add(1);
            self.workers.diff.pending = None;
            self.nav.mode = Mode::Nav;
            return;
        }
        match self.right_key_for() {
            None => {
                self.right.diff = DiffView::None;
                self.right.key = None;
                self.workers.diff.generation = self.workers.diff.generation.saturating_add(1);
                self.workers.diff.pending = None;
                self.right.scroll = 0;
                self.nav.mode = Mode::Nav;
            },
            Some(key) if self.right.key.as_ref() == Some(&key) => {
                if std::mem::take(&mut self.workers.diff.refresh_requested) {
                    self.workers.diff.generation = self.workers.diff.generation.saturating_add(1);
                    self.right.diff = DiffView::Note("loading diff...".into());
                    self.queue_diff_query(key, self.workers.diff.generation);
                }
                self.right.clamp_scroll();
                self.resync_diff_cursor();
            },
            Some(key) => {
                self.right.scroll = 0;
                self.right.key = Some(key.clone());
                self.workers.diff.generation = self.workers.diff.generation.saturating_add(1);
                self.right.diff = DiffView::Note("loading diff...".into());
                self.workers.diff.refresh_requested = false;
                self.queue_diff_query(key, self.workers.diff.generation);
                self.nav.mode = Mode::Nav;
            },
        }
    }

    fn queue_diff_query(&mut self, key: RightKey, generation: u64) {
        let Some(sender) = self.workers.sender.clone() else {
            self.right.diff = self.build_diff(&key);
            return;
        };
        if self.repo.is_none() {
            self.right.diff = DiffView::None;
            return;
        }
        if self.workers.diff.in_flight {
            self.workers.diff.pending = Some((key, generation));
            return;
        }
        self.start_diff_query(sender, key, generation);
    }

    fn start_diff_query(&mut self, sender: mpsc::Sender<AppEvent>, key: RightKey, generation: u64) {
        let Some(handle) = self.reopen_repo() else {
            return;
        };
        self.workers.diff.in_flight = true;
        let opts = self.prefs.diff_opts();
        thread::spawn(move || {
            let result = run_worker(WorkerKind::Diff, || {
                handle.and_then(|repo| load(repo.as_ref(), &key, opts))
            })
            .map_err(AppError::from)
            .and_then(|result| result.map_err(AppError::from));
            let _ = sender.send(AppEvent::DiffDone(DiffCompletion {
                key,
                generation,
                result,
            }));
        });
    }

    pub(crate) fn on_diff_done(&mut self, completion: DiffCompletion) {
        let DiffCompletion {
            key,
            generation,
            result,
        } = completion;
        self.workers.diff.in_flight = false;
        if generation == self.workers.diff.generation && self.right.key.as_ref() == Some(&key) {
            self.right.diff = match result {
                Ok(result) => self.diff_view_from_query(&key, result),
                Err(error) => DiffView::Note(error.to_string()),
            };
            self.right.clamp_scroll();
            self.resync_diff_cursor();
        }
        if let Some((next_key, next_generation)) = self.workers.diff.pending.take()
            && next_generation == self.workers.diff.generation
            && self.right.key.as_ref() == Some(&next_key)
            && let Some(sender) = self.workers.sender.clone()
        {
            self.start_diff_query(sender, next_key, next_generation);
        }
    }

    /// The diff identity for the current focus and selection: a worktree /
    /// staged file for Files, a commit for Commits, nothing elsewhere.
    fn right_key_for(&self) -> Option<RightKey> {
        match self.nav.focus {
            Pane::Files => {
                let rows = self.rows().files_tree_rows();
                match rows.get(self.selected(Pane::Files))? {
                    FileRow::File { index, .. } => Some(RightKey::File {
                        path: self.snapshot.files.get(*index)?.path.clone(),
                    }),
                    // A directory row shows the diff of everything under it (`git diff --
                    // <dir>`), as lazygit does; the root row, an empty path, is every file.
                    FileRow::Dir { path, .. } => Some(RightKey::File { path: path.clone() }),
                }
            },
            // Drilled: the selection indexes the file tree, not `self.snapshot.commits`
            // (`commit_tree_rows`), so the diff stays keyed on the drilled
            // commit's own hash regardless of which file row is highlighted.
            Pane::Commits => {
                if let Some(drill) = &self.nav.commit_drill {
                    Some(RightKey::Commit {
                        full_hash: drill.hash.clone(),
                    })
                } else {
                    let entry = self.snapshot.commits.get(self.selected(Pane::Commits))?;
                    Some(RightKey::Commit {
                        full_hash: entry.full_hash.clone(),
                    })
                }
            },
            // Drilled: the selected row is a commit, same as Commits. Not
            // drilled: no Enter yet, so preview the selected branch's own
            // log passively (lazygit's live branch -> log, no key needed).
            Pane::Branches => {
                if let Some(drill) = &self.nav.branch_drill {
                    let entry = drill.commits.get(self.selected(Pane::Branches))?;
                    Some(RightKey::Commit {
                        full_hash: entry.full_hash.clone(),
                    })
                } else {
                    let entry = self.snapshot.branches.get(self.selected(Pane::Branches))?;
                    Some(RightKey::BranchLog {
                        branch: entry.name.clone(),
                    })
                }
            },
            Pane::Stash => {
                let entry = self.snapshot.stashes.get(self.selected(Pane::Stash))?;
                Some(RightKey::Stash {
                    oid: entry.oid.clone(),
                    header: format!("stash@{{{}}}: {}", entry.index, entry.message),
                })
            },
            Pane::Status => None,
        }
    }

    /// Run the diff read for `key`; worker and eventless paths share this
    /// function so Git errors have identical UI treatment.
    fn build_diff(&self, key: &RightKey) -> DiffView {
        let Some(repo) = &self.repo else {
            return DiffView::None;
        };
        match load(repo.as_ref(), key, self.prefs.diff_opts()) {
            Ok(result) => self.diff_view_from_query(key, result),
            Err(error) => DiffView::Note(error.to_string()),
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
                let drill_commits = self.nav.branch_drill.iter().flat_map(|d| d.commits.iter());
                match self
                    .snapshot
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
            (RightKey::Stash { oid, .. }, DiffQueryResult::Stash(diff)) => {
                match self.snapshot.stashes.iter().find(|entry| &entry.oid == oid) {
                    Some(entry) => DiffView::Stash(entry.clone(), diff),
                    None => DiffView::Note("stash entry no longer exists".into()),
                }
            },
            _ => DiffView::Note("diff result did not match selection".into()),
        }
    }
}
