//! Copyable, reusable Ratatui components and terminal UI state.
//!
//! Components render themselves but do not own application actions. The
//! parent app owns focus, events and domain state.

pub mod state;
pub mod widgets;
