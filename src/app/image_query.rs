//! Background blob reads and image decoding for file previews.

use std::path::{Path, PathBuf};
use std::sync::mpsc;

use super::{App, git, mock, thread};
use crate::app::error::AppError;
use crate::app::events::AppEvent;
use crate::app::pane::Pane;
use crate::app::tree::FileRow;
use crate::app::workers::{WorkerKind, run_worker};
use crate::git::error::GitResult;
use crate::git::image::preview;
use crate::git::image::preview::Preview;
use crate::git::port::GitPort;

/// Image worker result, applied only if selection and generation still match.
#[doc(hidden)]
#[derive(Debug)]
pub struct ImageCompletion {
    pub(crate) path: PathBuf,
    pub(crate) generation: u64,
    pub(crate) result: Result<::image::DynamicImage, AppError>,
}

/// Why a file preview could not show an image.
#[derive(Debug, thiserror::Error)]
pub enum ImageError {
    #[error(transparent)]
    Open(#[from] git::error::GitError),
    #[error("[image] {}  ({source})", .path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: git::error::GitError,
    },
    #[error("[image] {}  (no bytes)", .path.display())]
    Empty { path: PathBuf },
    #[error("[image] {}  ({bytes} bytes)  decode failed: {source}", .path.display())]
    Decode {
        path: PathBuf,
        bytes: usize,
        #[source]
        source: ::image::ImageError,
    },
}

pub(crate) fn load(
    repo: GitResult<Box<dyn GitPort>>,
    image_path: &Path,
) -> Result<::image::DynamicImage, ImageError> {
    let repo = repo?;
    let bytes = repo
        .blob_bytes(image_path, git::blob::Rev::Workdir)
        .map_err(|source| ImageError::Read {
            path: image_path.to_path_buf(),
            source,
        })?;
    if bytes.is_empty() {
        return Err(ImageError::Empty {
            path: image_path.to_path_buf(),
        });
    }
    ::image::load_from_memory(&bytes).map_err(|source| ImageError::Decode {
        path: image_path.to_path_buf(),
        bytes: bytes.len(),
        source,
    })
}

impl App {
    pub(super) fn update_preview(&mut self) {
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

    pub(super) fn invalidate_image_query(&mut self) {
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

    pub(super) fn on_image_done(&mut self, completion: ImageCompletion) {
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
