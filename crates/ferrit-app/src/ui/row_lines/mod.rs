//! Styled text renderers used by TUI panes and right-pane views.
//!
//! Public functions live under explicit rendering namespaces. List rows,
//! diffs and status output evolve independently.

use ratatui::style::{Color, Style};

const META: &[&str] = &[
    "diff --git",
    "index ",
    "--- ",
    "+++ ",
    "old mode",
    "new mode",
    "new file",
    "deleted file",
    "rename ",
    "copy ",
    "similarity ",
    "dissimilarity ",
    "commit ",
    "Author:",
    "AuthorDate:",
    "Commit:",
    "CommitDate:",
    "Date:",
    "Merge:",
];

fn fg(color: Color) -> Style {
    Style::new().fg(color)
}

pub mod diff;
pub mod rows;
pub mod status;
