//! Right-pane image preview.
//!
//! This is the only domain that touches `image` or `ratatui-image`; `src/git/`
//! stays clean of both and just hands over bytes. When the terminal speaks a
//! graphics protocol (sixel / kitty / iterm2) the picture renders natively;
//! otherwise `ratatui-image` falls back to unicode half-blocks, which work
//! anywhere, so there is always something to show.
//!
//! `detect` picks the `Picker`; `preview` turns bytes into a `Preview`. See
//! `docs/BUG_IMAGE_PREVIEW.md` for the terminal-detection bug this split grew
//! out of.

pub mod detect;
pub mod preview;
