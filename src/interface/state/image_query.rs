//! Background blob reads and image decoding for file previews.

use std::path::{Path, PathBuf};

use crate::app::error::AppError;
use crate::git;
use crate::git::error::GitResult;
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
