//! The right column: `queries`.

use crate::ui::error::AppError;
use ferrit_domain::diff::{DiffOpts, DiffSide};
use ferrit_domain::error::{GitError, GitResult};
use ferrit_domain::port::GitPort;
use std::path::{Path, PathBuf};

#[derive(Default)]
pub(crate) struct DiffQueryState {
    pub(crate) in_flight: bool,
    pub(crate) pending: Option<(RightKey, u64)>,
    pub(crate) generation: u64,
    pub(crate) refresh_requested: bool,
}

/// Identity of selected right-pane content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RightKey {
    File { path: PathBuf },
    Commit { full_hash: String },
    BranchLog { branch: String },
    Stash { oid: String, header: String },
}

#[derive(Debug)]
pub(crate) enum DiffQueryResult {
    File {
        unstaged: ferrit_domain::diff::Diff,
        staged: ferrit_domain::diff::Diff,
    },
    Commit(ferrit_domain::diff::Diff),
    BranchLog(Vec<ferrit_domain::model::CommitEntry>),
    Stash(ferrit_domain::diff::Diff),
}

/// Hidden event payload for a selected-diff worker completion.
#[doc(hidden)]
#[derive(Debug)]
pub struct DiffCompletion {
    pub(crate) key: RightKey,
    pub(crate) generation: u64,
    pub(crate) result: Result<DiffQueryResult, AppError>,
}

pub(crate) fn load_diff(
    repo: &dyn GitPort,
    key: &RightKey,
    opts: DiffOpts,
) -> Result<DiffQueryResult, GitError> {
    match key {
        RightKey::File { path } => {
            // The root directory row has an empty path: `git diff -- .` is every file.
            let path = if path.as_os_str().is_empty() {
                Path::new(".")
            } else {
                path.as_path()
            };
            Ok(DiffQueryResult::File {
                unstaged: repo.file_diff(path, DiffSide::Worktree, opts)?,
                staged: repo.file_diff(path, DiffSide::Staged, opts)?,
            })
        },
        RightKey::Commit { full_hash } => repo
            .commit_diff(full_hash, opts)
            .map(DiffQueryResult::Commit),
        RightKey::BranchLog { branch } => repo.branch_log(branch).map(DiffQueryResult::BranchLog),
        RightKey::Stash { oid, header } => repo
            .stash_diff(oid, header, opts)
            .map(DiffQueryResult::Stash),
    }
}

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
    Open(#[from] GitError),
    #[error("[image] {}  ({source})", .path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: GitError,
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

pub(crate) fn load_image(
    repo: GitResult<Box<dyn GitPort>>,
    image_path: &Path,
) -> Result<::image::DynamicImage, ImageError> {
    let repo = repo?;
    let bytes = repo
        .blob_bytes(image_path, ferrit_domain::diff::Rev::Workdir)
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
