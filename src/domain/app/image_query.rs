//! Background blob reads and image decoding for file previews.

use std::path::{Path, PathBuf};
use std::sync::mpsc;

use super::{
    App, AppEvent, FileRow, Pane, Preview, WorkerKind, git, mock, preview, run_worker, thread,
};

/// Image worker result, applied only if selection and generation still match.
#[doc(hidden)]
#[derive(Debug)]
pub struct ImageCompletion {
    pub(crate) path: PathBuf,
    pub(crate) generation: u64,
    pub(crate) result: Result<::image::DynamicImage, String>,
}

pub(crate) fn load(repo_path: &Path, image_path: &Path) -> Result<::image::DynamicImage, String> {
    let repo = git::Repo::open(repo_path).map_err(|error| error.to_string())?;
    let bytes = repo
        .blob_bytes(image_path, git::blob::Rev::Workdir)
        .map_err(|error| format!("[image] {}  ({error})", image_path.display()))?;
    if bytes.is_empty() {
        return Err(format!("[image] {}  (no bytes)", image_path.display()));
    }
    ::image::load_from_memory(&bytes).map_err(|error| {
        format!(
            "[image] {}  ({} bytes)  decode failed: {error}",
            image_path.display(),
            bytes.len()
        )
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
        let Some(entry) = self.files.get(*index) else {
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
        if let (Some(sender), Some(repo_path)) = (
            self.event_sender.clone(),
            self.repo
                .as_ref()
                .map(|repo| repo.reopen_path().to_path_buf()),
        ) {
            self.preview = Preview::Note("loading image...".into());
            self.queue_image_query(sender, repo_path, path, generation);
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
        repo_path: PathBuf,
        path: PathBuf,
        generation: u64,
    ) {
        if self.image_query.in_flight {
            self.image_query.pending = Some((path, generation));
            return;
        }
        self.start_image_query(sender, repo_path, path, generation);
    }

    fn start_image_query(
        &mut self,
        sender: mpsc::Sender<AppEvent>,
        repo_path: PathBuf,
        path: PathBuf,
        generation: u64,
    ) {
        self.image_query.in_flight = true;
        thread::spawn(move || {
            let result = run_worker(WorkerKind::ImagePreview, || load(&repo_path, &path))
                .map_err(|error| error.to_string())
                .and_then(|result| result);
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
                Err(error) => Preview::Note(error),
            };
            self.update_diff();
        }
        if let Some((path, generation)) = self.image_query.pending.take()
            && generation == self.image_query.generation
            && self.image_query.path.as_ref() == Some(&path)
            && let (Some(sender), Some(repo_path)) = (
                self.event_sender.clone(),
                self.repo
                    .as_ref()
                    .map(|repo| repo.reopen_path().to_path_buf()),
            )
        {
            self.start_image_query(sender, repo_path, path, generation);
        }
    }
}
