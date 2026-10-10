//! Terminal image support for the TUI right pane.
//!
//! Git only supplies image bytes. Terminal protocol detection and decoding live
//! here because they depend on ratatui-image and terminal capabilities.

pub(crate) mod detect;
pub mod preview;
