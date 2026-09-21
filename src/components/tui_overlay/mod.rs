//! Composable overlay widget for [Ratatui]: drawers, modals, popovers, and toasts
//! from a single configurable primitive.
//!
//! # Quick start
//!
//! ```rust
//! use std::time::Duration;
//! use ratatui_core::layout::Constraint;
//! use ferrit::components::tui_overlay::{Anchor, Easing, Overlay, OverlayState, Slide};
//!
//! let overlay = Overlay::new()
//!     .anchor(Anchor::Right)
//!     .slide(Slide::Right)
//!     .width(Constraint::Percentage(30));
//!
//! let mut state = OverlayState::new()
//!     .with_duration(Duration::from_millis(150))
//!     .with_easing(Easing::EaseOut);
//!
//! state.open();
//! ```
//!
//! # Rendering
//!
//! Render main content first, then the overlay (which dims and draws chrome),
//! then body content into the exposed inner area.
//!
//! ```rust,ignore
//! // 1. Main UI
//! frame.render_widget(dashboard, area);
//!
//! // 2. Overlay (backdrop + chrome)
//! frame.render_stateful_widget(overlay, area, &mut state);
//!
//! // 3. Body content into the overlay
//! if let Some(inner) = state.inner_area() {
//!     frame.render_widget(detail_panel, inner);
//! }
//! ```
//!
//! Call [`OverlayState::tick`] with elapsed time each frame to drive slide animation.
//!
//! For click-outside-to-dismiss, use [`OverlayState::overlay_rect`] for hit testing.
//!
//! [Ratatui]: https://ratatui.rs

mod anchor;
mod backdrop;
mod easing;
mod layout;
mod overlay;
mod slide;
mod state;

pub use anchor::Anchor;
pub use backdrop::Backdrop;
pub use easing::Easing;
pub use overlay::Overlay;
pub use slide::Slide;
pub use state::OverlayState;
