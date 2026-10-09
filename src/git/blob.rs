//! Raw bytes of a path, either from the working directory or from HEAD.
//!
//! The right pane's image preview (see `src/git/image/preview.rs`) is the first consumer;
//! diffs in phase 3 will be the second. No `git2` type escapes this module.
//!
//! This file holds the types and the pure functions. The code that reads with
//! `git2` or runs `git` is `crate::git::repo::read`.

/// Which version of a path's bytes to read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rev {
    /// The file as it currently sits in the working directory.
    Workdir,
    /// The blob recorded in HEAD's tree.
    Head,
}
