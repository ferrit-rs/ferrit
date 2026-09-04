//! Which `Picker` (graphics protocol) to render images with.
//!
//! See `docs/BUG_IMAGE_PREVIEW.md`: some terminal hosts answer the capability
//! query with a protocol they then don't actually draw, so a query success is
//! not proof the picture will show up.

use ratatui_image::picker::Picker;

/// Query the real terminal for a graphics protocol. Returns `None` when the
/// query fails, or when the host is known to lie about support, so the
/// caller keeps its half-block fallback (always draws, everywhere).
///
/// Call once, before entering the alternate screen, same as the ratatui-image
/// examples: `Picker::from_query_stdio` reads and writes stdio momentarily.
pub fn detect_picker() -> Option<Picker> {
    if is_known_liar() {
        return None;
    }
    Picker::from_query_stdio().ok()
}

/// VS Code's integrated terminal answers the iTerm2 capability query but
/// doesn't draw the protocol: `from_query_stdio` reports a working graphics
/// backend that then renders nothing. Real terminals (Terminal.app, iTerm2,
/// Kitty, WezTerm...) are unaffected.
fn is_known_liar() -> bool {
    std::env::var_os("TERM_PROGRAM").as_deref() == Some(std::ffi::OsStr::new("vscode"))
}
