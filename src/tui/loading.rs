//! Loading what the right column shows without stalling the UI: the diff of the
//! selection and the preview of an image, each read on a worker and applied when
//! it answers, a stale answer dropped. A flow across the selection, the workers,
//! the repository and the right pane, so it is written on `App`.

use crate::git::image::preview;
use crate::git::image::preview::Preview;
use crate::tui::App;
use crate::tui::components::diff::BranchLog;
use crate::tui::components::diff::DiffCompletion;
use crate::tui::components::diff::DiffQueryResult;
use crate::tui::components::diff::DiffView;
use crate::tui::components::diff::FilesDiff;
use crate::tui::components::diff::ImageCompletion;
use crate::tui::components::diff::Mode;
use crate::tui::components::diff::RightKey;
use crate::tui::components::diff::load_diff;
use crate::tui::components::diff::load_image;
use crate::tui::components::panes::FileRow;
use crate::tui::components::panes::Pane;
use crate::tui::error::AppError;
use crate::tui::events::AppEvent;
use crate::tui::mock;
use crate::tui::workers::WorkerKind;
use crate::tui::workers::run_worker;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;

use crate::git;

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
                handle.and_then(|repo| load_diff(repo.as_ref(), &key, opts))
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
        match load_diff(repo.as_ref(), key, self.prefs.diff_opts()) {
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

impl App {
    pub(crate) fn update_preview(&mut self) {
        if self.nav.focus != Pane::Files {
            self.invalidate_image_query();
            self.render.preview = Preview::None;
            return;
        }
        let rows = self.rows().files_tree_rows();
        let Some(FileRow::File { index, .. }) = rows.get(self.selected(Pane::Files)) else {
            self.invalidate_image_query();
            self.render.preview = Preview::None;
            return;
        };
        let Some(entry) = self.snapshot.files.get(*index) else {
            self.invalidate_image_query();
            self.render.preview = Preview::None;
            return;
        };
        if !preview::is_image_path(&entry.path) {
            self.invalidate_image_query();
            self.render.preview = Preview::None;
            return;
        }
        let path = entry.path.clone();
        if self.workers.image.path.as_ref() == Some(&path) {
            return;
        }
        self.workers.image.generation = self.workers.image.generation.saturating_add(1);
        let generation = self.workers.image.generation;
        self.workers.image.path = Some(path.clone());
        if let (Some(sender), true) = (self.workers.sender.clone(), self.repo.is_some()) {
            self.render.preview = Preview::Note("loading image...".into());
            self.queue_image_query(sender, path, generation);
        } else {
            let bytes = match &self.repo {
                Some(repo) => match repo.blob_bytes(&path, git::blob::Rev::Workdir) {
                    Ok(bytes) => bytes,
                    Err(error) => {
                        self.render.preview =
                            Preview::Note(format!("[image] {}  ({error})", path.display()));
                        return;
                    },
                },
                None => mock::mock_image_bytes(&path)
                    .map(<[u8]>::to_vec)
                    .unwrap_or_default(),
            };
            self.render.preview = preview::from_bytes(&self.right.picker, &path, &bytes);
        }
    }

    pub(crate) fn invalidate_image_query(&mut self) {
        if self.workers.image.path.take().is_some() {
            self.workers.image.generation = self.workers.image.generation.saturating_add(1);
        }
    }

    fn queue_image_query(
        &mut self,
        sender: mpsc::Sender<AppEvent>,
        path: PathBuf,
        generation: u64,
    ) {
        if self.workers.image.in_flight {
            self.workers.image.pending = Some((path, generation));
            return;
        }
        self.start_image_query(sender, path, generation);
    }

    fn start_image_query(
        &mut self,
        sender: mpsc::Sender<AppEvent>,
        path: PathBuf,
        generation: u64,
    ) {
        let Some(handle) = self.reopen_repo() else {
            return;
        };
        self.workers.image.in_flight = true;
        thread::spawn(move || {
            let result = run_worker(WorkerKind::Image, || load_image(handle, &path))
                .map_err(AppError::from)
                .and_then(|result| result.map_err(AppError::from));
            let _ = sender.send(AppEvent::ImageDone(ImageCompletion {
                path,
                generation,
                result,
            }));
        });
    }

    pub(crate) fn on_image_done(&mut self, completion: ImageCompletion) {
        self.workers.image.in_flight = false;
        if completion.generation == self.workers.image.generation
            && self.workers.image.path.as_ref() == Some(&completion.path)
        {
            self.render.preview = match completion.result {
                Ok(image) => Preview::Image(Box::new(self.right.picker.new_resize_protocol(image))),
                Err(error) => Preview::Note(error.to_string()),
            };
            self.update_diff();
        }
        if let Some((path, generation)) = self.workers.image.pending.take()
            && generation == self.workers.image.generation
            && self.workers.image.path.as_ref() == Some(&path)
            && let Some(sender) = self.workers.sender.clone()
        {
            self.start_image_query(sender, path, generation);
        }
    }
}
