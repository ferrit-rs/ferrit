//! What the keys do in `App` for `image_query`: the glue between the interface, the git code and the app's state.

use crate::app::error::AppError;
use crate::app::events::AppEvent;
use crate::app::workers::{WorkerKind, run_worker};
use crate::app::{App, mock};
use crate::git;
use crate::git::image::preview;
use crate::git::image::preview::Preview;
use crate::interface::state::image_query::ImageCompletion;
use crate::interface::state::image_query::load;
use crate::interface::state::pane::Pane;
use crate::interface::state::tree::FileRow;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;

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
            let result = run_worker(WorkerKind::Image, || load(handle, &path))
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
