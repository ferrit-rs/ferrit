//! The result of a repository refresh, as it comes back from its worker.

use super::error::AppError;
use crate::domain::git;
use crate::domain::profile::Profile;
use color_eyre::Result;
use std::sync::Arc;

/// Snapshot plus any active drill-down data loaded in the same worker.
/// A result whose error is shared between the Status line, the toast and the
/// refresh bookkeeping, none of which can own it alone.
pub(super) type Shared<T> = Result<T, Arc<AppError>>;

#[doc(hidden)]
#[derive(Debug)]
pub struct RefreshCompletion {
    pub(crate) snapshot: Shared<git::Snapshot>,
    pub(crate) profile: Option<Profile>,
    pub(crate) branch_log: Option<(String, Shared<Vec<git::model::CommitEntry>>)>,
    pub(crate) commit_files: Option<(String, Shared<Vec<git::model::FileEntry>>)>,
}
