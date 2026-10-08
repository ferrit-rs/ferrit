//! Background blob reads and image decoding for file previews.

use std::path::{Path, PathBuf};
use std::sync::mpsc;

use super::{
    App, AppError, AppEvent, FileRow, Pane, Preview, WorkerKind, git, mock, preview, run_worker,
    thread,
};
use crate::domain::git::error::GitResult;
use crate::domain::git::port::GitPort;

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
        if self.focus != Pane::Files {
            self.invalidate_image_query();
            self.preview = Preview::None;
            return;
        }
        let rows = self.files_tree_rows();
        let Some(FileRow::File { index, .. }) = rows.get(self.selected(Pane::Files)) else {
            self.invalidate_image_query();
            self.preview = Preview::None;
            return;
        };
        let Some(entry) = self.snapshot.files.get(*index) else {
            self.invalidate_image_query();
            self.preview = Preview::None;
            return;
        };
        if !preview::is_image_path(&entry.path) {
            self.invalidate_image_query();
            self.preview = Preview::None;
            return;
        }
        let path = entry.path.clone();
        if self.image_query.path.as_ref() == Some(&path) {
            return;
        }
        self.image_query.generation = self.image_query.generation.saturating_add(1);
        let generation = self.image_query.generation;
        self.image_query.path = Some(path.clone());
        if let (Some(sender), true) = (self.event_sender.clone(), self.repo.is_some()) {
            self.preview = Preview::Note("loading image...".into());
            self.queue_image_query(sender, path, generation);
        } else {
            let bytes = match &self.repo {
                Some(repo) => match repo.blob_bytes(&path, git::blob::Rev::Workdir) {
                    Ok(bytes) => bytes,
                    Err(error) => {
                        self.preview =
                            Preview::Note(format!("[image] {}  ({error})", path.display()));
                        return;
                    },
                },
                None => mock::mock_image_bytes(&path)
                    .map(<[u8]>::to_vec)
                    .unwrap_or_default(),
            };
            self.preview = preview::from_bytes(&self.picker, &path, &bytes);
        }
    }

    pub(super) fn invalidate_image_query(&mut self) {
        if self.image_query.path.take().is_some() {
            self.image_query.generation = self.image_query.generation.saturating_add(1);
        }
    }

    fn queue_image_query(
        &mut self,
        sender: mpsc::Sender<AppEvent>,
        path: PathBuf,
        generation: u64,
    ) {
        if self.image_query.in_flight {
            self.image_query.pending = Some((path, generation));
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
        self.image_query.in_flight = true;
        thread::spawn(move || {
            let result = run_worker(WorkerKind::ImagePreview, || load(handle, &path))
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
        self.image_query.in_flight = false;
        if completion.generation == self.image_query.generation
            && self.image_query.path.as_ref() == Some(&completion.path)
        {
            self.preview = match completion.result {
                Ok(image) => Preview::Image(Box::new(self.picker.new_resize_protocol(image))),
                Err(error) => Preview::Note(error.to_string()),
            };
            self.update_diff();
        }
        if let Some((path, generation)) = self.image_query.pending.take()
            && generation == self.image_query.generation
            && self.image_query.path.as_ref() == Some(&path)
            && let Some(sender) = self.event_sender.clone()
        {
            self.start_image_query(sender, path, generation);
        }
    }
}
