//! Terminal colors, palettes and persisted theme settings.
//!
//! This crate owns theme data and rendering-independent color policy. The
//! The reusable `termui` crate owns interactive color-picker widgets; config
//! loading owns the file, not the theme model.

pub mod palette;
pub mod scheme;
pub mod theme_config;
