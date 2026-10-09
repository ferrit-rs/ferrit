//! The five left panes and the right column: what each shows, where the cursor
//! is, and where the last frame put the clickable things.

pub(crate) mod diff_cursor;
pub mod diff_query;
pub(crate) mod drill;
pub mod drill_nav;
pub(crate) mod hit_areas;
pub mod image_query;
pub mod nav;
pub mod pane;
pub(crate) mod pane_rows;
pub(crate) mod right_pane;
pub mod row_lines;
pub mod selection;
pub(crate) mod tree;
pub mod views;
