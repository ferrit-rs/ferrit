//! What the interface remembers: the panes (where the cursor is, what the right
//! column shows, where the last frame put the clickable things), what can sit
//! over them (a popup, a question, a menu), the side sheets, the help and the
//! full-screen views. Drawing it is `screens`; the widgets are `components`.

pub(crate) mod askpass;
pub(crate) mod commit_draft;
pub(crate) mod confirm;
pub mod context_menu;
pub mod create_remote_form;
pub mod dashboard;
pub(crate) mod diff_cursor;
pub mod diff_query;
pub(crate) mod drill;
pub mod full_screens;
pub mod git_config;
pub mod help;
pub(crate) mod hit_areas;
pub mod image_query;
pub mod menu;
pub(crate) mod modal;
pub mod nav;
pub mod pane;
pub(crate) mod pane_rows;
pub(crate) mod popup;
pub mod popup_keys;
pub(crate) mod render_state;
pub(crate) mod right_pane;
pub mod selection;
pub(crate) mod settings_hits;
pub mod sheet;
pub(crate) mod tree;
pub mod views;
