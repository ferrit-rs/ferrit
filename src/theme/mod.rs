//! How ferrit looks: the colour palette every screen is painted with, the
//! terminal colour depth it is approximated for, the `[theme]` section of
//! `config.toml`, the colour picker, and the state of the theme being edited
//! in the settings sheet. Knows nothing of git or of what is drawn with it.

pub(crate) mod color_picker;
pub mod config;
pub mod editor;
pub mod palette;
pub mod scheme;
pub(crate) mod style;
